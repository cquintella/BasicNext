use std::sync::{Condvar, Mutex};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

fn deadline(timeout_ms: i128) -> Result<Instant, super::DispatchError> {
    let limits = crate::config::dispatch_limits();
    if !(limits.timeout_min_ms..=limits.timeout_max_ms).contains(&timeout_ms) {
        return Err(super::DispatchError::InvalidTimeout);
    }
    Ok(Instant::now()
        + Duration::from_millis(u64::try_from(timeout_ms).expect("validated timeout")))
}

pub(crate) struct DispatchGroup {
    state: Mutex<usize>,
    wake: Condvar,
}
impl DispatchGroup {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(0),
            wake: Condvar::new(),
        }
    }
    pub(crate) fn enter(&self) {
        *self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
    }
    pub(crate) fn leave(&self) -> Result<(), super::DispatchError> {
        let mut count = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *count == 0 {
            return Err(super::DispatchError::GroupUnderflow);
        }
        *count -= 1;
        self.wake.notify_all();
        Ok(())
    }
    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<(), super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut count = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while *count != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DispatchError::Timeout);
            }
            (count, _) = self
                .wake
                .wait_timeout(count, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        Ok(())
    }
}

pub(crate) struct Barrier {
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
    pub(crate) fn new(parties: i128) -> Option<Self> {
        let parties = usize::try_from(parties).ok()?;
        (1..=crate::config::dispatch_limits().worker_count_max)
            .contains(&parties)
            .then_some(Self {
                parties,
                state: Mutex::new(BarrierState {
                    arrived: 0,
                    generation: 0,
                    broken_generation: None,
                }),
                wake: Condvar::new(),
            })
    }
    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<bool, super::DispatchError> {
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
                return Err(super::DispatchError::Timeout);
            }
            (state, _) = self
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if state.broken_generation == Some(generation) {
            Err(super::DispatchError::Timeout)
        } else {
            Ok(false)
        }
    }
}

pub(crate) struct DispatchSemaphore {
    state: Mutex<SemaphoreState>,
    wake: Condvar,
}

struct SemaphoreState {
    initial: usize,
    available: usize,
}
impl DispatchSemaphore {
    pub(crate) fn new(permits: i128) -> Option<Self> {
        let permits = usize::try_from(permits).ok()?;
        (1..=crate::config::dispatch_limits().pending_tickets_max)
            .contains(&permits)
            .then_some(Self {
                state: Mutex::new(SemaphoreState {
                    initial: permits,
                    available: permits,
                }),
                wake: Condvar::new(),
            })
    }
    pub(crate) fn acquire(&self, timeout_ms: i128) -> Result<(), super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut permits = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while permits.available == 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DispatchError::Timeout);
            }
            (permits, _) = self
                .wake
                .wait_timeout(permits, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        permits.available -= 1;
        Ok(())
    }
    pub(crate) fn release(&self) -> Result<(), super::DispatchError> {
        let mut permits = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if permits.available == permits.initial {
            return Err(super::DispatchError::InvalidRelease);
        }
        permits.available += 1;
        self.wake.notify_one();
        Ok(())
    }
}

pub(crate) struct DispatchMutex {
    state: Mutex<Option<ThreadId>>,
    wake: Condvar,
}

impl DispatchMutex {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(None),
            wake: Condvar::new(),
        }
    }
    pub(crate) fn lock(&self, timeout_ms: i128) -> Result<(), super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut locked = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while locked.is_some() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DispatchError::Timeout);
            }
            (locked, _) = self
                .wake
                .wait_timeout(locked, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        *locked = Some(std::thread::current().id());
        Ok(())
    }
    pub(crate) fn unlock(&self) -> Result<(), super::DispatchError> {
        let mut locked = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *locked != Some(std::thread::current().id()) {
            return Err(super::DispatchError::NotOwner);
        }
        *locked = None;
        self.wake.notify_one();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::DispatchError;

    #[test]
    fn barrier_timeout_releases_the_next_generation() {
        let barrier = Barrier::new(2).expect("valid barrier");
        assert_eq!(barrier.wait(1), Err(DispatchError::Timeout));
        assert_eq!(barrier.wait(1), Err(DispatchError::Timeout));
    }

    #[test]
    fn semaphore_and_mutex_honor_timeout_bounds() {
        let semaphore = DispatchSemaphore::new(1).expect("valid semaphore");
        semaphore.acquire(1).expect("first permit");
        assert_eq!(semaphore.acquire(1), Err(DispatchError::Timeout));
        semaphore.release().expect("release first permit");
        let mutex = DispatchMutex::new();
        mutex.lock(1).expect("first lock");
        assert_eq!(mutex.lock(1), Err(DispatchError::Timeout));
        mutex.unlock().expect("owner unlock");
    }

    #[test]
    fn barrier_timeout_breaks_generation_for_concurrent_waiters() {
        let barrier = std::sync::Arc::new(Barrier::new(2).expect("valid barrier"));
        let ready = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_barrier = std::sync::Arc::clone(&barrier);
        let worker_ready = std::sync::Arc::clone(&ready);
        let worker = std::thread::spawn(move || {
            worker_ready.store(true, std::sync::atomic::Ordering::Release);
            worker_barrier.wait(5)
        });
        while !ready.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::yield_now();
        }
        std::thread::sleep(Duration::from_millis(10));
        assert_eq!(barrier.wait(100), Err(DispatchError::Timeout));
        assert_eq!(
            worker.join().expect("barrier worker").unwrap_err(),
            DispatchError::Timeout
        );
    }

    #[test]
    fn semaphore_rejects_release_above_initial_capacity() {
        let semaphore = DispatchSemaphore::new(1).expect("valid semaphore");
        semaphore.acquire(1).expect("consume initial permit");
        semaphore.release().expect("first release");
        assert_eq!(semaphore.release(), Err(DispatchError::InvalidRelease));
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
            Err(DispatchError::NotOwner)
        );
        mutex.unlock().expect("owner unlock");
    }

    #[test]
    fn group_rejects_leave_without_a_matching_enter() {
        let group = DispatchGroup::new();
        assert_eq!(group.leave(), Err(DispatchError::GroupUnderflow));
        group.enter();
        group.leave().expect("matching leave");
    }
}
