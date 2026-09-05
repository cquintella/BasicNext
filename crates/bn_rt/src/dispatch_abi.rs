//! Stable C representation shared by compiled `BNDispatch` code and `bn_rt`.
#![allow(unsafe_code)]
#![allow(clippy::too_many_lines)]

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

struct Group {
    tickets: Mutex<Vec<BNDispatchHandle>>,
}

struct BarrierHandle {
    barrier: std::sync::Barrier,
}

struct SemaphoreHandle {
    permits: Mutex<u32>,
    wake: Condvar,
}

struct MutexHandle {
    locked: Mutex<bool>,
    wake: Condvar,
}

struct Registry {
    next: AtomicU64,
    queues: Mutex<HashMap<BNDispatchHandle, Arc<Queue>>>,
    tickets: Mutex<HashMap<BNDispatchHandle, Arc<Ticket>>>,
    closed_tickets: Mutex<std::collections::HashSet<BNDispatchHandle>>,
    groups: Mutex<HashMap<BNDispatchHandle, Arc<Group>>>,
    barriers: Mutex<HashMap<BNDispatchHandle, Arc<BarrierHandle>>>,
    semaphores: Mutex<HashMap<BNDispatchHandle, Arc<SemaphoreHandle>>>,
    mutexes: Mutex<HashMap<BNDispatchHandle, Arc<MutexHandle>>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        next: AtomicU64::new(1),
        queues: Mutex::new(HashMap::new()),
        tickets: Mutex::new(HashMap::new()),
        closed_tickets: Mutex::new(std::collections::HashSet::new()),
        groups: Mutex::new(HashMap::new()),
        barriers: Mutex::new(HashMap::new()),
        semaphores: Mutex::new(HashMap::new()),
        mutexes: Mutex::new(HashMap::new()),
    })
}

fn next_handle() -> BNDispatchHandle {
    registry().next.fetch_add(1, Ordering::Relaxed)
}

fn dispatch_limits() -> (u32, u32) {
    (1, 64)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_queue_create(
    workers: u32,
    out_queue: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    if out_queue.is_null() {
        return BN_DISPATCH_ERROR;
    }
    let (minimum, maximum) = dispatch_limits();
    if !(minimum..=maximum).contains(&workers) {
        return BN_DISPATCH_LIMIT;
    }
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
        return BN_DISPATCH_CLOSED;
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

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_await(
    ticket: BNDispatchHandle,
    timeout_ms: i64,
    out_result: *mut BNValue,
    out_error: *mut BNDispatchError,
) -> BNDispatchStatus {
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
    if !state.done {
        if timeout_ms < 0 {
            return BN_DISPATCH_TIMEOUT;
        }
        let timeout = Duration::from_millis(timeout_ms.cast_unsigned());
        let started = Instant::now();
        while !state.done {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return BN_DISPATCH_TIMEOUT;
            }
            let (next, timed) = ticket_ref
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next;
            if timed.timed_out() && !state.done {
                return BN_DISPATCH_TIMEOUT;
            }
        }
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
    if state.error.code != 0 {
        BN_DISPATCH_ERROR
    } else {
        BN_DISPATCH_OK
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_cancel(ticket: BNDispatchHandle) -> BNDispatchStatus {
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
    queue_ref.closed.store(true, Ordering::Release);
    let joined = bn_rt_dispatch_queue_join(queue, timeout_ms);
    if joined != BN_DISPATCH_OK {
        return joined;
    }
    registry()
        .queues
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&queue);
    BN_DISPATCH_OK
}

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
    let deadline = timeout_ms
        .checked_nonnegative_duration()
        .map(|duration| Instant::now() + duration);
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
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return BN_DISPATCH_TIMEOUT;
        }
        let wait = deadline.map_or(Duration::from_millis(1), |deadline| {
            deadline.saturating_duration_since(Instant::now())
        });
        let active = queue_ref
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = queue_ref
            .idle
            .wait_timeout(active, wait)
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

trait NonnegativeDuration {
    fn checked_nonnegative_duration(self) -> Option<Duration>;
}

impl NonnegativeDuration for i64 {
    fn checked_nonnegative_duration(self) -> Option<Duration> {
        (self >= 0).then(|| Duration::from_millis(self.cast_unsigned()))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_create(out: *mut BNDispatchHandle) -> BNDispatchStatus {
    if out.is_null() {
        return BN_DISPATCH_ERROR;
    }
    let handle = next_handle();
    registry()
        .groups
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            handle,
            Arc::new(Group {
                tickets: Mutex::new(Vec::new()),
            }),
        );
    unsafe {
        *out = handle;
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_add(
    group: BNDispatchHandle,
    ticket: BNDispatchHandle,
) -> BNDispatchStatus {
    let Some(group) = registry()
        .groups
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&group)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    if !registry()
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains_key(&ticket)
    {
        return BN_DISPATCH_INVALID_HANDLE;
    }
    group
        .tickets
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(ticket);
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_wait(
    group: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    let Some(group) = registry()
        .groups
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&group)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    let deadline = timeout_ms
        .checked_nonnegative_duration()
        .map(|d| Instant::now() + d);
    loop {
        let handles = group
            .tickets
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if handles.iter().all(|handle| {
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
        }) {
            return BN_DISPATCH_OK;
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return BN_DISPATCH_TIMEOUT;
        }
        thread::yield_now();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_close(group: BNDispatchHandle) -> BNDispatchStatus {
    registry()
        .groups
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&group)
        .map_or(BN_DISPATCH_INVALID_HANDLE, |_| BN_DISPATCH_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_barrier_create(
    parties: u32,
    out: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    if out.is_null() || parties == 0 {
        return BN_DISPATCH_ERROR;
    }
    let handle = next_handle();
    registry()
        .barriers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            handle,
            Arc::new(BarrierHandle {
                barrier: std::sync::Barrier::new(parties as usize),
            }),
        );
    unsafe {
        *out = handle;
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_barrier_wait(
    barrier: BNDispatchHandle,
    _timeout_ms: i64,
) -> BNDispatchStatus {
    let Some(barrier) = registry()
        .barriers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&barrier)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    barrier.barrier.wait();
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_barrier_close(barrier: BNDispatchHandle) -> BNDispatchStatus {
    registry()
        .barriers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&barrier)
        .map_or(BN_DISPATCH_INVALID_HANDLE, |_| BN_DISPATCH_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_create(
    initial: u32,
    out: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    if out.is_null() {
        return BN_DISPATCH_ERROR;
    }
    let handle = next_handle();
    registry()
        .semaphores
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            handle,
            Arc::new(SemaphoreHandle {
                permits: Mutex::new(initial),
                wake: Condvar::new(),
            }),
        );
    unsafe {
        *out = handle;
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_acquire(
    semaphore: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    let Some(semaphore) = registry()
        .semaphores
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&semaphore)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    let mut permits = semaphore
        .permits
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if timeout_ms < 0 {
        while *permits == 0 {
            permits = semaphore
                .wake
                .wait(permits)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    } else {
        let (next, timed) = semaphore
            .wake
            .wait_timeout_while(
                permits,
                Duration::from_millis(timeout_ms.cast_unsigned()),
                |count| *count == 0,
            )
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        permits = next;
        if timed.timed_out() && *permits == 0 {
            return BN_DISPATCH_TIMEOUT;
        }
    }
    *permits -= 1;
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_release(
    semaphore: BNDispatchHandle,
) -> BNDispatchStatus {
    let Some(semaphore) = registry()
        .semaphores
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&semaphore)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    let mut permits = semaphore
        .permits
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *permits = permits.saturating_add(1);
    semaphore.wake.notify_one();
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_close(semaphore: BNDispatchHandle) -> BNDispatchStatus {
    registry()
        .semaphores
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&semaphore)
        .map_or(BN_DISPATCH_INVALID_HANDLE, |_| BN_DISPATCH_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_create(out: *mut BNDispatchHandle) -> BNDispatchStatus {
    if out.is_null() {
        return BN_DISPATCH_ERROR;
    }
    let handle = next_handle();
    registry()
        .mutexes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            handle,
            Arc::new(MutexHandle {
                locked: Mutex::new(false),
                wake: Condvar::new(),
            }),
        );
    unsafe {
        *out = handle;
    }
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_lock(
    mutex: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    let Some(mutex) = registry()
        .mutexes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&mutex)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    let mut locked = mutex
        .locked
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if timeout_ms < 0 {
        while *locked {
            locked = mutex
                .wake
                .wait(locked)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    } else {
        let (next, timed) = mutex
            .wake
            .wait_timeout_while(
                locked,
                Duration::from_millis(timeout_ms.cast_unsigned()),
                |value| *value,
            )
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        locked = next;
        if timed.timed_out() && *locked {
            return BN_DISPATCH_TIMEOUT;
        }
    }
    *locked = true;
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_unlock(mutex: BNDispatchHandle) -> BNDispatchStatus {
    let Some(mutex) = registry()
        .mutexes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&mutex)
        .cloned()
    else {
        return BN_DISPATCH_INVALID_HANDLE;
    };
    let mut locked = mutex
        .locked
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !*locked {
        return BN_DISPATCH_ERROR;
    }
    *locked = false;
    mutex.wake.notify_one();
    BN_DISPATCH_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_close(mutex: BNDispatchHandle) -> BNDispatchStatus {
    registry()
        .mutexes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&mutex)
        .map_or(BN_DISPATCH_INVALID_HANDLE, |_| BN_DISPATCH_OK)
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
    fn synchronization_handles_have_functional_lifecycle() {
        let mut semaphore = 0;
        assert_eq!(
            bn_rt_dispatch_semaphore_create(1, &raw mut semaphore),
            BN_DISPATCH_OK
        );
        assert_eq!(
            bn_rt_dispatch_semaphore_acquire(semaphore, 0),
            BN_DISPATCH_OK
        );
        assert_eq!(
            bn_rt_dispatch_semaphore_acquire(semaphore, 1),
            BN_DISPATCH_TIMEOUT
        );
        assert_eq!(bn_rt_dispatch_semaphore_release(semaphore), BN_DISPATCH_OK);
        assert_eq!(
            bn_rt_dispatch_semaphore_acquire(semaphore, 1),
            BN_DISPATCH_OK
        );
        assert_eq!(bn_rt_dispatch_semaphore_close(semaphore), BN_DISPATCH_OK);

        let mut mutex = 0;
        assert_eq!(bn_rt_dispatch_mutex_create(&raw mut mutex), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_mutex_lock(mutex, 0), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_mutex_unlock(mutex), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_mutex_close(mutex), BN_DISPATCH_OK);
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
            BN_DISPATCH_ERROR
        );
        assert_eq!(error.code, BN_DISPATCH_CANCELLED);
        assert_eq!(bn_rt_dispatch_ticket_close(second), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_ticket_close(second), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_queue_close(queue, 1_000), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_ticket_close(first), BN_DISPATCH_OK);
    }
}
