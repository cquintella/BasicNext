use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

fn deadline(timeout_ms: i128) -> Result<Instant, super::DispatchError> {
    if !(super::MIN_TIMEOUT_MS..=super::MAX_TIMEOUT_MS).contains(&timeout_ms) {
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
        *self.state.lock().expect("group mutex poisoned") += 1;
    }
    pub(crate) fn leave(&self) {
        let mut count = self.state.lock().expect("group mutex poisoned");
        if *count > 0 {
            *count -= 1;
        }
        self.wake.notify_all();
    }
    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<(), super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut count = self.state.lock().expect("group mutex poisoned");
        while *count != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DispatchError::Timeout);
            }
            (count, _) = self
                .wake
                .wait_timeout(count, remaining)
                .expect("group mutex poisoned");
        }
        Ok(())
    }
}

pub(crate) struct Barrier {
    parties: usize,
    state: Mutex<(usize, u64)>,
    wake: Condvar,
}
impl Barrier {
    pub(crate) fn new(parties: i128) -> Option<Self> {
        let parties = usize::try_from(parties).ok()?;
        (1..=super::MAX_WORKERS).contains(&parties).then_some(Self {
            parties,
            state: Mutex::new((0, 0)),
            wake: Condvar::new(),
        })
    }
    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<bool, super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut state = self.state.lock().expect("barrier mutex poisoned");
        let generation = state.1;
        state.0 += 1;
        if state.0 == self.parties {
            state.0 = 0;
            state.1 += 1;
            self.wake.notify_all();
            return Ok(true);
        }
        while generation == state.1 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                state.0 = 0;
                state.1 += 1;
                self.wake.notify_all();
                return Err(super::DispatchError::Timeout);
            }
            (state, _) = self
                .wake
                .wait_timeout(state, remaining)
                .expect("barrier mutex poisoned");
        }
        Ok(false)
    }
}

pub(crate) struct DispatchSemaphore {
    state: Mutex<usize>,
    wake: Condvar,
}
impl DispatchSemaphore {
    pub(crate) fn new(permits: i128) -> Option<Self> {
        let permits = usize::try_from(permits).ok()?;
        (1..=super::MAX_PENDING).contains(&permits).then_some(Self {
            state: Mutex::new(permits),
            wake: Condvar::new(),
        })
    }
    pub(crate) fn acquire(&self, timeout_ms: i128) -> Result<(), super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut permits = self.state.lock().expect("semaphore mutex poisoned");
        while *permits == 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DispatchError::Timeout);
            }
            (permits, _) = self
                .wake
                .wait_timeout(permits, remaining)
                .expect("semaphore mutex poisoned");
        }
        *permits -= 1;
        Ok(())
    }
    pub(crate) fn release(&self) {
        *self.state.lock().expect("semaphore mutex poisoned") += 1;
        self.wake.notify_one();
    }
}

pub(crate) struct DispatchMutex {
    state: Mutex<bool>,
    wake: Condvar,
}

impl DispatchMutex {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(false),
            wake: Condvar::new(),
        }
    }
    pub(crate) fn lock(&self, timeout_ms: i128) -> Result<(), super::DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut locked = self.state.lock().expect("mutex state poisoned");
        while *locked {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(super::DispatchError::Timeout);
            }
            (locked, _) = self
                .wake
                .wait_timeout(locked, remaining)
                .expect("mutex state poisoned");
        }
        *locked = true;
        Ok(())
    }
    pub(crate) fn unlock(&self) {
        let mut locked = self.state.lock().expect("mutex state poisoned");
        *locked = false;
        self.wake.notify_one();
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
        semaphore.release();
        let mutex = DispatchMutex::new();
        mutex.lock(1).expect("first lock");
        assert_eq!(mutex.lock(1), Err(DispatchError::Timeout));
        mutex.unlock();
    }
}
