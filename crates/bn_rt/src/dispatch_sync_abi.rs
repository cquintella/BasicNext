// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(unsafe_code)] // C ABI: handles and flags written through caller pointers.

//! C ABI of the `BNDispatch` synchronization classes. The semantics are
//! [`crate::dispatch_sync`], the implementation the interpreter uses; these
//! functions only convert arguments and handles, and record each failure
//! ([`DispatchFailure`]) for the emitted code (`bn_rt_error_take`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::dispatch_abi::{
    BN_DISPATCH_ERROR, BN_DISPATCH_INVALID_HANDLE, BN_DISPATCH_OK, BN_DISPATCH_POLICY_DENIED,
    BN_DISPATCH_TIMEOUT, BNDispatchHandle, BNDispatchStatus, next_handle,
};
use crate::dispatch_error::DispatchFailure;
use crate::dispatch_sync::{Barrier, DispatchGroup, DispatchMutex, DispatchSemaphore};

type Table<T> = Mutex<HashMap<BNDispatchHandle, Arc<T>>>;

fn table<T>(cell: &'static OnceLock<Table<T>>) -> &'static Table<T> {
    cell.get_or_init(|| Mutex::new(HashMap::new()))
}

static GROUPS: OnceLock<Table<DispatchGroup>> = OnceLock::new();
static BARRIERS: OnceLock<Table<Barrier>> = OnceLock::new();
static SEMAPHORES: OnceLock<Table<DispatchSemaphore>> = OnceLock::new();
static MUTEXES: OnceLock<Table<DispatchMutex>> = OnceLock::new();

/// Records `failure` of `operation` and returns its status.
fn failed(operation: &str, failure: &DispatchFailure) -> BNDispatchStatus {
    crate::set_error_report(
        failure.code(),
        operation,
        failure.message(),
        failure.cause(),
    );
    if matches!(failure, DispatchFailure::Timeout { .. }) {
        BN_DISPATCH_TIMEOUT
    } else {
        BN_DISPATCH_ERROR
    }
}

/// The status of a VOID member.
fn status(operation: &str, outcome: Result<(), DispatchFailure>) -> BNDispatchStatus {
    outcome.map_or_else(|failure| failed(operation, &failure), |()| BN_DISPATCH_OK)
}

/// Files `made` (or records why it failed) and writes its handle to `out`.
fn create<T>(
    cell: &'static OnceLock<Table<T>>,
    operation: &str,
    made: Result<T, DispatchFailure>,
    out: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    if out.is_null() {
        return BN_DISPATCH_ERROR;
    }
    let value = match made {
        Ok(value) => value,
        Err(failure) => return failed(operation, &failure),
    };
    let handle = next_handle();
    table(cell)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, Arc::new(value));
    // SAFETY: `out` is non-null and points at the caller's handle slot.
    unsafe { *out = handle };
    BN_DISPATCH_OK
}

/// Runs `operation` on the object behind `handle`, outside the table lock
/// (waits must not block other handles).
fn with<T>(
    cell: &'static OnceLock<Table<T>>,
    handle: BNDispatchHandle,
    operation: impl FnOnce(&T) -> BNDispatchStatus,
) -> BNDispatchStatus {
    if !crate::policy::allows(crate::policy::POLICY_DISPATCH) {
        return BN_DISPATCH_POLICY_DENIED;
    }
    let object = table(cell)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&handle)
        .cloned();
    object.map_or(BN_DISPATCH_INVALID_HANDLE, |object| operation(&object))
}

/// Releases a handle (`RELEASE` of the BN object).
fn close<T>(cell: &'static OnceLock<Table<T>>, handle: BNDispatchHandle) -> BNDispatchStatus {
    table(cell)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&handle)
        .map_or(BN_DISPATCH_INVALID_HANDLE, |_| BN_DISPATCH_OK)
}

/// `Group.New()`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_create(out: *mut BNDispatchHandle) -> BNDispatchStatus {
    create(
        &GROUPS,
        "BNDispatch.Group.New",
        Ok(DispatchGroup::new()),
        out,
    )
}

/// `group.Enter()`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_enter(group: BNDispatchHandle) -> BNDispatchStatus {
    with(&GROUPS, group, |group| {
        group.enter();
        BN_DISPATCH_OK
    })
}

/// `group.Leave()`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_leave(group: BNDispatchHandle) -> BNDispatchStatus {
    with(&GROUPS, group, |group| {
        status("BNDispatch.Group.Leave", group.leave())
    })
}

/// `group.Wait(timeoutMs)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_wait(
    group: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    with(&GROUPS, group, |group| {
        status("BNDispatch.Group.Wait", group.wait(i128::from(timeout_ms)))
    })
}

/// Releases a group handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_group_close(group: BNDispatchHandle) -> BNDispatchStatus {
    close(&GROUPS, group)
}

/// `Barrier.New(parties)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_barrier_create(
    parties: i64,
    out: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    create(
        &BARRIERS,
        "BNDispatch.Barrier.New",
        Barrier::new(i128::from(parties)),
        out,
    )
}

/// `barrier.Wait(timeoutMs)`: `out_last` receives 1 for the last caller to
/// arrive, 0 for the others.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_barrier_wait(
    barrier: BNDispatchHandle,
    timeout_ms: i64,
    out_last: *mut i32,
) -> BNDispatchStatus {
    if out_last.is_null() {
        return BN_DISPATCH_ERROR;
    }
    with(&BARRIERS, barrier, |barrier| {
        match barrier.wait(i128::from(timeout_ms)) {
            Ok(last) => {
                // SAFETY: `out_last` is non-null and points at the caller's flag.
                unsafe { *out_last = i32::from(last) };
                BN_DISPATCH_OK
            }
            Err(failure) => failed("BNDispatch.Barrier.Wait", &failure),
        }
    })
}

/// Releases a barrier handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_barrier_close(barrier: BNDispatchHandle) -> BNDispatchStatus {
    close(&BARRIERS, barrier)
}

/// `Semaphore.New(permits)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_create(
    permits: i64,
    out: *mut BNDispatchHandle,
) -> BNDispatchStatus {
    create(
        &SEMAPHORES,
        "BNDispatch.Semaphore.New",
        DispatchSemaphore::new(i128::from(permits)),
        out,
    )
}

/// `semaphore.Acquire(timeoutMs)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_acquire(
    semaphore: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    with(&SEMAPHORES, semaphore, |semaphore| {
        status(
            "BNDispatch.Semaphore.Acquire",
            semaphore.acquire(i128::from(timeout_ms)),
        )
    })
}

/// `semaphore.Release()`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_release(
    semaphore: BNDispatchHandle,
) -> BNDispatchStatus {
    with(&SEMAPHORES, semaphore, |semaphore| {
        status("BNDispatch.Semaphore.Release", semaphore.release())
    })
}

/// Releases a semaphore handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_semaphore_close(semaphore: BNDispatchHandle) -> BNDispatchStatus {
    close(&SEMAPHORES, semaphore)
}

/// `Mutex.New()`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_create(out: *mut BNDispatchHandle) -> BNDispatchStatus {
    create(
        &MUTEXES,
        "BNDispatch.Mutex.New",
        Ok(DispatchMutex::new()),
        out,
    )
}

/// `mutex.Lock(timeoutMs)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_lock(
    mutex: BNDispatchHandle,
    timeout_ms: i64,
) -> BNDispatchStatus {
    with(&MUTEXES, mutex, |mutex| {
        status("BNDispatch.Mutex.Lock", mutex.lock(i128::from(timeout_ms)))
    })
}

/// `mutex.Unlock()`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_unlock(mutex: BNDispatchHandle) -> BNDispatchStatus {
    with(&MUTEXES, mutex, |mutex| {
        status("BNDispatch.Mutex.Unlock", mutex.unlock())
    })
}

/// Releases a mutex handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dispatch_mutex_close(mutex: BNDispatchHandle) -> BNDispatchStatus {
    close(&MUTEXES, mutex)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ABI follows `dispatch_sync` and records the failure it reports.
    #[test]
    fn synchronization_handles_follow_the_core_and_record_failures() {
        let mut semaphore = 0;
        assert_eq!(
            bn_rt_dispatch_semaphore_create(0, &raw mut semaphore),
            BN_DISPATCH_ERROR
        );
        assert_eq!(
            bn_rt_dispatch_semaphore_create(1, &raw mut semaphore),
            BN_DISPATCH_OK
        );
        assert_eq!(
            bn_rt_dispatch_semaphore_release(semaphore),
            BN_DISPATCH_ERROR
        );
        let record = crate::error_abi::bn_rt_error_take(1, std::ptr::null());
        assert_eq!(
            crate::error_abi::bn_rt_error_code(record),
            i64::from(bn_types::error_codes::dispatch::INVALID_STATE)
        );
        assert_eq!(
            bn_rt_dispatch_semaphore_acquire(semaphore, 1),
            BN_DISPATCH_OK
        );
        assert_eq!(
            bn_rt_dispatch_semaphore_acquire(semaphore, 1),
            BN_DISPATCH_TIMEOUT
        );
        assert_eq!(bn_rt_dispatch_semaphore_release(semaphore), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_semaphore_close(semaphore), BN_DISPATCH_OK);

        let mut group = 0;
        assert_eq!(bn_rt_dispatch_group_create(&raw mut group), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_group_leave(group), BN_DISPATCH_ERROR);
        assert_eq!(bn_rt_dispatch_group_enter(group), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_group_leave(group), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_group_wait(group, 1), BN_DISPATCH_OK);

        let mut barrier = 0;
        let mut last = -1;
        assert_eq!(
            bn_rt_dispatch_barrier_create(1, &raw mut barrier),
            BN_DISPATCH_OK
        );
        assert_eq!(
            bn_rt_dispatch_barrier_wait(barrier, 10, &raw mut last),
            BN_DISPATCH_OK
        );
        assert_eq!(last, 1);

        let mut mutex = 0;
        assert_eq!(bn_rt_dispatch_mutex_create(&raw mut mutex), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_mutex_unlock(mutex), BN_DISPATCH_ERROR);
        assert_eq!(bn_rt_dispatch_mutex_lock(mutex, 1), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_mutex_unlock(mutex), BN_DISPATCH_OK);
        assert_eq!(bn_rt_dispatch_mutex_close(mutex), BN_DISPATCH_OK);
    }
}
