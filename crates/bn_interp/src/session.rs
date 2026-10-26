// Basic Next interactive session execution runtime (Cluster B)
// Preserves interpreter memory, ARC heap, and providers state between turns
// with transactional rollback on runtime errors (W1–W5).

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{BufRead, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bn_diag::Diagnostic;
use bn_ir::{SymbolId, ValidatedModule};
use bn_runtime::Heap;
use bn_value::{Handle, Value};

use crate::{
    ClassInit, Executor, Flow, HostEnv, Instance, default_span, function_index, provider::Provider,
    runtime_error, stack_address, stack_budget,
};

/// Result of an executed interactive submission.
#[derive(Debug)]
pub struct ExecutionResult {
    pub exit_code: u8,
    pub returned_value: Option<Value>,
    pub updated_variables: BTreeMap<String, Value>,
}

/// Snapshot of live ARC references and allocations for the `:arc` command.
#[derive(Clone, Debug)]
pub struct ArcInspection {
    pub live_objects: Vec<bn_rt::arc::ObjectInfo>,
    pub total_live_objects: usize,
    pub live_memory_allocations: usize,
    pub live_instances: usize,
}

/// Transactional persistent state of an interactive session.
#[derive(Clone, Debug, Default)]
pub(crate) struct SessionState {
    pub(crate) statics: HashMap<(String, String), Value>,
    pub(crate) class_init: HashMap<String, ClassInit>,
    pub(crate) arc: bn_rt::arc::ArcCore,
    pub(crate) objects: Heap<Instance>,
    pub(crate) memory: Heap<Value>,
    pub(crate) pinned_dispatch: Vec<(Handle, String)>,
    pub(crate) variable_values: BTreeMap<String, Value>,
}

/// Persistent interpreter execution session holding memory and providers across turns.
pub struct InteractiveSession {
    host: HostEnv,
    state: SessionState,
    libraries: HashMap<&'static str, Box<dyn Provider>>,
    hosts: HashMap<&'static str, Box<dyn Provider>>,
    cancel_flag: Arc<AtomicBool>,
}

impl std::fmt::Debug for InteractiveSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteractiveSession")
            .field("host", &self.host)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl InteractiveSession {
    /// Creates a fresh interactive session with initialized host capabilities.
    #[must_use]
    pub fn new(host: HostEnv) -> Self {
        let libraries = host.libraries.instantiate();
        let hosts = host.hosts.instantiate();
        Self {
            host,
            state: SessionState::default(),
            libraries,
            hosts,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Read-only reference to the session's active host environment.
    #[must_use]
    pub const fn host(&self) -> &HostEnv {
        &self.host
    }

    /// Handle to the atomic cancellation flag (set by signal handlers on Ctrl-C).
    #[must_use]
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }

    /// Requests immediate cancellation of the currently executing turn.
    pub fn cancel(&self) {
        self.cancel_flag.store(true, Ordering::Relaxed);
    }

    /// Resets all accumulated variables, heap allocations, and ARC objects.
    pub fn reset(&mut self) {
        self.state = SessionState::default();
        self.libraries = self.host.libraries.instantiate();
        self.hosts = self.host.hosts.instantiate();
        self.cancel_flag.store(false, Ordering::Relaxed);
    }

    /// Current live variable bindings in the session (name -> current Value).
    #[must_use]
    pub const fn variables(&self) -> &BTreeMap<String, Value> {
        &self.state.variable_values
    }

    /// Removes a variable from the active session state.
    pub fn remove_variable(&mut self, name: &str) {
        self.state.variable_values.remove(name);
    }

    /// Inspects live ARC references and allocations in the current session.
    #[must_use]
    pub fn arc_inspection(&self) -> ArcInspection {
        let live_objects = self.state.arc.snapshot();
        let total_live_objects = live_objects.len();
        ArcInspection {
            live_objects,
            total_live_objects,
            live_memory_allocations: self.state.memory.count(),
            live_instances: self.state.objects.count(),
        }
    }

    /// Executes a single validated submission turn with transactional rollback on failure.
    ///
    /// # Errors
    ///
    /// Returns runtime diagnostics on evaluation error or user cancellation.
    #[allow(clippy::too_many_lines)]
    pub fn execute_submission(
        &mut self,
        validated: &ValidatedModule,
        variable_symbol_map: &HashMap<String, SymbolId>,
        input: &mut dyn BufRead,
        output: &mut dyn Write,
    ) -> Result<ExecutionResult, Diagnostic> {
        let module = validated.as_module();
        if !self.host.provides_filesystem()
            && let Some(span) = module.filesystem_import
        {
            return Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                bn_catalog::text("frontend", "msg.host_filesystem_not_provided"),
                span,
            ));
        }

        let start = module.entry().ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::START_NOT_FOUND,
                bn_catalog::text("frontend", "msg.session_start_not_found"),
                default_span(),
            )
        })?;

        // 1. Transactional state snapshot
        let backup_state = self.state.clone();

        // 2. Prepare restored symbols
        let mut session_bindings = HashMap::new();
        let mut session_restored_symbols = HashSet::new();
        for (name, symbol_id) in variable_symbol_map {
            if let Some(val) = self.state.variable_values.get(name) {
                session_bindings.insert(*symbol_id, val.clone());
                session_restored_symbols.insert(*symbol_id);
            }
        }

        // 3. Move session state into Executor
        let statics = std::mem::take(&mut self.state.statics);
        let class_init = std::mem::take(&mut self.state.class_init);
        let arc = std::mem::take(&mut self.state.arc);
        let objects = std::mem::take(&mut self.state.objects);
        let memory = std::mem::take(&mut self.state.memory);
        let pinned_dispatch = std::mem::take(&mut self.state.pinned_dispatch);
        let libraries = std::mem::take(&mut self.libraries);
        let hosts = std::mem::take(&mut self.hosts);

        let mut executor = Executor {
            module,
            function_index: function_index(module),
            stack_base: stack_address(),
            stack_budget: stack_budget(),
            input,
            output,
            host: &self.host,
            stop_code: None,
            unwinding: None,
            statics,
            class_init,
            arc,
            objects,
            memory,
            pinned_dispatch,
            libraries,
            hosts,
            debug_hook: None,
            debug_control: None,
            emitter: None,
            call_depth: 0,
            ownership_frames: Vec::new(),
            session_bindings,
            session_restored_symbols,
            session_captured_symbols: Some(HashMap::new()),
            cancel_flag: Some(Arc::clone(&self.cancel_flag)),
        };

        // 4. Run the turn
        let flow_result = executor.function(start, Vec::new());

        // 5. Restore providers back to session
        self.libraries = executor.libraries;
        self.hosts = executor.hosts;

        // 6. Handle failure with transactional rollback
        let flow = match flow_result {
            Ok(flow) => flow,
            Err(diag) => {
                self.state = backup_state;
                self.cancel_flag.store(false, Ordering::Relaxed);
                return Err(diag);
            }
        };

        let (exit_code, returned_value) = match flow {
            Flow::Return(None | Some(Value::Null)) => (0, None),
            Flow::Return(Some(Value::Integer(code, kind))) => {
                let code_u8 = u8::try_from(code).unwrap_or(0);
                (code_u8, Some(Value::Integer(code, kind)))
            }
            Flow::Stop(code) => {
                let code_u8 = u8::try_from(code).unwrap_or(0);
                (
                    code_u8,
                    Some(Value::Integer(code, bn_types::IntegerType::Int32)),
                )
            }
            Flow::Return(Some(Value::Error { code, message, .. })) => {
                self.state = backup_state;
                self.cancel_flag.store(false, Ordering::Relaxed);
                return Err(runtime_error(
                    bn_diag::DiagId::DISPATCH,
                    format!("{code}: {message}"),
                    start.span,
                ));
            }
            Flow::Return(Some(other)) => (0, Some(other)),
        };

        // 7. Extract updated variable bindings
        let captured = executor.session_captured_symbols.take().unwrap_or_default();
        let mut updated_variables = BTreeMap::new();
        for (name, symbol_id) in variable_symbol_map {
            if let Some(val) = captured.get(symbol_id) {
                updated_variables.insert(name.clone(), val.clone());
                self.state.variable_values.insert(name.clone(), val.clone());
            } else {
                self.state.variable_values.remove(name);
            }
        }

        // 8. Commit updated executor state
        self.state.statics = executor.statics;
        self.state.class_init = executor.class_init;
        self.state.arc = executor.arc;
        self.state.objects = executor.objects;
        self.state.memory = executor.memory;
        self.state.pinned_dispatch = executor.pinned_dispatch;
        self.cancel_flag.store(false, Ordering::Relaxed);

        Ok(ExecutionResult {
            exit_code,
            returned_value,
            updated_variables,
        })
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod session_tests;
