// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[allow(unused_imports)]
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, Read, Write},
    sync::atomic::AtomicU64,
    time::{SystemTime, UNIX_EPOCH},
};

#[path = "runtime/allocation.rs"]
mod allocation;
#[path = "runtime/collections.rs"]
mod collections;
#[path = "runtime/compare.rs"]
mod compare;
#[path = "runtime/helpers.rs"]
mod helpers;
#[path = "runtime/math.rs"]
mod math;
#[path = "runtime/net_values.rs"]
mod net_values;
#[path = "runtime/numeric.rs"]
mod numeric;
#[path = "runtime/render.rs"]
mod render;
#[path = "runtime/temporal_ops.rs"]
mod temporal_ops;

use allocation::{add_sizes, display_element, pointer_element_default, pointer_element_size};
use collections::{
    collect_indices, dataframe_index_error, dataframe_numeric_values, unsigned_indices,
};
use compare::{equals, is_host_file_method, is_host_file_type, is_value, value_matches_type};
use helpers::{
    constant_value, default_function_owner, empty_named, find_block, require_arity, set, value,
};
use math::reduce_vector;
use net_values::{
    address_value, endpoint_value, net_address, net_addresses, net_endpoint, ping_reply_value,
};
use numeric::{
    boolean, exit_code, float_kind, float_value, integer, integer_kind, integer_range,
    integer_width, is_float_value, number_as_float, ordered, parse_float, parse_integer, parse_val,
};
use render::render;
use temporal_ops::{is_temporal_builtin, temporal_call};

#[allow(unused_imports)]
use crate::{
    dataframe::{
        DataFrameColumn, DataFrameJoin, DataFrameResource, duplicate_column_names, join_dataframes,
        parse_csv,
    },
    diagnostic::Diagnostic,
    heap::{Handle, Heap},
    ir::{Function, Instruction, Module, Terminator, ValidatedModule, ValueId, validate_module},
    module_graph::ModuleId,
    semantic::{
        FloatType, IntegerType, PointerLength, SymbolId, Type, integer_byte_size, static_size_of,
    },
    source::Span,
};

#[derive(Clone, Debug)]
pub(crate) enum Value {
    Integer(i128, IntegerType),
    Float(f64, FloatType),
    Boolean(bool),
    String(String),
    Vector(Vec<Value>),
    Function(String),
    Type(String),
    Null,
    NotAvailable,
    EndOfFile,
    #[allow(dead_code)]
    Error {
        code: i32,
        message: String,
    },
    HostConsole,
    HostArgs,
    TcpStream(u64),
    TcpListener(u64),
    UdpSocket(u64),
    LogFields(u64),
    LogEntry(u64),
    LogLogger(u64),
    Json(u64),
    DispatchQueue(u64),
    DispatchTicket(u64),
    DispatchGroup(u64),
    DispatchBarrier(u64),
    DispatchSemaphore(u64),
    DispatchMutex(u64),
    File(u64),
    DataFrame(u64),
    Handle {
        type_name: String,
    },
    Record {
        type_name: String,
        fields: HashMap<String, Value>,
    },
    Object {
        handle: Handle,
        class: String,
    },
    Pointer {
        handle: Handle,
    },
    Date(i32),
    Time(u32),
    TimeZone(String),
}

/// Host-supplied arguments and clocks for one `bn run` execution.
pub struct HostEnv {
    arguments: Vec<String>,
    clock: ClockKind,
    random_state: AtomicU64,
    filesystem: bool,
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
            filesystem: self.filesystem,
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
            filesystem: true,
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
            filesystem: true,
        }
    }

    /// Creates an environment that denies filesystem capability imports.
    #[must_use]
    pub fn without_filesystem(mut self) -> Self {
        self.filesystem = false;
        self
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

    pub(crate) fn fork_for_task(&self) -> Self {
        let seed = self
            .random_state
            .fetch_add(0x9E37_79B9_7F4A_7C15, std::sync::atomic::Ordering::Relaxed)
            .max(1);
        Self {
            arguments: self.arguments.clone(),
            clock: self.clock.clone(),
            random_state: AtomicU64::new(seed),
            filesystem: self.filesystem,
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
    fields: HashMap<String, Value>,
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
    files: HashMap<u64, FileResource>,
    next_file: u64,
    tcp_streams: HashMap<u64, crate::net::TcpStream>,
    next_tcp_stream: u64,
    tcp_listeners: HashMap<u64, Vec<crate::net::TcpListener>>,
    next_tcp_listener: u64,
    udp_sockets: HashMap<u64, crate::net::UdpSocket>,
    next_udp_socket: u64,
    log_fields: HashMap<u64, HashMap<String, String>>,
    next_log_fields: u64,
    log_entries: HashMap<u64, HashMap<String, String>>,
    next_log_entry: u64,
    log_loggers: HashMap<u64, LogLoggerResource>,
    next_log_logger: u64,
    json_values: HashMap<u64, crate::json::Value>,
    next_json_value: u64,
    dispatch_queues: HashMap<u64, crate::dispatch::Queue>,
    next_dispatch_queue: u64,
    dispatch_tickets: HashMap<u64, crate::dispatch::Ticket>,
    next_dispatch_ticket: u64,
    dispatch_groups: HashMap<u64, crate::dispatch::DispatchGroup>,
    dispatch_barriers: HashMap<u64, crate::dispatch::Barrier>,
    dispatch_semaphores: HashMap<u64, crate::dispatch::DispatchSemaphore>,
    dispatch_mutexes: HashMap<u64, crate::dispatch::DispatchMutex>,
    next_dispatch_sync: u64,
    dataframes: HashMap<u64, DataFrameResource>,
    next_dataframe: u64,
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
            files: HashMap::new(),
            next_file: 1,
            tcp_streams: HashMap::new(),
            next_tcp_stream: 1,
            tcp_listeners: HashMap::new(),
            next_tcp_listener: 1,
            udp_sockets: HashMap::new(),
            next_udp_socket: 1,
            log_fields: HashMap::new(),
            next_log_fields: 1,
            log_entries: HashMap::new(),
            next_log_entry: 1,
            log_loggers: HashMap::new(),
            next_log_logger: 1,
            json_values: HashMap::new(),
            next_json_value: 1,
            dispatch_queues: HashMap::new(),
            next_dispatch_queue: 1,
            dispatch_tickets: HashMap::new(),
            next_dispatch_ticket: 1,
            dispatch_groups: HashMap::new(),
            dispatch_barriers: HashMap::new(),
            dispatch_semaphores: HashMap::new(),
            dispatch_mutexes: HashMap::new(),
            next_dispatch_sync: 1,
            dataframes: HashMap::new(),
            next_dataframe: 1,
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

#[derive(Clone)]
struct LogLoggerResource {
    label: String,
    context: std::collections::BTreeMap<String, String>,
    null_transports: Vec<i128>,
    console_transports: Vec<i128>,
    file_transports: Vec<LogFileTransport>,
    closed: bool,
}

#[derive(Clone)]
struct LogFileTransport {
    path: String,
    minimum: i128,
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
        .map_err(|message| runtime_error("TLS_PROVIDER_UNAVAILABLE", message, default_span()))?;
    if !host.filesystem
        && let Some(span) = module.filesystem_import
    {
        return Err(runtime_error(
            "HOST_CAPABILITY_UNAVAILABLE",
            "HOST.FileSystem is not provided by this host",
            span,
        ));
    }
    let start = module
        .functions
        .iter()
        .find(|function| function.name == "Start")
        .ok_or_else(|| {
            runtime_error(
                "START_NOT_FOUND",
                "executable module requires FUNCTION Start",
                default_span(),
            )
        })?;
    if !start.parameters.is_empty() {
        return Err(runtime_error(
            "INVALID_START",
            "FUNCTION Start cannot declare parameters",
            start.span,
        ));
    }
    let mut executor = Executor::new(module, input, output, host, debug_hook, debug_control);
    match executor.function(start, Vec::new())? {
        Flow::Return(None) => Ok(0),
        Flow::Return(Some(Value::Integer(code, _))) | Flow::Stop(code) => {
            exit_code(code, start.span)
        }
        Flow::Return(Some(_)) => Err(runtime_error(
            "INVALID_START",
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
    if !host.filesystem && let Some(span) = module.filesystem_import {
        return Err(runtime_error(
            "HOST_CAPABILITY_UNAVAILABLE",
            "HOST.FileSystem is not provided by this host",
            span,
        )
        .message);
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
fn integer_from_i128_count(count: i128, span: Span) -> Result<Value, Diagnostic> {
    if !(0..=i128::from(i32::MAX)).contains(&count) {
        return Err(integer_overflow(span));
    }
    Ok(Value::Integer(count, IntegerType::Int32))
}

fn runtime_error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic { code, message: message.into(), span }
}

fn console_runtime_error(error: &bn_rt::ConsoleError, span: Span) -> Diagnostic {
    runtime_error(error.code(), error.message(), span)
}

fn integer_overflow(span: Span) -> Diagnostic {
    runtime_error("NUMERIC_OVERFLOW", "result does not fit INTEGER", span)
}
fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
