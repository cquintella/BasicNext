//! Stable C representation shared by compiled `BNDispatch` code and `bn_rt`.
#![allow(unsafe_code)]
#![allow(clippy::too_many_lines)]

use crate::dispatch_error::DispatchFailure;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

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
#[derive(Clone, Copy, Debug)]
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

impl std::fmt::Debug for BNValuePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BNValuePayload").finish_non_exhaustive()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BNValue {
    pub kind: BNValueKind,
    pub flags: u32,
    pub payload: BNValuePayload,
}

impl std::fmt::Debug for BNValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("BNValue");
        d.field("kind", &self.kind);
        d.field("flags", &self.flags);
        match self.kind {
            BNValueKind::Integer => unsafe { d.field("integer", &self.payload.integer) },
            BNValueKind::Float => unsafe { d.field("floating", &self.payload.floating) },
            BNValueKind::Boolean => unsafe { d.field("boolean", &self.payload.boolean) },
            BNValueKind::Bytes => unsafe { d.field("bytes_len", &self.payload.bytes.length) },
            BNValueKind::Handle => unsafe { d.field("handle", &self.payload.handle) },
            _ => &mut d,
        };
        d.finish()
    }
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
#[derive(Clone, Copy, Debug)]
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

/// What a task left for `AWAIT`. Created before the task is submitted and
/// shared with it, so a task that finishes before `submit_with` returns
/// still records its result.
struct Outcome {
    result: Mutex<BNValue>,
    error: Mutex<BNDispatchError>,
}

struct Ticket {
    core: bn_core_dispatch::Ticket,
    outcome: Arc<Outcome>,
}

struct Queue {
    core: bn_core_dispatch::Queue,
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
        DispatchFailure::Saturated { .. } => BN_DISPATCH_LIMIT,
        _ => BN_DISPATCH_ERROR,
    }
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
    let handle = next_handle();
    let core_queue =
        match bn_core_dispatch::Queue::new(i128::try_from(workers).unwrap_or(1), handle) {
            Ok(queue) => queue,
            Err(failure) => return failed("BNDispatch.Queue.Create", &failure),
        };
    registry()
        .queues
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            handle,
            Arc::new(Queue {
                core: core_queue,
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
    if queue_ref.core.is_closed() {
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
    let Some(task) = task else {
        return BN_DISPATCH_ERROR;
    };
    let context = context as usize;
    let outcome = Arc::new(Outcome {
        result: Mutex::new(BNValue::null()),
        error: Mutex::new(BNDispatchError::empty()),
    });
    let task_outcome = Arc::clone(&outcome);
    let submit_res =
        queue_ref
            .core
            .submit_with(format!("task_{ticket_handle}"), move |core_ticket| {
                if core_ticket.status() == bn_core_dispatch::CANCELLED {
                    task_outcome
                        .error
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .code = BN_DISPATCH_CANCELLED;
                    return;
                }
                let mut result = BNValue::null();
                let mut error = BNDispatchError::empty();
                let status = task(
                    context as *mut c_void,
                    args.as_ptr(),
                    u32::try_from(args.len()).unwrap_or(u32::MAX),
                    &raw mut result,
                    &raw mut error,
                );
                if core_ticket.status() != bn_core_dispatch::CANCELLED {
                    *task_outcome
                        .result
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = result;
                    let mut err = task_outcome
                        .error
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    *err = error;
                    if status != BN_DISPATCH_OK && err.code == 0 {
                        err.code = status;
                    }
                }
            });
    let core_ticket = match submit_res {
        Ok(t) => t,
        Err(failure) => return failed("BNDispatch.Queue.Async", &failure),
    };
    let ticket_storage = Arc::new(Ticket {
        core: core_ticket,
        outcome,
    });
    registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(ticket_handle, ticket_storage);
    queue_ref
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(ticket_handle);

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
    if let Err(failure) = ticket_ref.core.wait(i128::from(timeout_ms)) {
        if matches!(failure, DispatchFailure::Cancelled) {
            #[allow(unsafe_code)]
            unsafe {
                if !out_error.is_null() {
                    (*out_error).code = BN_DISPATCH_CANCELLED;
                }
            }
        }
        return failed(OPERATION, &failure);
    }
    let result = *ticket_ref
        .outcome
        .result
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let error = *ticket_ref
        .outcome
        .error
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    #[allow(unsafe_code)]
    unsafe {
        if !out_result.is_null() {
            *out_result = result;
        }
        if !out_error.is_null() {
            *out_error = error;
        }
    }
    if error.code == BN_DISPATCH_CANCELLED || error.code == BN_DISPATCH_CLOSED {
        return failed(OPERATION, &DispatchFailure::Cancelled);
    }
    if error.code != 0 {
        let (code, message) =
            crate::error_abi::code_and_message(error.message, i64::from(error.code));
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
    if ticket_ref.core.is_done() {
        return BN_DISPATCH_CLOSED;
    }
    match ticket_ref.core.cancel() {
        Ok(true) => {
            let mut err = ticket_ref
                .outcome
                .error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            err.code = BN_DISPATCH_CANCELLED;
            BN_DISPATCH_OK
        }
        Ok(false) => {
            // Already running or completed
            BN_DISPATCH_CLOSED
        }
        Err(failure) => failed("BNDispatch.Ticket.Cancel", &failure),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_ticket_close(ticket: BNDispatchHandle) -> BNDispatchStatus {
    let removed = registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&ticket);
    if let Some(t) = removed {
        t.core.close();
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
    match ticket_ref.core.cancel() {
        Ok(cancelled) => {
            if cancelled {
                let mut err = ticket_ref
                    .outcome
                    .error
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                err.code = BN_DISPATCH_CANCELLED;
            }
            #[allow(unsafe_code)]
            unsafe {
                if !out_cancelled.is_null() {
                    *out_cancelled = i32::from(cancelled);
                }
            }
            BN_DISPATCH_OK
        }
        Err(failure) => failed(OPERATION, &failure),
    }
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
            bn_core_dispatch::COMPLETED
        } else {
            bn_core_dispatch::PENDING
        };
    };
    let status = ticket_ref.core.status();
    if status == bn_core_dispatch::COMPLETED {
        let err = ticket_ref
            .outcome
            .error
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if err.code != 0 {
            return bn_core_dispatch::FAILED;
        }
    }
    status
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
    i32::from(ticket_ref.core.is_done())
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
    if ticket_ref.core.status() == bn_core_dispatch::CANCELLED {
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
    let error = *ticket_ref
        .outcome
        .error
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if error.code != 0 {
        let (code, message) =
            crate::error_abi::code_and_message(error.message, i64::from(error.code));
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
    let queue_ref = registry()
        .queues
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&queue)
        .cloned();
    let Some(queue_ref) = queue_ref else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    match queue_ref.core.close(i128::from(timeout_ms)) {
        Ok(()) => BN_DISPATCH_OK,
        Err(failure) => failed("BNDispatch.Queue.Close", &failure),
    }
}

/// `queue.Join(timeoutMs)`: waits until every ticket is done.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_queue_join(
    queue: BNDispatchHandle,
    timeout_ms: i64,
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
    match queue_ref.core.join(i128::from(timeout_ms)) {
        Ok(()) => BN_DISPATCH_OK,
        Err(failure) => failed("BNDispatch.Queue.Join", &failure),
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
    use std::time::Duration;

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
