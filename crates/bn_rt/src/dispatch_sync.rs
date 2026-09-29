// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::missing_errors_doc)] // Every failure is a `DispatchFailure` (dispatch_error.rs).

//! `BNDispatch` synchronization (`language/0.6/bndispatch.md`
//! "Synchronization"): the one implementation of `Group`, `Barrier`,
//! `Semaphore`, and `Mutex`, used by the interpreter provider and the C ABI.

use std::sync::{Condvar, Mutex};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use crate::dispatch_error::DispatchFailure;

/// The instant `timeout_ms` from now; the timeout must be within bounds.
///
/// # Errors
///
/// [`DispatchFailure::OutOfRange`] outside the timeout bounds.
pub fn deadline(timeout_ms: i128) -> Result<Instant, DispatchFailure> {
    let limits = bn_limits::dispatch_limits();
    let (min, max) = (limits.timeout_min_ms, limits.timeout_max_ms);
    if !(min..=max).contains(&timeout_ms) {
        return Err(DispatchFailure::OutOfRange {
            what: "timeout in milliseconds",
            value: timeout_ms,
            min,
            max,
        });
    }
    // Validated above: `timeout_ms` is positive and fits a `u64`.
    Ok(Instant::now() + Duration::from_millis(u64::try_from(timeout_ms).unwrap_or_default()))
}

/// `value` as a count within `1..=max`, or why it is not one.
fn count(what: &'static str, value: i128, max: usize) -> Result<usize, DispatchFailure> {
    usize::try_from(value)
        .ok()
        .filter(|count| (1..=max).contains(count))
        .ok_or(DispatchFailure::OutOfRange {
            what,
            value,
            min: 1,
            max: i128::try_from(max).unwrap_or(i128::MAX),
        })
}

/// `value` as a worker count for `Queue.Concurrent`.
///
/// # Errors
///
/// [`DispatchFailure::OutOfRange`] outside 1 through the worker maximum.
pub fn worker_count(value: i128) -> Result<usize, DispatchFailure> {
    count(
        "worker count",
        value,
        bn_limits::dispatch_limits().worker_count_max,
    )
}

/// The worker count of `Queue.Auto`: one per available processor.
///
/// # Errors
///
/// [`DispatchFailure::Unavailable`] when the system reports no count.
pub fn auto_worker_count() -> Result<usize, DispatchFailure> {
    std::thread::available_parallelism()
        .map(|count| {
            count
                .get()
                .min(bn_limits::dispatch_limits().worker_count_max)
        })
        .map_err(|_| DispatchFailure::Unavailable("the system reports no processor count"))
}

#[derive(Default)]
pub struct DispatchGroup {
    state: Mutex<usize>,
    wake: Condvar,
}
impl DispatchGroup {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn enter(&self) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
    }
    pub fn leave(&self) -> Result<(), DispatchFailure> {
        let mut count = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *count == 0 {
            return Err(DispatchFailure::GroupUnderflow);
        }
        *count -= 1;
        self.wake.notify_all();
        Ok(())
    }
    pub fn wait(&self, timeout_ms: i128) -> Result<(), DispatchFailure> {
        let deadline = deadline(timeout_ms)?;
        let mut count = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *count != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchFailure::Timeout { ms: timeout_ms });
            }
            (count, _) = self
                .wake
                .wait_timeout(count, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        Ok(())
    }
}

pub struct Barrier {
    parties: usize,
    state: Mutex<BarrierState>,
    wake: Condvar,
}

struct BarrierState {
    arrived: usize,
    generation: u64,
    broken_generation: Option<u64>,
}
impl Barrier {
    /// # Errors
    ///
    /// [`DispatchFailure::OutOfRange`] outside 1 through the worker maximum.
    pub fn new(parties: i128) -> Result<Self, DispatchFailure> {
        let parties = count(
            "number of barrier parties",
            parties,
            bn_limits::dispatch_limits().worker_count_max,
        )?;
        Ok(Self {
            parties,
            state: Mutex::new(BarrierState {
                arrived: 0,
                generation: 0,
                broken_generation: None,
            }),
            wake: Condvar::new(),
        })
    }
    pub fn wait(&self, timeout_ms: i128) -> Result<bool, DispatchFailure> {
        let deadline = deadline(timeout_ms)?;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = state.generation;
        state.arrived += 1;
        if state.arrived == self.parties {
            state.arrived = 0;
            state.generation += 1;
            self.wake.notify_all();
            return Ok(true);
        }
        while generation == state.generation {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.arrived = 0;
                state.broken_generation = Some(generation);
                state.generation += 1;
                self.wake.notify_all();
                return Err(DispatchFailure::Timeout { ms: timeout_ms });
            }
            (state, _) = self
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if state.broken_generation == Some(generation) {
            Err(DispatchFailure::Timeout { ms: timeout_ms })
        } else {
            Ok(false)
        }
    }
}

pub struct DispatchSemaphore {
    state: Mutex<SemaphoreState>,
    wake: Condvar,
}

struct SemaphoreState {
    initial: usize,
    available: usize,
}
impl DispatchSemaphore {
    /// # Errors
    ///
    /// [`DispatchFailure::OutOfRange`] outside 1 through the pending-ticket
    /// maximum.
    pub fn new(permits: i128) -> Result<Self, DispatchFailure> {
        let permits = count(
            "number of semaphore permits",
            permits,
            bn_limits::dispatch_limits().pending_tickets_max,
        )?;
        Ok(Self {
            state: Mutex::new(SemaphoreState {
                initial: permits,
                available: permits,
            }),
            wake: Condvar::new(),
        })
    }
    pub fn acquire(&self, timeout_ms: i128) -> Result<(), DispatchFailure> {
        let deadline = deadline(timeout_ms)?;
        let mut permits = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while permits.available == 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchFailure::Timeout { ms: timeout_ms });
            }
            (permits, _) = self
                .wake
                .wait_timeout(permits, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        permits.available -= 1;
        Ok(())
    }
    pub fn release(&self) -> Result<(), DispatchFailure> {
        let mut permits = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if permits.available == permits.initial {
            return Err(DispatchFailure::ReleaseWithEveryPermit {
                permits: permits.initial,
            });
        }
        permits.available += 1;
        self.wake.notify_one();
        Ok(())
    }
}

#[derive(Default)]
pub struct DispatchMutex {
    state: Mutex<Option<ThreadId>>,
    wake: Condvar,
}

impl DispatchMutex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
    pub fn lock(&self, timeout_ms: i128) -> Result<(), DispatchFailure> {
        let deadline = deadline(timeout_ms)?;
        let mut locked = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while locked.is_some() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchFailure::Timeout { ms: timeout_ms });
            }
            (locked, _) = self
                .wake
                .wait_timeout(locked, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *locked = Some(std::thread::current().id());
        Ok(())
    }
    pub fn unlock(&self) -> Result<(), DispatchFailure> {
        let mut locked = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *locked != Some(std::thread::current().id()) {
            return Err(DispatchFailure::NotOwner);
        }
        *locked = None;
        self.wake.notify_one();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch_error::DispatchFailure;

    #[test]
    fn barrier_timeout_releases_the_next_generation() {
        let barrier = Barrier::new(2).expect("valid barrier");
        assert_eq!(barrier.wait(1), Err(DispatchFailure::Timeout { ms: 1 }));
        assert_eq!(barrier.wait(1), Err(DispatchFailure::Timeout { ms: 1 }));
    }

    #[test]
    fn semaphore_and_mutex_honor_timeout_bounds() {
        let semaphore = DispatchSemaphore::new(1).expect("valid semaphore");
        semaphore.acquire(1).expect("first permit");
        assert_eq!(
            semaphore.acquire(1),
            Err(DispatchFailure::Timeout { ms: 1 })
        );
        semaphore.release().expect("release first permit");
        let mutex = DispatchMutex::new();
        mutex.lock(1).expect("first lock");
        assert_eq!(mutex.lock(1), Err(DispatchFailure::Timeout { ms: 1 }));
        mutex.unlock().expect("owner unlock");
    }

    #[test]
    fn barrier_timeout_breaks_generation_for_concurrent_waiters() {
        // Three parties, two waiters: the barrier cannot complete, so the only
        // way the long-waiting worker returns early is the broken generation.
        let barrier = std::sync::Arc::new(Barrier::new(3).expect("valid barrier"));
        let worker_barrier = std::sync::Arc::clone(&barrier);
        let worker = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            (worker_barrier.wait(5_000), started.elapsed())
        });
        // Wait on the barrier's own state, not on scheduler timing, until the
        // worker has arrived in the current generation.
        while barrier
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .arrived
            == 0
        {
            std::thread::yield_now();
        }
        assert_eq!(barrier.wait(50), Err(DispatchFailure::Timeout { ms: 50 }));
        let (outcome, elapsed) = worker.join().expect("barrier worker");
        assert_eq!(outcome, Err(DispatchFailure::Timeout { ms: 5_000 }));
        assert!(
            elapsed < Duration::from_secs(4),
            "worker must be released by the broken generation, not its own timeout: {elapsed:?}"
        );
    }

    #[test]
    fn semaphore_rejects_release_above_initial_capacity() {
        let semaphore = DispatchSemaphore::new(1).expect("valid semaphore");
        semaphore.acquire(1).expect("consume initial permit");
        semaphore.release().expect("first release");
        assert_eq!(
            semaphore.release(),
            Err(DispatchFailure::ReleaseWithEveryPermit { permits: 1 })
        );
        semaphore
            .acquire(1)
            .expect("original permit remains available");
    }

    #[test]
    fn mutex_rejects_unlock_by_a_different_thread() {
        let mutex = std::sync::Arc::new(DispatchMutex::new());
        mutex.lock(100).expect("owner lock");
        let foreign_mutex = std::sync::Arc::clone(&mutex);
        let foreign = std::thread::spawn(move || foreign_mutex.unlock());
        assert_eq!(
            foreign.join().expect("foreign unlock thread"),
            Err(DispatchFailure::NotOwner)
        );
        mutex.unlock().expect("owner unlock");
    }

    #[test]
    fn group_rejects_leave_without_a_matching_enter() {
        let group = DispatchGroup::new();
        assert_eq!(group.leave(), Err(DispatchFailure::GroupUnderflow));
        group.enter();
        group.leave().expect("matching leave");
    }
}
