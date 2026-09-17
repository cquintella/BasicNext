// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[allow(unused_imports)]
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Read, Write},
    sync::{Arc, atomic::AtomicU64},
    time::{SystemTime, UNIX_EPOCH},
    path::{Path, PathBuf},
};

use bn_value::Value;

#[path = "runtime/allocation.rs"]
mod allocation;
#[path = "runtime/collections.rs"]
mod collections;
#[path = "runtime/compare.rs"]
mod compare;
#[path = "runtime/helpers.rs"]
mod helpers;
#[path = "runtime/net_values.rs"]
mod net_values;
#[path = "runtime/numeric.rs"]
mod numeric;
#[path = "runtime/render.rs"]
mod render;
#[path = "runtime/temporal_ops.rs"]
mod temporal_ops;
#[path = "runtime/provider.rs"]
pub mod provider;

// Helpers a library provider (`crate::libraries::*`) may use. They are the
// core's value/diagnostic vocabulary, not language semantics.
pub(crate) use executor::numeric_overflow;
pub(crate) use helpers::require_arity as require_arity_pub;
pub(crate) use integer_from_i128_count as integer_from_i128_count_pub;
pub(crate) use collections::{
    collect_indices as collect_indices_pub, dataframe_index_error as dataframe_index_error_pub,
    unsigned_indices as unsigned_indices_pub,
};
pub(crate) use executor::integer_from_count_pub;
pub(crate) use render::render as render_pub;
pub(crate) use compare::equals as equals_pub;
pub(crate) use is_not_available as is_not_available_pub;
pub(crate) use numeric::{
    integer as integer_pub, number_as_float as number_as_float_pub, parse_val as parse_val_pub,
};
pub(crate) use {index_out_of_bounds as index_out_of_bounds_pub, runtime_error as runtime_error_pub};

use allocation::{add_sizes, display_element, pointer_element_default, pointer_element_size};
use compare::{equals, is_host_file_method, is_host_file_type, is_value, value_matches_type};
use helpers::{
    constant_value, default_function_owner, empty_named, find_block, require_arity, set, value,
};
use net_values::{
    address_value, endpoint_value, net_address, net_addresses, net_endpoint, ping_reply_value,
};
use numeric::{
    boolean, exit_code, float_kind, float_value, integer, integer_kind, integer_range,
    integer_width, is_float_value, number_as_float, ordered, parse_float, parse_integer,
};
use render::render;
use temporal_ops::{is_temporal_builtin, temporal_call};

pub use bn_rt::{DataProvider, StandardDataProvider};

#[allow(unused_imports)]
use crate::{
    dataframe::{
        DataFrameJoin, DataFrameJoinConfig,
        DataFrameResource as GenericDataFrameResource, add_dataframe_column,
        append_columns, append_rows, column_name, convert_dataframe_column,
        copy_dataframe_column, dataframe_reduce_column, duplicate_column_names,
        get_dataframe_cell, join_dataframes, select_dataframe, set_column_label,
        transpose_dataframe, zscore_column,
    },
    diagnostic::Diagnostic,
    heap::{Handle, Heap},
    ir::{Function, Instruction, Module, ModuleId, SymbolId, Terminator, ValidatedModule, ValueId, validate_module},
    source::Span,
    types::{
        FloatType, IntegerType, PointerLength, Type, integer_byte_size, static_size_of,
    },
};

pub(crate) type DataFrameResource = GenericDataFrameResource<Value>;

pub(crate) fn is_not_available(value: &Value) -> bool {
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
    libraries: provider::Libraries,
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

    pub(crate) fn allows_capability(&self) -> bool {
        self.read_roots.as_ref().is_none_or(|roots| !roots.is_empty())
            || self
                .write_roots
                .as_ref()
                .is_none_or(|roots| !roots.is_empty())
    }

    pub(crate) fn allows_path(&self, path: &Path, write: bool) -> bool {
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

    pub(crate) fn open(&self, path: &Path, mode: bn_rt::secure_fs::OpenMode) -> std::io::Result<std::fs::File> {
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

    fn remove_file(&self, path: &Path) -> std::io::Result<()> {
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
                self.random_state
                    .load(std::sync::atomic::Ordering::Relaxed),
            ),
            filesystem: self.filesystem.clone(),
            exec_allowed: self.exec_allowed,
            exec_timeout: self.exec_timeout,
            exec_capture_limit: self.exec_capture_limit,
            data_provider: Arc::clone(&self.data_provider),
            libraries: self.libraries.clone(),
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
            libraries: crate::libraries::default_libraries(),
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
            libraries: crate::libraries::default_libraries(),
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
            libraries: crate::libraries::default_libraries(),
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

    fn timestamp_ms(&self) -> i64 {
        match self.clock {
            ClockKind::Fixed { timestamp_ms, .. } => timestamp_ms,
            ClockKind::System => bn_rt::timestamp_ms(),
        }
    }

    fn monotonic_ns(&self) -> i64 {
        match self.clock {
            ClockKind::Fixed { monotonic_ns, .. } => monotonic_ns,
            ClockKind::System => bn_rt::monotonic_ns(),
        }
    }

    /// CSV/data provider bound to this host.
    #[must_use]
    pub(crate) fn data_provider(&self) -> &Arc<dyn DataProvider> {
        &self.data_provider
    }

    /// Filesystem policy in force for this host.
    #[must_use]
    pub(crate) fn filesystem(&self) -> &FilesystemPolicy {
        &self.filesystem
    }

    /// Replaces the library providers this host offers (CLI features, tests).
    #[must_use]
    pub fn with_libraries(mut self, libraries: provider::Libraries) -> Self {
        self.libraries = libraries;
        self
    }

    pub(crate) fn fork_for_task(&self) -> Self {
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
        }
    }
}

#[path = "runtime/support.rs"]
mod support;
use support::{debug_variables, host_random_seed};

#[cfg(test)]
#[path = "runtime/tests.rs"]
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
    files: HashMap<u64, FileResource>,
    next_file: u64,
    tcp_streams: HashMap<u64, crate::net::TcpStream>,
    next_tcp_stream: u64,
    tcp_listeners: HashMap<u64, Vec<crate::net::TcpListener>>,
    next_tcp_listener: u64,
    udp_sockets: HashMap<u64, crate::net::UdpSocket>,
    next_udp_socket: u64,
    dispatch_queues: HashMap<u64, crate::dispatch::Queue>,
    next_dispatch_queue: u64,
    dispatch_tickets: HashMap<u64, crate::dispatch::Ticket>,
    next_dispatch_ticket: u64,
    dispatch_groups: HashMap<u64, crate::dispatch::DispatchGroup>,
    dispatch_barriers: HashMap<u64, crate::dispatch::Barrier>,
    dispatch_semaphores: HashMap<u64, crate::dispatch::DispatchSemaphore>,
    dispatch_mutexes: HashMap<u64, crate::dispatch::DispatchMutex>,
    next_dispatch_sync: u64,
    web_servers: HashMap<Handle, std::sync::Arc<std::sync::Mutex<crate::web::ServerState>>>,
    web_loggers: HashMap<Handle, u64>,
    web_tls_configs: HashMap<Handle, std::sync::Arc<rustls::ServerConfig>>,
    web_server_options: HashMap<Handle, crate::web::ServerOptions>,
    web_egress_policies: HashMap<Handle, crate::web::EgressPolicy>,
    web_cookie_jars: HashMap<Handle, crate::web_state::CookieJar>,
    web_session_stores: HashMap<Handle, crate::web_state::SessionStore>,
    web_acls: HashMap<Handle, crate::web_state::Acl>,
    web_scrapers: HashMap<Handle, crate::web_state::Scraper>,
    web_handlers: HashMap<Handle, HashMap<String, String>>,
    web_filters: HashMap<Handle, Vec<String>>,
    web_responses: HashMap<Handle, crate::web::Response>,
    web_requests: HashMap<Handle, crate::web::Request>,
    web_values: HashMap<Handle, Vec<String>>,
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
            files: HashMap::new(),
            next_file: 1,
            tcp_streams: HashMap::new(),
            next_tcp_stream: 1,
            tcp_listeners: HashMap::new(),
            next_tcp_listener: 1,
            udp_sockets: HashMap::new(),
            next_udp_socket: 1,
            dispatch_queues: HashMap::new(),
            next_dispatch_queue: 1,
            dispatch_tickets: HashMap::new(),
            next_dispatch_ticket: 1,
            dispatch_groups: HashMap::new(),
            dispatch_barriers: HashMap::new(),
            dispatch_semaphores: HashMap::new(),
            dispatch_mutexes: HashMap::new(),
            next_dispatch_sync: 1,
            web_servers: HashMap::new(),
            web_loggers: HashMap::new(),
            web_tls_configs: HashMap::new(),
            web_server_options: HashMap::new(),
            web_egress_policies: HashMap::new(),
            web_cookie_jars: HashMap::new(),
            web_session_stores: HashMap::new(),
            web_acls: HashMap::new(),
            web_scrapers: HashMap::new(),
            web_handlers: HashMap::new(),
            web_filters: HashMap::new(),
            web_responses: HashMap::new(),
            web_requests: HashMap::new(),
            web_values: HashMap::new(),
            debug_hook,
            debug_control,
            call_depth: 0,
            ownership_frames: Vec::new(),
        }
    }
}

/// Read-only interpreter event emitted at an executable instruction boundary.
pub type DebugHook<'a> = &'a mut dyn FnMut(&str, crate::source::Span);

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
    &'a mut dyn FnMut(&str, usize, crate::source::Span, &[DebugVariable]) -> DebugDecision;

struct FileResource {
    file: Option<std::fs::File>,
    family: Option<bool>, // ponytail: one bit for text/binary; expand only if modes grow.
}

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
pub(crate) fn execute_named_with_host(
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
        .ok_or_else(|| runtime_error(crate::diagnostic::DiagId::FUNCTION_NOT_FOUND, format!("function '{name}' was not found"), default_span()))?;
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
    crate::tls::install_ring_provider()
        .map_err(|message| runtime_error(crate::diagnostic::DiagId::TLS_PROVIDER_UNAVAILABLE, message, default_span()))?;
    if !host.filesystem.allows_capability()
        && let Some(span) = module.filesystem_import
    {
        return Err(runtime_error(crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
            "HOST.FileSystem is not provided by this host",
            span,
        ));
    }
    let start = module
        .entry()
        .ok_or_else(|| {
            runtime_error(crate::diagnostic::DiagId::START_NOT_FOUND,
                "executable module requires FUNCTION Start",
                default_span(),
            )
        })?;
    if !start.parameters.is_empty() {
        return Err(runtime_error(crate::diagnostic::DiagId::INVALID_START,
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
        Flow::Return(Some(Value::Error { code, message })) => {
            Err(runtime_error(crate::diagnostic::DiagId::DISPATCH, format!("{code}: {message}"), start.span))
        }
        Flow::Return(Some(_)) => Err(runtime_error(crate::diagnostic::DiagId::INVALID_START,
            "FUNCTION Start must return VOID or INTEGER",
            start.span,
        )),
    }
}

/// Executes one `BNWeb` callback in a fresh interpreter instance.
///
/// A network request must not borrow the `Executor` that registered the
/// server: that executor may be running user code, and its heap is not
/// thread-safe. The callback receives copies of the request/response state
/// and returns the response projection to the transport layer.
pub(crate) fn execute_web_callback(
    module: &Module,
    host: &HostEnv,
    function_name: &str,
    request: crate::web::Request,
    response: crate::web::Response,
) -> Result<crate::web::Response, String> {
    crate::tls::install_ring_provider().map_err(std::borrow::ToOwned::to_owned)?;
    if !host.filesystem.allows_capability() && let Some(span) = module.filesystem_import {
        return Err(runtime_error(crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
            "HOST.FileSystem is not provided by this host",
            span,
        )
        .message.to_string());
    }
    let mut input = std::io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::<u8>::new();
    let mut executor = Executor::new(module, &mut input, &mut output, host, None, None);
    let request_value = executor
        .allocate_object("BNWeb.Request", default_span())
        .map_err(|error| error.message)?;
    let response_value = executor
        .allocate_object("BNWeb.Response", default_span())
        .map_err(|error| error.message)?;
    let request_handle = match &request_value {
        Value::Object { handle, .. } => *handle,
        _ => return Err("BNWeb callback object allocation failed".into()),
    };
    let response_handle = match &response_value {
        Value::Object { handle, .. } => *handle,
        _ => return Err("BNWeb callback object allocation failed".into()),
    };
    executor.web_requests.insert(request_handle, request);
    executor.web_responses.insert(response_handle, response);
    let result = executor
        .call_named(
            function_name,
            vec![request_value, response_value],
            default_span(),
        )
        .map_err(|error| error.message)?;
    if let Value::Error { message, .. } = result {
        return Err(message);
    }
    executor
        .web_responses
        .remove(&response_handle)
        .ok_or_else(|| "BNWeb callback did not retain its response".into())
}

enum Flow {
    Return(Option<Value>),
    Stop(i128),
}
#[path = "runtime/executor.rs"]
mod executor;

#[allow(dead_code)]
fn coerce(value: Value, ty: &Type, span: Span) -> Result<Value, Diagnostic> {
    crate::runtime::executor::coerce(value, ty, span)
}

#[allow(dead_code)]
pub(crate) fn integer_from_i128_count(count: i128, span: Span) -> Result<Value, Diagnostic> {
    if !(0..=i128::from(i32::MAX)).contains(&count) {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(count, IntegerType::Int32))
}

pub(crate) fn runtime_error(
    id: crate::diagnostic::DiagId,
    message: impl Into<String>,
    span: Span,
) -> Diagnostic {
    let message = message.into();
    let arguments = match id {
        crate::diagnostic::DiagId::NAME_NOT_FOUND => vec![
            ("name".into(), message.clone().into()),
            ("context".into(), "runtime lookup".into()),
        ],
        crate::diagnostic::DiagId::INDEX_OUT_OF_BOUNDS => vec![
            ("index".into(), "unknown".into()),
            ("bound".into(), "unknown".into()),
            ("context".into(), message.into()),
        ],
        crate::diagnostic::DiagId::TYPE_MISMATCH => vec![
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
        vec![crate::diagnostic::Label {
            span,
            style: crate::diagnostic::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("runtime compatibility diagnostic schema")
}

pub(crate) fn name_not_found(name: impl Into<String>, context: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::structured(
        crate::diagnostic::DiagId::NAME_NOT_FOUND,
        vec![
            ("name".into(), name.into().into()),
            ("context".into(), context.into().into()),
        ],
        vec![crate::diagnostic::Label {
            span,
            style: crate::diagnostic::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("name-not-found diagnostic schema")
}

pub(crate) fn index_out_of_bounds(
    index: impl std::fmt::Display,
    bound: impl std::fmt::Display,
    context: impl std::fmt::Display,
    span: Span,
) -> Diagnostic {
    Diagnostic::structured(
        crate::diagnostic::DiagId::INDEX_OUT_OF_BOUNDS,
        vec![
            ("index".into(), index.to_string().into()),
            ("bound".into(), bound.to_string().into()),
            ("context".into(), context.to_string().into()),
        ],
        vec![crate::diagnostic::Label {
            span,
            style: crate::diagnostic::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("index-out-of-bounds diagnostic schema")
}

pub(crate) fn type_mismatch(
    expected: impl std::fmt::Display,
    actual: impl std::fmt::Display,
    context: impl std::fmt::Display,
    span: Span,
) -> Diagnostic {
    Diagnostic::structured(
        crate::diagnostic::DiagId::TYPE_MISMATCH,
        vec![
            ("expected".into(), expected.to_string().into()),
            ("actual".into(), actual.to_string().into()),
            ("context".into(), context.to_string().into()),
        ],
        vec![crate::diagnostic::Label {
            span,
            style: crate::diagnostic::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("type-mismatch diagnostic schema")
}

fn console_runtime_error(error: &bn_rt::ConsoleError, span: Span) -> Diagnostic {
    // `bn_rt` reports codes as strings (C ABI boundary); the interpreter owns
    // the mapping onto registry identities.
    let id = match error {
        bn_rt::ConsoleError::Unavailable(_) => crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
        bn_rt::ConsoleError::OutOfBounds => crate::diagnostic::DiagId::INDEX_OUT_OF_BOUNDS,
        bn_rt::ConsoleError::Output(_) => crate::diagnostic::DiagId::OUTPUT_ERROR,
        bn_rt::ConsoleError::Overflow => crate::diagnostic::DiagId::NUMERIC_OVERFLOW,
    };
    debug_assert_eq!(id.code(), error.code());
    runtime_error(id, error.message(), span)
}

fn integer_overflow(span: Span) -> Diagnostic {
    Diagnostic::structured(
        crate::diagnostic::DiagId::NUMERIC_OVERFLOW,
        vec![(
            "operation".into(),
            "converting a value to INTEGER".into(),
        )],
        vec![crate::diagnostic::Label {
            span,
            style: crate::diagnostic::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("numeric-overflow diagnostic schema")
}
fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            source_id: crate::source::Position::UNKNOWN_SOURCE,
            revision: crate::source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            source_id: crate::source::Position::UNKNOWN_SOURCE,
            revision: crate::source::Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
