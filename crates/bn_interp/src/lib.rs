// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[allow(unused_imports)]
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicU64},
    time::{SystemTime, UNIX_EPOCH},
};

use bn_value::Value;

pub mod temporal;

mod allocation;
mod collections;
mod compare;
mod helpers;
mod numeric;
pub mod provider;
mod render;
mod temporal_ops;

// Helpers a provider (the `bn` crate's `libraries`/`hosts`) may use. They are the
// core's value/diagnostic vocabulary, not language semantics.
pub use collections::{
    collect_indices as collect_indices_pub, dataframe_index_error as dataframe_index_error_pub,
    unsigned_indices as unsigned_indices_pub,
};
pub use compare::equals as equals_pub;
pub use executor::integer_from_count_pub;
pub use executor::numeric_overflow;
pub use helpers::require_arity as require_arity_pub;
pub use integer_from_i128_count as integer_from_i128_count_pub;
pub use is_not_available as is_not_available_pub;
pub use numeric::{
    integer as integer_pub, number_as_float as number_as_float_pub, parse_val as parse_val_pub,
};
pub use render::render as render_pub;
pub use {index_out_of_bounds as index_out_of_bounds_pub, runtime_error as runtime_error_pub};

use allocation::{add_sizes, display_element, pointer_element_default, pointer_element_size};
use compare::{equals, is_host_file_method, is_host_file_type, is_value, value_matches_type};
use helpers::{
    constant_value, default_function_owner, empty_named, find_block, require_arity, set, value,
};
use numeric::{
    boolean, exit_code, float_kind, float_value, integer, integer_kind, integer_range,
    integer_width, is_float_value, number_as_float, ordered, parse_float, parse_integer,
};
use render::render;
use temporal_ops::{is_temporal_builtin, temporal_call};

pub use bn_rt::{DataProvider, StandardDataProvider};

use bn_diag::Diagnostic;
use bn_ir::{
    Function, Instruction, Module, ModuleId, SymbolId, Terminator, ValidatedModule, ValueId,
    validate_module,
};
use bn_runtime::{Handle, Heap};
use bn_source::Span;
use bn_types::{FloatType, IntegerType, PointerLength, Type, integer_byte_size, static_size_of};

#[must_use]
pub fn is_not_available(value: &Value) -> bool {
    matches!(value, Value::NotAvailable)
}

/// Host-supplied arguments and clocks for one `bn run` execution.
pub struct HostEnv {
    arguments: Vec<String>,
    clock: ClockKind,
    random_state: AtomicU64,
    filesystem: FilesystemPolicy,
    exec_allowed: bool,
    /// Wall-clock ceiling for HOST.Exec.Run (D-H1-02). Policy may reduce; never exceeds 60s.
    exec_timeout: std::time::Duration,
    /// Per-stream capture ceiling in bytes (D-H1-02). Policy may reduce; never exceeds 16 MiB.
    exec_capture_limit: usize,
    data_provider: Arc<dyn DataProvider>,
    libraries: provider::Providers,
    hosts: provider::Providers,
}

#[derive(Clone, Debug)]
pub struct FilesystemPolicy {
    read_roots: Option<Vec<bn_rt::secure_fs::RootedDir>>,
    write_roots: Option<Vec<bn_rt::secure_fs::RootedDir>>,
}

impl FilesystemPolicy {
    fn unrestricted() -> Self {
        Self {
            read_roots: None,
            write_roots: None,
        }
    }

    fn denied() -> Self {
        Self {
            read_roots: Some(Vec::new()),
            write_roots: Some(Vec::new()),
        }
    }

    #[must_use]
    pub fn allows_capability(&self) -> bool {
        self.read_roots
            .as_ref()
            .is_none_or(|roots| !roots.is_empty())
            || self
                .write_roots
                .as_ref()
                .is_none_or(|roots| !roots.is_empty())
    }

    #[must_use]
    pub fn allows_path(&self, path: &Path, write: bool) -> bool {
        let roots = if write {
            &self.write_roots
        } else {
            &self.read_roots
        };
        let Some(roots) = roots else {
            return true;
        };
        roots.iter().any(|root| root.contains_resolved(path))
    }

    /// # Errors
    ///
    /// Propagates the I/O error; `PermissionDenied` when the path is outside the policy roots.
    pub fn open(
        &self,
        path: &Path,
        mode: bn_rt::secure_fs::OpenMode,
    ) -> std::io::Result<std::fs::File> {
        let roots = if mode == bn_rt::secure_fs::OpenMode::Read {
            &self.read_roots
        } else {
            &self.write_roots
        };
        let Some(roots) = roots else {
            let mut options = std::fs::OpenOptions::new();
            match mode {
                bn_rt::secure_fs::OpenMode::Read => {
                    options.read(true);
                }
                bn_rt::secure_fs::OpenMode::Write => {
                    options.write(true).create(true).truncate(true);
                }
                bn_rt::secure_fs::OpenMode::Append => {
                    options.append(true).create(true);
                }
            }
            return options.open(path);
        };
        roots
            .iter()
            .filter(|root| root.contains(path))
            .max_by_key(|root| root.path().components().count())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "filesystem path is outside the execution policy",
                )
            })?
            .open(path, mode)
    }

    /// # Errors
    ///
    /// Propagates the I/O error; `PermissionDenied` when the path is outside the policy roots.
    pub fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        let Some(roots) = &self.write_roots else {
            return std::fs::remove_file(path);
        };
        roots
            .iter()
            .filter(|root| root.contains(path))
            .max_by_key(|root| root.path().components().count())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "filesystem path is outside the execution policy",
                )
            })?
            .remove_file(path)
    }
}

impl Clone for HostEnv {
    fn clone(&self) -> Self {
        Self {
            arguments: self.arguments.clone(),
            clock: self.clock.clone(),
            random_state: AtomicU64::new(
                self.random_state.load(std::sync::atomic::Ordering::Relaxed),
            ),
            filesystem: self.filesystem.clone(),
            exec_allowed: self.exec_allowed,
            exec_timeout: self.exec_timeout,
            exec_capture_limit: self.exec_capture_limit,
            data_provider: Arc::clone(&self.data_provider),
            libraries: self.libraries.clone(),
            hosts: self.hosts.clone(),
        }
    }
}

#[derive(Clone)]
enum ClockKind {
    System,
    Fixed {
        timestamp_ms: i64,
        monotonic_ns: i64,
    },
}

impl HostEnv {
    #[must_use]
    pub fn system(arguments: Vec<String>) -> Self {
        Self {
            arguments,
            clock: ClockKind::System,
            random_state: AtomicU64::new(host_random_seed()),
            filesystem: FilesystemPolicy::unrestricted(),
            exec_allowed: true,
            exec_timeout: std::time::Duration::from_secs(60),
            exec_capture_limit: 16 * 1024 * 1024,
            data_provider: Arc::new(StandardDataProvider),
            libraries: provider::Providers::default(),
            hosts: provider::Providers::default(),
        }
    }

    #[must_use]
    pub fn fixed(arguments: Vec<String>, timestamp_ms: i64, monotonic_ns: i64) -> Self {
        Self {
            arguments,
            clock: ClockKind::Fixed {
                timestamp_ms,
                monotonic_ns,
            },
            random_state: AtomicU64::new(1),
            filesystem: FilesystemPolicy::unrestricted(),
            exec_allowed: true,
            exec_timeout: std::time::Duration::from_secs(60),
            exec_capture_limit: 16 * 1024 * 1024,
            data_provider: Arc::new(StandardDataProvider),
            libraries: provider::Providers::default(),
            hosts: provider::Providers::default(),
        }
    }

    /// Creates an untrusted sandbox environment that is fail-closed by default:
    /// filesystem access is completely denied unless explicitly configured with roots.
    #[must_use]
    pub fn sandbox(arguments: Vec<String>) -> Self {
        Self {
            arguments,
            clock: ClockKind::System,
            random_state: AtomicU64::new(host_random_seed()),
            filesystem: FilesystemPolicy::denied(),
            exec_allowed: false,
            exec_timeout: std::time::Duration::from_secs(60),
            exec_capture_limit: 16 * 1024 * 1024,
            data_provider: Arc::new(StandardDataProvider),
            libraries: provider::Providers::default(),
            hosts: provider::Providers::default(),
        }
    }

    /// Creates an environment that denies filesystem capability imports.
    #[must_use]
    pub fn without_filesystem(mut self) -> Self {
        self.filesystem = FilesystemPolicy::denied();
        self
    }

    /// Denies HOST.Exec capability for this execution.
    #[must_use]
    pub fn without_exec(mut self) -> Self {
        self.exec_allowed = false;
        self
    }

    /// Reduces the HOST.Exec wall-clock ceiling. Values above 60 seconds are clamped
    /// to the compiled default (D-H1-02: policy may reduce, never exceed).
    #[must_use]
    pub fn with_exec_timeout_secs(mut self, seconds: u64) -> Self {
        self.exec_timeout = std::time::Duration::from_secs(seconds.min(60));
        self
    }

    /// Reduces the per-stream HOST.Exec capture ceiling. Values above 16 MiB are
    /// clamped to the compiled default (D-H1-02).
    #[must_use]
    pub fn with_exec_capture_limit(mut self, bytes: usize) -> Self {
        self.exec_capture_limit = bytes.min(16 * 1024 * 1024);
        self
    }

    /// Restricts writes while preserving the default read capability.
    #[must_use]
    pub fn without_filesystem_writes(mut self) -> Self {
        self.filesystem.write_roots = Some(Vec::new());
        self
    }

    /// Replaces the standard-library data provider for this execution.
    #[must_use]
    pub fn with_data_provider(mut self, provider: Arc<dyn DataProvider>) -> Self {
        self.data_provider = provider;
        self
    }

    /// Restricts filesystem reads and writes to canonicalized directory roots.
    ///
    /// Existing roots must be directories. An empty root list denies that
    /// operation; passing both lists empty denies all filesystem access.
    ///
    /// # Errors
    ///
    /// Returns an error when a configured root does not exist or is not a
    /// directory.
    pub fn with_filesystem_roots(
        mut self,
        read_roots: Vec<PathBuf>,
        write_roots: Vec<PathBuf>,
    ) -> Result<Self, &'static str> {
        let canonicalize_roots = |roots: Vec<PathBuf>| {
            roots
                .into_iter()
                .map(|root| {
                    bn_rt::secure_fs::RootedDir::new(&root)
                        .map_err(|_| "filesystem policy root cannot be opened")
                })
                .collect::<Result<Vec<_>, _>>()
        };
        self.filesystem = FilesystemPolicy {
            read_roots: Some(canonicalize_roots(read_roots)?),
            write_roots: Some(canonicalize_roots(write_roots)?),
        };
        Ok(self)
    }

    pub fn timestamp_ms(&self) -> i64 {
        match self.clock {
            ClockKind::Fixed { timestamp_ms, .. } => timestamp_ms,
            ClockKind::System => bn_rt::timestamp_ms(),
        }
    }

    pub fn monotonic_ns(&self) -> i64 {
        match self.clock {
            ClockKind::Fixed { monotonic_ns, .. } => monotonic_ns,
            ClockKind::System => bn_rt::monotonic_ns(),
        }
    }

    /// CSV/data provider bound to this host.
    #[must_use]
    pub fn data_provider(&self) -> &Arc<dyn DataProvider> {
        &self.data_provider
    }

    /// Filesystem policy in force for this host.
    #[must_use]
    pub fn filesystem(&self) -> &FilesystemPolicy {
        &self.filesystem
    }

    /// Replaces the library providers this host offers (CLI features, tests).
    #[must_use]
    pub fn with_libraries(mut self, libraries: provider::Providers) -> Self {
        self.libraries = libraries;
        self
    }

    pub fn exec_allowed(&self) -> bool {
        self.exec_allowed
    }

    pub fn exec_timeout(&self) -> std::time::Duration {
        self.exec_timeout
    }

    pub fn exec_capture_limit(&self) -> usize {
        self.exec_capture_limit
    }

    pub fn random_state(&self) -> &AtomicU64 {
        &self.random_state
    }

    /// Replaces the HOST capability providers this host offers.
    #[must_use]
    pub fn with_hosts(mut self, hosts: provider::Providers) -> Self {
        self.hosts = hosts;
        self
    }

    #[must_use]
    pub fn fork_for_task(&self) -> Self {
        let seed = self
            .random_state
            .fetch_add(0x9E37_79B9_7F4A_7C15, std::sync::atomic::Ordering::Relaxed)
            .max(1);
        Self {
            arguments: self.arguments.clone(),
            clock: self.clock.clone(),
            random_state: AtomicU64::new(seed),
            filesystem: self.filesystem.clone(),
            exec_allowed: self.exec_allowed,
            exec_timeout: self.exec_timeout,
            exec_capture_limit: self.exec_capture_limit,
            data_provider: Arc::clone(&self.data_provider),
            libraries: self.libraries.clone(),
            hosts: self.hosts.clone(),
        }
    }
}

mod support;
use support::{debug_variables, host_random_seed};

#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Instance {
    class: String,
    fields: HashMap<String, Value>,
}

#[derive(Default)]
struct OwnershipFrame {
    owned_values: std::collections::HashSet<ValueId>,
    loaded_values: HashMap<ValueId, SymbolId>,
    released_symbols: std::collections::HashSet<SymbolId>,
    release_values: std::collections::HashSet<ValueId>,
    local_symbols: std::collections::HashSet<SymbolId>,
    weak_symbols: std::collections::HashSet<SymbolId>,
}

struct Executor<'a, 'debug> {
    module: &'a Module,
    input: &'a mut dyn BufRead,
    output: &'a mut dyn Write,
    host: &'a HostEnv,
    stop_code: Option<i128>,
    statics: HashMap<(String, String), Value>,
    class_init: HashMap<String, ClassInit>,
    objects: Heap<Instance>,
    memory: Heap<Value>,
    pinned_dispatch: Vec<(Handle, String)>,
    /// Library providers for this execution, keyed by standard-module name.
    libraries: HashMap<&'static str, Box<dyn provider::Provider>>,
    hosts: HashMap<&'static str, Box<dyn provider::Provider>>,
    debug_hook: Option<DebugHook<'debug>>,
    debug_control: Option<DebugControl<'debug>>,
    call_depth: usize,
    ownership_frames: Vec<OwnershipFrame>,
}

impl<'a, 'debug> Executor<'a, 'debug> {
    fn new(
        module: &'a Module,
        input: &'a mut dyn BufRead,
        output: &'a mut dyn Write,
        host: &'a HostEnv,
        debug_hook: Option<DebugHook<'debug>>,
        debug_control: Option<DebugControl<'debug>>,
    ) -> Self {
        Self {
            module,
            input,
            output,
            host,
            stop_code: None,
            statics: HashMap::new(),
            class_init: HashMap::new(),
            objects: Heap::default(),
            memory: Heap::default(),
            pinned_dispatch: Vec::new(),
            libraries: host.libraries.instantiate(),
            hosts: host.hosts.instantiate(),
            debug_hook,
            debug_control,
            call_depth: 0,
            ownership_frames: Vec::new(),
        }
    }
}

/// Read-only interpreter event emitted at an executable instruction boundary.
pub type DebugHook<'a> = &'a mut dyn FnMut(&str, bn_source::Span);

/// Decision returned by an interactive debugger at an instruction boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebugDecision {
    Continue,
    Terminate,
}

/// Read-only value visible to an interactive debugger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugVariable {
    pub name: String,
    pub value: String,
}

/// Interactive debugger callback. It is invoked before each executable
/// instruction and may block while the client is paused.
pub type DebugControl<'a> =
    &'a mut dyn FnMut(&str, usize, bn_source::Span, &[DebugVariable]) -> DebugDecision;

#[derive(Clone, Copy)]
enum ClassInit {
    Running,
    Ready,
}

/// Executes the `Start` function of a validated BN IR module.
///
/// # Errors
///
/// Returns a source-spanned runtime diagnostic for invalid operations, missing
/// entry points, overflow, invalid indices, or I/O failures.
pub fn execute(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> Result<u8, Diagnostic> {
    let host = HostEnv::system(vec!["bn".into()]);
    execute_with_host(module, input, output, &host)
}

/// Executes `Start` with injected command-line arguments and clocks.
///
/// # Errors
///
/// Returns a source-spanned runtime diagnostic for invalid operations, missing
/// entry points, overflow, invalid indices, or I/O failures.
pub fn execute_with_host(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
) -> Result<u8, Diagnostic> {
    let validated = validate_module(module.clone())?;
    execute_validated_with_host(&validated, input, output, host)
}

/// Executes a named function for an isolated dispatch worker and preserves its
/// returned BN value for the ticket.
///
/// # Errors
///
/// Returns the runtime diagnostic the function raises.
pub fn execute_named_with_host(
    module: &Module,
    name: &str,
    arguments: Vec<Value>,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
) -> Result<Value, Diagnostic> {
    let validated = validate_module(module.clone())?;
    let function = validated
        .as_module()
        .functions
        .iter()
        .find(|function| function.name == name)
        .ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::FUNCTION_NOT_FOUND,
                format!("function '{name}' was not found"),
                default_span(),
            )
        })?;
    let mut executor = Executor::new(validated.as_module(), input, output, host, None, None);
    match executor.function(function, arguments)? {
        Flow::Return(Some(value)) => Ok(value),
        Flow::Return(None) => Ok(Value::Null),
        Flow::Stop(code) => Ok(Value::Integer(code, IntegerType::Int32)),
    }
}

/// Executes a module after the language validator has produced its proof
/// object.
///
/// # Errors
///
/// Returns a runtime diagnostic for invalid operations, missing entry points,
/// capability failures, or I/O errors.
pub fn execute_validated_with_host(
    validated: &ValidatedModule,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
) -> Result<u8, Diagnostic> {
    execute_with_host_inner(validated.as_module(), input, output, host, None, None)
}

/// Executes `Start` while reporting each interpreter instruction to a caller-owned
/// debug hook. The hook observes source spans only and cannot evaluate BN code.
///
/// # Errors
///
/// Returns the same source-spanned runtime diagnostics as [`execute_with_host`].
pub fn execute_with_host_debug(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
    debug_hook: DebugHook<'_>,
) -> Result<u8, Diagnostic> {
    let validated = validate_module(module.clone())?;
    execute_validated_with_host_debug(&validated, input, output, host, debug_hook)
}

/// Executes validated IR while reporting instruction-boundary debug events.
///
/// # Errors
///
/// Returns the same runtime diagnostics as [`execute_validated_with_host`].
pub fn execute_validated_with_host_debug(
    validated: &ValidatedModule,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
    debug_hook: DebugHook<'_>,
) -> Result<u8, Diagnostic> {
    execute_with_host_inner(
        validated.as_module(),
        input,
        output,
        host,
        Some(debug_hook),
        None,
    )
}

/// Executes `Start` with an interactive debugger control callback.
///
/// The callback runs at instruction boundaries and may block to implement
/// pause/continue/step. Returning [`DebugDecision::Terminate`] stops execution
/// without evaluating further user code.
///
/// # Errors
///
/// Returns the same source-spanned runtime diagnostics as
/// [`execute_with_host`], including `DEBUG_TERMINATED` when the callback
/// requests termination.
pub fn execute_with_host_debug_control(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
    debug_control: DebugControl<'_>,
) -> Result<u8, Diagnostic> {
    let validated = validate_module(module.clone())?;
    execute_validated_with_host_debug_control(&validated, input, output, host, debug_control)
}

/// Executes validated IR with an interactive debugger control callback.
///
/// # Errors
///
/// Returns the same runtime diagnostics as [`execute_validated_with_host`],
/// including a termination diagnostic when the callback requests it.
pub fn execute_validated_with_host_debug_control(
    validated: &ValidatedModule,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
    debug_control: DebugControl<'_>,
) -> Result<u8, Diagnostic> {
    execute_with_host_inner(
        validated.as_module(),
        input,
        output,
        host,
        None,
        Some(debug_control),
    )
}

fn execute_with_host_inner<'debug>(
    module: &Module,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
    host: &HostEnv,
    debug_hook: Option<DebugHook<'debug>>,
    debug_control: Option<DebugControl<'debug>>,
) -> Result<u8, Diagnostic> {
    if !host.filesystem.allows_capability()
        && let Some(span) = module.filesystem_import
    {
        return Err(runtime_error(
            bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
            "HOST.FileSystem is not provided by this host",
            span,
        ));
    }
    let start = module.entry().ok_or_else(|| {
        runtime_error(
            bn_diag::DiagId::START_NOT_FOUND,
            "executable module requires FUNCTION Start",
            default_span(),
        )
    })?;
    if !start.parameters.is_empty() {
        return Err(runtime_error(
            bn_diag::DiagId::INVALID_START,
            "FUNCTION Start cannot declare parameters",
            start.span,
        ));
    }
    let mut executor = Executor::new(module, input, output, host, debug_hook, debug_control);
    match executor.function(start, Vec::new())? {
        Flow::Return(None | Some(Value::Null)) => Ok(0),
        Flow::Return(Some(Value::Integer(code, _))) | Flow::Stop(code) => {
            exit_code(code, start.span)
        }
        Flow::Return(Some(Value::Error { code, message })) => Err(runtime_error(
            bn_diag::DiagId::DISPATCH,
            format!("{code}: {message}"),
            start.span,
        )),
        Flow::Return(Some(_)) => Err(runtime_error(
            bn_diag::DiagId::INVALID_START,
            "FUNCTION Start must return VOID or INTEGER",
            start.span,
        )),
    }
}

/// Runs `f` against a fresh executor for `module` and `host`, with no input
/// and discarded output. Libraries use it to run BN code off the main
/// executor (`BNWeb` request callbacks).
pub fn run_isolated<R>(
    module: &Module,
    host: &HostEnv,
    f: impl FnOnce(&mut dyn provider::CoreContext) -> R,
) -> R {
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::<u8>::new();
    let mut executor = Executor::new(module, &mut input, &mut output, host, None, None);
    f(&mut executor)
}

enum Flow {
    Return(Option<Value>),
    Stop(i128),
}
mod executor;

#[allow(dead_code)]
fn coerce(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    crate::executor::coerce(value, ty, span)
}

/// # Errors
///
/// Returns `NUMERIC_OVERFLOW` when the count does not fit the integer type.
#[allow(dead_code)]
pub fn integer_from_i128_count(count: i128, span: Span) -> Result<Value, Diagnostic> {
    if !(0..=i128::from(i32::MAX)).contains(&count) {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(count, IntegerType::Int32))
}

/// # Panics
///
/// Only if the diagnostic registry schema for this identity is inconsistent (a build error, never a runtime state).
pub fn runtime_error(id: bn_diag::DiagId, message: impl Into<String>, span: Span) -> Diagnostic {
    let message = message.into();
    let arguments = match id {
        bn_diag::DiagId::NAME_NOT_FOUND => vec![
            ("name".into(), message.clone().into()),
            ("context".into(), "runtime lookup".into()),
        ],
        bn_diag::DiagId::INDEX_OUT_OF_BOUNDS => vec![
            ("index".into(), "unknown".into()),
            ("bound".into(), "unknown".into()),
            ("context".into(), message.into()),
        ],
        bn_diag::DiagId::TYPE_MISMATCH => vec![
            ("expected".into(), "a value matching the operation".into()),
            ("actual".into(), "an incompatible value".into()),
            ("context".into(), message.into()),
        ],
        // Single-argument schemas (`message`, `detail`, `operation`, …) take
        // the whole text under the registry's argument name.
        _ => match id.argument_schema() {
            [only] => vec![(only.name.into(), message.into())],
            schema => unreachable!(
                "{} needs an explicit argument mapping ({} arguments)",
                id.code(),
                schema.len()
            ),
        },
    };
    Diagnostic::structured(
        id,
        arguments,
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("runtime compatibility diagnostic schema")
}

/// # Panics
///
/// Only if the diagnostic registry schema for this identity is inconsistent (a build error, never a runtime state).
pub fn name_not_found(
    name: impl Into<String>,
    context: impl Into<String>,
    span: Span,
) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::NAME_NOT_FOUND,
        vec![
            ("name".into(), name.into().into()),
            ("context".into(), context.into().into()),
        ],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("name-not-found diagnostic schema")
}

/// # Panics
///
/// Only if the diagnostic registry schema for this identity is inconsistent (a build error, never a runtime state).
pub fn index_out_of_bounds(
    index: impl std::fmt::Display,
    bound: impl std::fmt::Display,
    context: impl std::fmt::Display,
    span: Span,
) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
        vec![
            ("index".into(), index.to_string().into()),
            ("bound".into(), bound.to_string().into()),
            ("context".into(), context.to_string().into()),
        ],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("index-out-of-bounds diagnostic schema")
}

/// # Panics
///
/// Only if the diagnostic registry schema for this identity is inconsistent (a build error, never a runtime state).
pub fn type_mismatch(
    expected: impl std::fmt::Display,
    actual: impl std::fmt::Display,
    context: impl std::fmt::Display,
    span: Span,
) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::TYPE_MISMATCH,
        vec![
            ("expected".into(), expected.to_string().into()),
            ("actual".into(), actual.to_string().into()),
            ("context".into(), context.to_string().into()),
        ],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("type-mismatch diagnostic schema")
}

fn integer_overflow(span: Span) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::NUMERIC_OVERFLOW,
        vec![("operation".into(), "converting a value to INTEGER".into())],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("numeric-overflow diagnostic schema")
}
#[must_use]
pub fn default_span() -> Span {
    Span {
        start: bn_source::Position {
            source_id: bn_source::Position::UNKNOWN_SOURCE,
            revision: bn_source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
        end: bn_source::Position {
            source_id: bn_source::Position::UNKNOWN_SOURCE,
            revision: bn_source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
