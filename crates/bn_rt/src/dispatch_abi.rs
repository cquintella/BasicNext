//! Stable C representation shared by compiled `BNDispatch` code and `bn_rt`.
#![allow(unsafe_code)]
#![allow(clippy::too_many_lines)]

use crate::dispatch_error::DispatchFailure;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

pub type BNDispatchHandle = u64;
pub type BNDispatchStatus = u32;
pub type BNDispatchTaskFn = extern "C" fn(
    *mut c_void,
    *const BNValue,
    u32,
    *mut BNValue,
    *mut BNDispatchError,
) -> BNDispatchStatus;

pub const BN_DISPATCH_OK: BNDispatchStatus = 0;
pub const BN_DISPATCH_ERROR: BNDispatchStatus = 1;
pub const BN_DISPATCH_TIMEOUT: BNDispatchStatus = 2;
pub const BN_DISPATCH_CANCELLED: BNDispatchStatus = 3;
pub const BN_DISPATCH_CLOSED: BNDispatchStatus = 4;
pub const BN_DISPATCH_INVALID_HANDLE: BNDispatchStatus = 5;
pub const BN_DISPATCH_LIMIT: BNDispatchStatus = 6;
pub const BN_DISPATCH_POLICY_DENIED: BNDispatchStatus = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BNValueKind {
    Null = 0,
    Boolean = 1,
    Integer = 2,
    Float = 3,
    String = 4,
    Bytes = 5,
    Handle = 6,
    NotAvailable = 7,
    EndOfFile = 8,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BNValueBytes {
    pub data: *const u8,
    pub length: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union BNValuePayload {
    pub integer: i64,
    pub floating: f64,
    pub boolean: u8,
    pub bytes: BNValueBytes,
    pub handle: BNDispatchHandle,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BNValue {
    pub kind: BNValueKind,
    pub flags: u32,
    pub payload: BNValuePayload,
}

impl BNValue {
    #[must_use]
    pub const fn null() -> Self {
        Self {
            kind: BNValueKind::Null,
            flags: 0,
            payload: BNValuePayload { integer: 0 },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BNDispatchError {
    pub code: u32,
    pub message: *mut c_char,
    pub message_length: u32,
}

// ABI values are copied into a task-owned context before crossing the worker
// boundary. Pointer payloads are only borrowed for the duration of the task.
unsafe impl Send for BNValue {}
unsafe impl Sync for BNValue {}
unsafe impl Send for BNDispatchError {}
unsafe impl Sync for BNDispatchError {}

struct TicketState {
    done: bool,
    cancelled: bool,
    running: bool,
    result: BNValue,
    error: BNDispatchError,
}

struct Ticket {
    state: Mutex<TicketState>,
    wake: Condvar,
}

struct Queue {
    closed: AtomicBool,
    workers: u32,
    active: Mutex<u32>,
    idle: Condvar,
    tickets: Mutex<Vec<BNDispatchHandle>>,
}

struct Registry {
    next: AtomicU64,
    queues: Mutex<HashMap<BNDispatchHandle, Arc<Queue>>>,
    tickets: Mutex<HashMap<BNDispatchHandle, Arc<Ticket>>>,
    closed_tickets: Mutex<std::collections::HashSet<BNDispatchHandle>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        next: AtomicU64::new(1),
        queues: Mutex::new(HashMap::new()),
        tickets: Mutex::new(HashMap::new()),
        closed_tickets: Mutex::new(std::collections::HashSet::new()),
    })
}

pub(crate) fn next_handle() -> BNDispatchHandle {
    registry().next.fetch_add(1, Ordering::Relaxed)
}

/// Records `failure` of `operation` for the emitted code and returns its
/// status (bndispatch.md "Errors").
fn failed(operation: &str, failure: &DispatchFailure) -> BNDispatchStatus {
    crate::set_error_report(
        failure.code(),
        operation,
        failure.message(),
        failure.cause(),
    );
    match failure {
        DispatchFailure::Timeout { .. } => BN_DISPATCH_TIMEOUT,
        DispatchFailure::Closed(_) => BN_DISPATCH_CLOSED,
        DispatchFailure::Cancelled => BN_DISPATCH_CANCELLED,
        _ => BN_DISPATCH_ERROR,
    }
}

thread_local! {
    /// The queue whose task this thread runs; 0 outside a worker.
    static CURRENT_QUEUE: std::cell::Cell<BNDispatchHandle> = const { std::cell::Cell::new(0) };
}

/// `Queue.Serial()` and `Queue.Concurrent(workers)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_queue_create(
    workers: i64,
    out_queue: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    match crate::dispatch_sync::worker_count(i128::from(workers)) {
        Ok(workers) => new_queue(workers, out_queue),
        Err(failure) => failed("BNDispatch.Queue.Concurrent", &failure),
    }
}

/// `Queue.Auto()`: one worker per available processor.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_queue_create_auto(
    out_queue: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    match crate::dispatch_sync::auto_worker_count() {
        Ok(workers) => new_queue(workers, out_queue),
        Err(failure) => failed("BNDispatch.Queue.Auto", &failure),
    }
}

fn new_queue(workers: usize, out_queue: *mut BNDispatchHandle) -> BNDispatchStatus {
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    if out_queue.is_null() {
        return BN_DISPATCH_ERROR;
    }
    let workers = u32::try_from(workers).unwrap_or(u32::MAX);
    let handle = next_handle();
    registry()
        .queues
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            handle,
            Arc::new(Queue {
                closed: AtomicBool::new(false),
                workers,
                active: Mutex::new(0),
                idle: Condvar::new(),
                tickets: Mutex::new(Vec::new()),
            }),
        );
    #[allow(unsafe_code)]
    unsafe {
        *out_queue = handle;
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_submit(
    queue: BNDispatchHandle,
    task: Option<BNDispatchTaskFn>,
    context: *mut c_void,
    arguments: *const BNValue,
    argument_count: u32,
    out_ticket: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    if out_ticket.is_null() || task.is_none() || (argument_count > 0 && arguments.is_null()) {
        return BN_DISPATCH_ERROR;
    }
    let queue_ref = registry()
        .queues
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&queue)
        .cloned();
    let Some(queue_ref) = queue_ref else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    if queue_ref.closed.load(Ordering::Acquire) {
        return failed("BNDispatch.Queue.Async", &DispatchFailure::Closed("queue"));
    }
    let args = if argument_count == 0 {
        Vec::new()
    } else {
        #[allow(unsafe_code)]
        unsafe {
            std::slice::from_raw_parts(arguments, argument_count as usize).to_vec()
        }
    };
    let ticket_handle = next_handle();
    let ticket = Arc::new(Ticket {
        state: Mutex::new(TicketState {
            done: false,
            cancelled: false,
            running: false,
            result: BNValue::null(),
            error: BNDispatchError::empty(),
        }),
        wake: Condvar::new(),
    });
    registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(ticket_handle, Arc::clone(&ticket));
    queue_ref
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(ticket_handle);
    let Some(task) = task else {
        return BN_DISPATCH_ERROR;
    };
    let context = context as usize;
    let workers = queue_ref.workers;
    thread::spawn(move || {
        CURRENT_QUEUE.with(|current| current.set(queue));
        // A queue may create lightweight waiting threads, but only `workers`
        // callbacks execute at once. This keeps the ABI deterministic without
        // introducing a dependency on a particular executor implementation.
        let mut active = queue_ref
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *active >= workers && !queue_ref.closed.load(Ordering::Acquire) {
            active = queue_ref
                .idle
                .wait(active)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if queue_ref.closed.load(Ordering::Acquire) {
            let mut state = ticket
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.done = true;
            state.running = false;
            state.error.code = BN_DISPATCH_CLOSED;
            ticket.wake.notify_all();
            return;
        }
        *active += 1;
        drop(active);

        let mut state = ticket
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.cancelled {
            state.done = true;
            state.running = false;
            ticket.wake.notify_all();
            drop(state);
            let mut active = queue_ref
                .active
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *active = active.saturating_sub(1);
            queue_ref.idle.notify_all();
            return;
        }
        state.running = true;
        drop(state);
        let mut result = BNValue::null();
        let mut error = BNDispatchError::empty();
        let status = task(
            context as *mut c_void,
            args.as_ptr(),
            u32::try_from(args.len()).unwrap_or(u32::MAX),
            &raw mut result,
            &raw mut error,
        );
        state = ticket
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.done = true;
        state.running = false;
        if !state.cancelled {
            state.result = result;
            state.error = error;
            if status != BN_DISPATCH_OK && state.error.code == 0 {
                state.error.code = status;
            }
        }
        ticket.wake.notify_all();
        let mut active = queue_ref
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *active = active.saturating_sub(1);
        queue_ref.idle.notify_all();
    });
    #[allow(unsafe_code)]
    unsafe {
        *out_ticket = ticket_handle;
    }
    BN_DISPATCH_OK
}

/// `AWAIT ticket(timeoutMs)`: the worker's result through `out_result`, or
/// the recorded failure (bndispatch.md "Errors").
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_await(
    ticket: BNDispatchHandle,
    timeout_ms: i64,
    out_result: *mut BNValue,
    out_error: *mut BNDispatchError,
) -> BNDispatchStatus {
    const OPERATION: &str = "BNDispatch.Ticket.Wait";
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    let ticket_ref = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&ticket)
        .cloned();
    let Some(ticket_ref) = ticket_ref else {
        let closed = registry()
            .closed_tickets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&ticket);
        return if closed {
            failed(OPERATION, &DispatchFailure::Closed("ticket"))
        } else {
            BN_DISPATCH_INVALID_HANDLE
        };
    };
    let deadline = match crate::dispatch_sync::deadline(i128::from(timeout_ms)) {
        Ok(deadline) => deadline,
        Err(failure) => return failed(OPERATION, &failure),
    };
    let mut state = ticket_ref
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    while !state.done {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return failed(
                OPERATION,
                &DispatchFailure::Timeout {
                    ms: i128::from(timeout_ms),
                },
            );
        }
        (state, _) = ticket_ref
            .wake
            .wait_timeout(state, remaining)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    #[allow(unsafe_code)]
    unsafe {
        if !out_result.is_null() {
            *out_result = state.result;
        }
        if !out_error.is_null() {
            *out_error = state.error;
        }
    }
    if state.cancelled || state.error.code == BN_DISPATCH_CLOSED {
        // `Cancel`, or the queue's `Close`, removed the task before it ran.
        return failed(OPERATION, &DispatchFailure::Cancelled);
    }
    if state.error.code != 0 {
        let (code, message) =
            crate::error_abi::code_and_message(state.error.message, i64::from(state.error.code));
        return failed(OPERATION, &DispatchFailure::TaskFailed { code, message });
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_cancel(ticket: BNDispatchHandle) -> BNDispatchStatus {
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    let ticket_ref = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&ticket)
        .cloned();
    let Some(ticket_ref) = ticket_ref else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    let mut state = ticket_ref
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.done {
        return BN_DISPATCH_CLOSED;
    }
    state.cancelled = true;
    state.done = true;
    state.error.code = BN_DISPATCH_CANCELLED;
    ticket_ref.wake.notify_all();
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_close(ticket: BNDispatchHandle) -> BNDispatchStatus {
    if registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&ticket)
        .is_some()
    {
        registry()
            .closed_tickets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(ticket);
        BN_DISPATCH_OK
    } else if registry()
        .closed_tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&ticket)
    {
        BN_DISPATCH_OK
    } else {
        BN_DISPATCH_INVALID_HANDLE
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_cancel(
    ticket: BNDispatchHandle,
    out_cancelled: *mut i32,
) -> BNDispatchStatus {
    const OPERATION: &str = "BNDispatch.Ticket.Cancel";
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    let ticket_ref = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&ticket)
        .cloned();
    let Some(ticket_ref) = ticket_ref else {
        let closed = registry()
            .closed_tickets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&ticket);
        return if closed {
            failed(OPERATION, &DispatchFailure::Closed("ticket"))
        } else {
            BN_DISPATCH_INVALID_HANDLE
        };
    };
    let mut state = ticket_ref
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.cancelled || state.done || state.running {
        #[allow(unsafe_code)]
        unsafe {
            if !out_cancelled.is_null() {
                *out_cancelled = 0;
            }
        }
        return BN_DISPATCH_OK;
    }
    state.cancelled = true;
    state.done = true;
    state.error.code = BN_DISPATCH_CANCELLED;
    ticket_ref.wake.notify_all();
    #[allow(unsafe_code)]
    unsafe {
        if !out_cancelled.is_null() {
            *out_cancelled = 1;
        }
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_id(ticket: BNDispatchHandle) -> i64 {
    ticket.cast_signed()
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_status(ticket: BNDispatchHandle) -> i32 {
    let ticket_ref = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&ticket)
        .cloned();
    let Some(ticket_ref) = ticket_ref else {
        return if registry()
            .closed_tickets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains(&ticket)
        {
            2 // COMPLETED
        } else {
            0 // PENDING
        };
    };
    let state = ticket_ref
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.cancelled {
        4 // CANCELLED
    } else if state.done {
        if state.error.code != 0 {
            3 // FAILED
        } else {
            2 // COMPLETED
        }
    } else {
        i32::from(state.running)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_is_done(ticket: BNDispatchHandle) -> i32 {
    let ticket_ref = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&ticket)
        .cloned();
    let Some(ticket_ref) = ticket_ref else {
        return i32::from(
            registry()
                .closed_tickets
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .contains(&ticket),
        );
    };
    let state = ticket_ref
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    i32::from(state.done)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_error(
    ticket: BNDispatchHandle,
    out_msg: *mut *const c_char,
    out_code: *mut i64,
) -> i32 {
    let ticket_ref = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&ticket)
        .cloned();
    let Some(ticket_ref) = ticket_ref else {
        return 0;
    };
    let state = ticket_ref
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.cancelled {
        crate::set_error_report(
            DispatchFailure::Cancelled.code(),
            "BNDispatch.Ticket.Error",
            DispatchFailure::Cancelled.message(),
            DispatchFailure::Cancelled.cause(),
        );
        let record = crate::error_abi::bn_rt_error_take(1, std::ptr::null());
        #[allow(unsafe_code)]
        unsafe {
            if !out_msg.is_null() {
                *out_msg = record;
            }
            if !out_code.is_null() {
                *out_code = crate::error_abi::bn_rt_error_code(record);
            }
        }
        return 1;
    }
    if state.error.code != 0 {
        let (code, message) =
            crate::error_abi::code_and_message(state.error.message, i64::from(state.error.code));
        let failure = DispatchFailure::TaskFailed { code, message };
        crate::set_error_report(
            failure.code(),
            "BNDispatch.Ticket.Error",
            failure.message(),
            failure.cause(),
        );
        let record = crate::error_abi::bn_rt_error_take(1, std::ptr::null());
        #[allow(unsafe_code)]
        unsafe {
            if !out_msg.is_null() {
                *out_msg = record;
            }
            if !out_code.is_null() {
                *out_code = crate::error_abi::bn_rt_error_code(record);
            }
        }
        return 1;
    }
    0
}

/// `queue.Close(timeoutMs)`: cancels pending tickets, then waits for the
/// running ones.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_queue_close(
    queue: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    // The closed queue stays registered, so a later submit reports `CLOSED`
    // rather than an unknown handle.
    wait_for_queue(queue, timeout_ms, "BNDispatch.Queue.Close", true)
}

/// `queue.Join(timeoutMs)`: waits until every ticket is done.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_queue_join(
    queue: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    wait_for_queue(queue, timeout_ms, "BNDispatch.Queue.Join", false)
}

/// Waits until every ticket of `queue` is done, first closing it when
/// `close` is set.
fn wait_for_queue(
    queue: BNDispatchHandle,
    timeout_ms: i64,
    operation: &str,
    close: bool,
) -> BNDispatchStatus {
    let queue_ref = registry()
        .queues
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&queue)
        .cloned();
    let Some(queue_ref) = queue_ref else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    if CURRENT_QUEUE.with(std::cell::Cell::get) == queue {
        return failed(operation, &DispatchFailure::SelfWait);
    }
    let deadline = match crate::dispatch_sync::deadline(i128::from(timeout_ms)) {
        Ok(deadline) => deadline,
        Err(failure) => return failed(operation, &failure),
    };
    if close {
        queue_ref.closed.store(true, Ordering::Release);
        queue_ref.idle.notify_all();
    }
    loop {
        let handles = queue_ref
            .tickets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let all_done = handles.iter().all(|handle| {
            registry()
                .tickets
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(handle)
                .is_none_or(|ticket| {
                    ticket
                        .state
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .done
                })
        });
        if all_done {
            return BN_DISPATCH_OK;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return failed(
                operation,
                &DispatchFailure::Timeout {
                    ms: i128::from(timeout_ms),
                },
            );
        }
        let active = queue_ref
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = queue_ref
            .idle
            .wait_timeout(active, remaining.min(Duration::from_millis(1)))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

impl BNDispatchError {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            code: 0,
            message: std::ptr::null_mut(),
            message_length: 0,
        }
    }
}

/// Releases the bounded message owned by an ABI error structure.
#[allow(unsafe_code, clippy::same_length_and_capacity)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_error_free(error: *mut BNDispatchError) {
    if error.is_null() {
        return;
    }
    unsafe {
        let error = &mut *error;
        if !error.message.is_null() {
            let length = usize::try_from(error.message_length).unwrap_or(0);
            drop(Vec::from_raw_parts(
                error.message.cast::<u8>(),
                length,
                length,
            ));
        }
        *error = BNDispatchError::empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn completed_task(
        _context: *mut c_void,
        _arguments: *const BNValue,
        _argument_count: u32,
        result: *mut BNValue,
        _error: *mut BNDispatchError,
    ) -> BNDispatchStatus {
        // The test callback writes only a scalar ABI value.
        #[allow(unsafe_code)]
        unsafe {
            *result = BNValue {
                kind: BNValueKind::Integer,
                flags: 0,
                payload: BNValuePayload { integer: 42 },
            };
        }
        BN_DISPATCH_OK
    }

    extern "C" fn slow_task(
        _context: *mut c_void,
        _arguments: *const BNValue,
        _argument_count: u32,
        result: *mut BNValue,
        _error: *mut BNDispatchError,
    ) -> BNDispatchStatus {
        std::thread::sleep(Duration::from_millis(20));
        #[allow(unsafe_code)]
        unsafe {
            *result = BNValue {
                kind: BNValueKind::Integer,
                flags: 0,
                payload: BNValuePayload { integer: 7 },
            };
        }
        BN_DISPATCH_OK
    }

    #[test]
    fn status_values_are_stable() {
        assert_eq!(BN_DISPATCH_OK, 0);
        assert_eq!(BN_DISPATCH_TIMEOUT, 2);
        assert_eq!(BN_DISPATCH_LIMIT, 6);
    }

    #[test]
    fn null_value_has_a_deterministic_payload() {
        let value = BNValue::null();
        assert_eq!(value.kind, BNValueKind::Null);
        assert_eq!(value.flags, 0);
    }

    #[test]
    fn error_free_accepts_null_and_clears_owned_storage() {
        bn_rt_dispatch_error_free(std::ptr::null_mut());
        let mut error = BNDispatchError::empty();
        bn_rt_dispatch_error_free(&raw mut error);
        assert!(error.message.is_null());
    }

    #[test]
    fn queue_submit_and_await_return_a_scalar_result() {
        let mut queue = 0;
        assert_eq!(
            bn_rt_dispatch_queue_create(1, &raw mut queue),
            BN_DISPATCH_OK
        );
        let mut ticket = 0;
        assert_eq!(
            bn_rt_dispatch_submit(
                queue,
                Some(completed_task),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &raw mut ticket
            ),
            BN_DISPATCH_OK
        );
        let mut result = BNValue::null();
        let mut error = BNDispatchError::empty();
        assert_eq!(
            bn_rt_dispatch_await(ticket, 1_000, &raw mut result, &raw mut error),
            BN_DISPATCH_OK
        );
        assert_eq!(result.kind, BNValueKind::Integer);
        #[allow(unsafe_code)]
        unsafe {
            assert_eq!(result.payload.integer, 42);
        }
        assert_eq!(bn_rt_dispatch_ticket_close(ticket), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_queue_close(queue, 1_000), BN_DISPATCH_OK);
    }

    #[test]
    fn await_timeout_does_not_destroy_the_ticket() {
        let mut queue = 0;
        assert_eq!(
            bn_rt_dispatch_queue_create(1, &raw mut queue),
            BN_DISPATCH_OK
        );
        let mut ticket = 0;
        assert_eq!(
            bn_rt_dispatch_submit(
                queue,
                Some(slow_task),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &raw mut ticket
            ),
            BN_DISPATCH_OK
        );
        let mut result = BNValue::null();
        let mut error = BNDispatchError::empty();
        assert_eq!(
            bn_rt_dispatch_await(ticket, 1, &raw mut result, &raw mut error),
            BN_DISPATCH_TIMEOUT
        );
        assert_eq!(
            bn_rt_dispatch_await(ticket, 1_000, &raw mut result, &raw mut error),
            BN_DISPATCH_OK
        );
        assert_eq!(bn_rt_dispatch_ticket_close(ticket), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_queue_close(queue, 1_000), BN_DISPATCH_OK);
    }

    #[test]
    fn cancellation_before_start_is_reported_and_ticket_close_is_idempotent() {
        let mut queue = 0;
        assert_eq!(
            bn_rt_dispatch_queue_create(1, &raw mut queue),
            BN_DISPATCH_OK
        );
        let mut first = 0;
        assert_eq!(
            bn_rt_dispatch_submit(
                queue,
                Some(slow_task),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &raw mut first
            ),
            BN_DISPATCH_OK
        );
        let mut second = 0;
        assert_eq!(
            bn_rt_dispatch_submit(
                queue,
                Some(completed_task),
                std::ptr::null_mut(),
                std::ptr::null(),
                0,
                &raw mut second
            ),
            BN_DISPATCH_OK
        );
        assert_eq!(bn_rt_dispatch_cancel(second), BN_DISPATCH_OK);
        let mut result = BNValue::null();
        let mut error = BNDispatchError::empty();
        assert_eq!(
            bn_rt_dispatch_await(second, 1_000, &raw mut result, &raw mut error),
            BN_DISPATCH_CANCELLED
        );
        assert_eq!(error.code, BN_DISPATCH_CANCELLED);
        // The emitted code reads the recorded `Error` (bndispatch.md "Errors").
        let record = crate::error_abi::bn_rt_error_take(1, std::ptr::null());
        assert_eq!(
            crate::error_abi::bn_rt_error_code(record),
            i64::from(bn_types::error_codes::dispatch::CANCELLED)
        );
        assert_eq!(bn_rt_dispatch_ticket_close(second), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_ticket_close(second), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_queue_close(queue, 1_000), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_ticket_close(first), BN_DISPATCH_OK);
    }

    #[test]
    fn c_abi_value_layout_keeps_discriminant_flags_and_payload_stable() {
        use std::mem::{align_of, offset_of, size_of};

        assert_eq!(size_of::<BNValueKind>(), size_of::<u32>());
        assert_eq!(offset_of!(BNValue, kind), 0);
        assert_eq!(offset_of!(BNValue, flags), size_of::<BNValueKind>());
        assert_eq!(
            offset_of!(BNValue, payload),
            size_of::<BNValueKind>() + size_of::<u32>()
        );
        assert_eq!(
            size_of::<BNValue>(),
            offset_of!(BNValue, payload) + size_of::<BNValuePayload>()
        );
        assert_eq!(align_of::<BNValue>(), align_of::<BNValuePayload>());
    }

    #[test]
    fn c_abi_error_layout_places_owned_message_after_code() {
        use std::mem::{align_of, offset_of, size_of};

        assert_eq!(offset_of!(BNDispatchError, code), 0);
        assert!(offset_of!(BNDispatchError, message) >= size_of::<u32>());
        assert_eq!(
            offset_of!(BNDispatchError, message) % align_of::<*mut c_char>(),
            0
        );
        assert_eq!(
            offset_of!(BNDispatchError, message_length),
            offset_of!(BNDispatchError, message) + size_of::<*mut c_char>()
        );
    }
}
