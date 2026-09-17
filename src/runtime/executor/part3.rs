#![allow(
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::needless_return,
    clippy::ignored_unit_patterns,
    clippy::redundant_closure
)]
use super::*;

impl Executor<'_, '_> {
    pub(crate) fn call_named(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if let Some(rest) = name.strip_prefix("HOST.") {
            if let Some((capability, member)) = rest.split_once('.')
                && self.hosts.contains_key(capability)
            {
                return self.host_provider_call(capability, member, arguments, span);
            }
            return Self::host_call(name, &arguments, span);
        }
        if is_host_file_method(name) {
            // `<alias>.File.<method>` on a `Value::File` receiver.
            let member = format!("File.{}", name.rsplit('.').next().unwrap_or_default());
            return self.host_provider_call("FileSystem", &member, arguments, span);
        }
        if let Some((library, member)) = self.library_callee(name) {
            return self.library_call(library, member, arguments, span);
        }
        if is_temporal_builtin(name) {
            return temporal_call(name, &arguments, span);
        }
        if matches!(name, "ASC" | "CHAR" | "TOLOWER" | "TOUPPER") || name == "$for_condition" {
            return builtin(name, &arguments, span);
        }
        let (name, super_call) = name
            .strip_prefix("@super:")
            .map_or((name, false), |name| (name, true));
        let resolved = if super_call {
            name.to_string()
        } else {
            self.dispatch_name(name, &arguments)
        };
        let index = self
            .module
            .functions
            .iter()
            .position(|function| function.name == resolved)
            .ok_or_else(|| {
                super::super::name_not_found(&resolved, "function dispatch", span)
            })?;
        let callee = &self.module.functions[index];
        let constructed = matches!(
            callee.kind,
            crate::ir::FunctionKind::Constructor | crate::ir::FunctionKind::FieldInit
        )
        .then(|| match arguments.first() {
            Some(Value::Object { handle, .. }) => Some(*handle),
            _ => None,
        })
        .flatten();
        let pinned = lifecycle_dispatch(callee, &arguments);
        if let Some(pinned) = pinned.clone() {
            self.pinned_dispatch.push(pinned);
        }
        self.call_depth += 1;
        let result = self.function(&self.module.functions[index], arguments);
        self.call_depth = self.call_depth.saturating_sub(1);
        if pinned.is_some() {
            let _ = self.pinned_dispatch.pop();
        }
        match result {
            Ok(Flow::Return(value)) => Ok(value.unwrap_or(Value::Null)),
            Ok(Flow::Stop(code)) => {
                self.stop_code = Some(code);
                Ok(Value::Null)
            }
            Err(error) => {
                if let Some(handle) = constructed {
                    let _ = self.objects.delete(handle, span);
                }
                Err(error)
            }
        }
    }

    /// Splits a library callee into (library name, member): `#3.MEAN` →
    /// (`"BNMath"`, `"MEAN"`) when the IR says module 3 provides `BNMath`.
    pub(crate) fn library_callee<'n>(&self, name: &'n str) -> Option<(&'static str, &'n str)> {
        let rest = name.strip_prefix('#')?;
        let (module, member) = rest.split_once('.')?;
        let library = self.module.standard_library_of(ModuleId(module.parse().ok()?))?;
        Some((library, member))
    }

    pub(crate) fn library_call(
        &mut self,
        library: &'static str,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        // Take the provider out so it can borrow the core mutably during the call.
        let Some(mut provider) = self.libraries.remove(library) else {
            return Err(runtime_error(
                crate::diagnostic::DiagId::LIBRARY_PROVIDER_UNAVAILABLE,
                format!("{library} provider unavailable"),
                span,
            ));
        };
        let result = provider.call(self, member, arguments, span);
        self.libraries.insert(library, provider);
        result
    }

    /// `HOST.<capability>.<member>` served by a registered HOST provider.
    fn host_provider_call(
        &mut self,
        capability: &str,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Some((key, mut provider)) = self.hosts.remove_entry(capability) else {
            return Err(host_unavailable(capability, span));
        };
        let result = provider.call(self, member, arguments, span);
        self.hosts.insert(key, provider);
        result
    }

    /// `NEW <class>()` owned by a HOST capability (`FS.File`).
    pub(crate) fn host_allocate(
        &mut self,
        capability: &'static str,
        class: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Some(mut provider) = self.hosts.remove(capability) else {
            return Err(host_unavailable(capability, span));
        };
        let result = provider.allocate(class, span);
        self.hosts.insert(capability, provider);
        result.unwrap_or_else(|| Err(host_unavailable(capability, span)))
    }

    /// `RELEASE` of a handle owned by a HOST capability.
    pub(crate) fn host_release(&mut self, value: &Value, span: Span) -> Result<(), Diagnostic> {
        let capabilities: Vec<&'static str> = self.hosts.keys().copied().collect();
        for capability in capabilities {
            let mut provider = self.hosts.remove(capability).expect("key just listed");
            let result = provider.release(value, span);
            self.hosts.insert(capability, provider);
            if let Some(result) = result {
                return result;
            }
        }
        Err(runtime_error(
            crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
            "no HOST capability owns this handle",
            span,
        ))
    }

    /// `NEW #N.Class()` served by a seam library, if `type_name` names one.
    pub(crate) fn library_allocate(
        &mut self,
        type_name: &str,
        span: Span,
    ) -> Option<Result<Value, Diagnostic>> {
        let (library, class) = self.library_callee(type_name)?;
        self.library_allocate_in(library, class, span)
    }

    /// `NEW <class>()` on a named seam library (cross-library use, e.g. `BNWeb`
    /// creating `BNLog` `Fields` for its access log).
    pub(crate) fn library_allocate_in(
        &mut self,
        library: &'static str,
        class: &str,
        span: Span,
    ) -> Option<Result<Value, Diagnostic>> {
        let mut provider = self.libraries.remove(library)?;
        let result = provider.allocate(class, span);
        self.libraries.insert(library, provider);
        result
    }

    /// `RELEASE` of a handle owned by a seam library, if any claims it.
    pub(crate) fn library_release(
        &mut self,
        value: &Value,
        span: Span,
    ) -> Option<Result<(), Diagnostic>> {
        let libraries: Vec<&'static str> = self.libraries.keys().copied().collect();
        for library in libraries {
            let mut provider = self.libraries.remove(library).expect("key just listed");
            let result = provider.release(value, span);
            self.libraries.insert(library, provider);
            if result.is_some() {
                return result;
            }
        }
        None
    }

    /// Tells every library the core destroyed the object at `handle`.
    pub(crate) fn notify_object_destroyed(&mut self, handle: Handle) {
        for provider in self.libraries.values_mut() {
            provider.object_destroyed(handle);
        }
    }


}

fn host_unavailable(capability: &str, span: Span) -> Diagnostic {
    runtime_error(
        crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
        format!("HOST.{capability} is not provided by this host"),
        span,
    )
}

impl crate::runtime::provider::CoreContext for Executor<'_, '_> {
    fn memory(&self) -> &Heap<Value> {
        &self.memory
    }

    fn memory_mut(&mut self) -> &mut Heap<Value> {
        &mut self.memory
    }

    fn call_function(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.call_named(name, arguments, span)
    }

    fn library_call(
        &mut self,
        library: &'static str,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        Executor::library_call(self, library, member, arguments, span)
    }

    fn output(&mut self) -> &mut dyn std::io::Write {
        self.output
    }

    fn module(&self) -> &Module {
        self.module
    }

    fn host(&self) -> &HostEnv {
        self.host
    }

    fn allocate_object(&mut self, class: &str, span: Span) -> Result<Value, Diagnostic> {
        Executor::allocate_object(self, class, span)
    }

    fn library_allocate(
        &mut self,
        library: &'static str,
        class: &str,
        span: Span,
    ) -> Option<Result<Value, Diagnostic>> {
        self.library_allocate_in(library, class, span)
    }

    fn library_release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        Executor::library_release(self, value, span)
    }

    fn library_take(&mut self, library: &'static str) -> Option<Box<dyn provider::Provider>> {
        self.libraries.remove(library)
    }

    fn library_insert(&mut self, library: &'static str, provider: Box<dyn provider::Provider>) {
        self.libraries.insert(library, provider);
    }
}
