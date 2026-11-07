// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The workers behind `HOST.Net.Resolve` and `HOST.Net.Reverse`.
//! `getaddrinfo` cannot be interrupted, so a timeout is a logical
//! cancellation: the caller stops waiting, a lookup that has not started is
//! never started, and one that has started finishes and is dropped.
//!
//! The number of threads is fixed ([`LOOKUP_WORKERS`]), the number of lookups
//! in flight (queued or running) has a ceiling ([`LOOKUPS_IN_FLIGHT_MAX`]),
//! and a lookup past the ceiling fails at once with [`Refusal`]. No thread is
//! created per lookup, and no lock is held while waiting for one.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, OnceLock, PoisonError};
use std::time::Duration;

/// Threads that run name lookups, for the whole process.
pub const LOOKUP_WORKERS: usize = 4;
/// Lookups queued or running at once; a further one is refused.
pub const LOOKUPS_IN_FLIGHT_MAX: usize = 32;

/// Why a lookup was not answered, other than by running out of time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// The in-flight ceiling is reached: the lookup was not queued.
    Full { in_flight_max: usize },
    /// The lookup started and then died (it panicked) without an answer.
    Lost,
    /// The lookup threads could not start (the operating system refused).
    Unavailable,
}

/// A place in the pool, given back when the lookup that holds it ends, also
/// by a panic.
struct Place(Arc<Shared>);

impl Drop for Place {
    fn drop(&mut self) {
        self.0.in_flight.fetch_sub(1, Ordering::AcqRel);
    }
}

const QUEUED: u8 = 0;
const RUNNING: u8 = 1;
const CANCELLED: u8 = 2;

type Job = Box<dyn FnOnce() + Send>;

struct Shared {
    queue: Mutex<VecDeque<Job>>,
    wake: Condvar,
    in_flight: AtomicUsize,
    in_flight_max: usize,
}

/// A fixed set of worker threads and a bounded queue.
pub struct LookupPool {
    shared: Arc<Shared>,
}

impl LookupPool {
    /// Starts `workers` threads. They live as long as the process.
    ///
    /// # Errors
    ///
    /// The operating system's error when a thread cannot start; threads
    /// already started stay idle on an empty queue.
    pub fn new(workers: usize, in_flight_max: usize) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            in_flight: AtomicUsize::new(0),
            in_flight_max,
        });
        for index in 0..workers {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name(format!("bn-lookup-{index}"))
                .spawn(move || work(&shared))?;
        }
        Ok(Self { shared })
    }

    /// The pool every `HOST.Net` lookup of the process shares, started at the
    /// first lookup. A start that fails is not kept: the next lookup tries again.
    ///
    /// # Errors
    ///
    /// [`Refusal::Unavailable`] when the lookup threads cannot start.
    pub fn global() -> Result<&'static Self, Refusal> {
        static POOL: OnceLock<LookupPool> = OnceLock::new();
        static STARTING: Mutex<()> = Mutex::new(());
        if let Some(pool) = POOL.get() {
            return Ok(pool);
        }
        let _starting = STARTING.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(pool) = POOL.get() {
            return Ok(pool);
        }
        let pool =
            Self::new(LOOKUP_WORKERS, LOOKUPS_IN_FLIGHT_MAX).map_err(|_| Refusal::Unavailable)?;
        Ok(POOL.get_or_init(|| pool))
    }

    /// Runs `lookup` on a worker and waits at most `timeout` for its answer;
    /// `Ok(None)` is the timeout.
    ///
    /// # Errors
    ///
    /// [`Refusal::Full`] when the in-flight ceiling is reached, [`Refusal::Lost`]
    /// when the lookup died after it started.
    pub fn run<T, F>(&self, timeout: Duration, lookup: F) -> Result<Option<T>, Refusal>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        let shared = &self.shared;
        // Reserve a place first; the check and the increment are one step.
        let reserved =
            shared
                .in_flight
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                    (count < shared.in_flight_max).then_some(count + 1)
                });
        if reserved.is_err() {
            return Err(Refusal::Full {
                in_flight_max: shared.in_flight_max,
            });
        }
        let state = Arc::new(AtomicU8::new(QUEUED));
        let (sender, receiver) = mpsc::sync_channel(1);
        let job_state = Arc::clone(&state);
        let job_shared = Arc::clone(shared);
        let job: Job = Box::new(move || {
            // The place is held from the queue until a worker takes the job,
            // so the queue never outgrows the ceiling; it is given back here,
            // whether the lookup runs, was cancelled, or panics.
            let _place = Place(job_shared);
            if job_state
                .compare_exchange(QUEUED, RUNNING, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                // The caller gave up before a worker took it: never started.
                return;
            }
            let _ = sender.send(lookup());
        });
        shared
            .queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(job);
        shared.wake.notify_one();
        match receiver.recv_timeout(timeout) {
            Ok(answer) => Ok(Some(answer)),
            Err(RecvTimeoutError::Timeout) => {
                // Not started: cancel it, so the worker that takes it drops it
                // unrun. Either way the worker gives the place back.
                let _ =
                    state.compare_exchange(QUEUED, CANCELLED, Ordering::AcqRel, Ordering::Acquire);
                Ok(None)
            }
            Err(RecvTimeoutError::Disconnected) => {
                if state.load(Ordering::Acquire) == RUNNING {
                    Err(Refusal::Lost)
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Lookups queued or running now.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.shared.in_flight.load(Ordering::Acquire)
    }
}

impl std::fmt::Debug for LookupPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LookupPool")
            .field("in_flight", &self.in_flight())
            .finish_non_exhaustive()
    }
}

fn work(shared: &Shared) -> ! {
    loop {
        let job = {
            let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                if let Some(job) = queue.pop_front() {
                    break job;
                }
                queue = shared
                    .wake
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        // A panic in a lookup must not take the worker, or its place, away.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::atomic::AtomicBool;
    use std::thread::{self, ThreadId};
    use std::time::Instant;

    const WORKERS: usize = 2;
    const IN_FLIGHT_MAX: usize = 8;
    const CALLS: usize = 1000;
    const SHORT: Duration = Duration::from_millis(1);

    /// A resolver that does not answer until the test lets it.
    struct Silence {
        open: AtomicBool,
        started: Mutex<HashSet<ThreadId>>,
        runs: AtomicUsize,
    }

    impl Silence {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                open: AtomicBool::new(false),
                started: Mutex::new(HashSet::new()),
                runs: AtomicUsize::new(0),
            })
        }

        fn lookup(self: &Arc<Self>) -> impl FnOnce() -> u32 + Send + use<> {
            let silence = Arc::clone(self);
            move || {
                silence.runs.fetch_add(1, Ordering::SeqCst);
                silence
                    .started
                    .lock()
                    .unwrap()
                    .insert(thread::current().id());
                while !silence.open.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_millis(1));
                }
                7
            }
        }
    }

    #[test]
    fn many_timeouts_use_the_fixed_threads_and_leave_no_lookup_behind() {
        let pool = LookupPool::new(WORKERS, IN_FLIGHT_MAX).expect("start the pool");
        let silence = Silence::new();
        let mut refused = 0;
        for _ in 0..CALLS {
            match pool.run(SHORT, silence.lookup()) {
                Ok(None) => {}
                Err(Refusal::Full { .. }) => refused += 1,
                outcome => panic!("the resolver does not answer: {outcome:?}"),
            }
            // A cancelled lookup still in the queue holds its place, so the
            // queue never outgrows the ceiling while the workers are stalled.
            let queued = pool.shared.queue.lock().unwrap().len();
            assert!(
                queued <= IN_FLIGHT_MAX && pool.in_flight() <= IN_FLIGHT_MAX,
                "{queued} lookups queued, {} in flight, past the ceiling {IN_FLIGHT_MAX}",
                pool.in_flight()
            );
        }
        assert!(refused > 0, "a stalled resolver must reach the ceiling");
        // Release the silent lookups and let the queue drain.
        silence.open.store(true, Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(5);
        while pool.in_flight() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(pool.in_flight(), 0, "places were not given back");
        // Only lookups that had started ran: at most one per worker was
        // running when its caller gave up; the queued ones were never started.
        assert!(
            silence.started.lock().unwrap().len() <= WORKERS,
            "more threads than workers ran a lookup"
        );
        assert!(
            silence.runs.load(Ordering::SeqCst) <= WORKERS,
            "{} lookups started; cancelled ones must not",
            silence.runs.load(Ordering::SeqCst)
        );
    }

    #[test]
    fn the_ceiling_refuses_lookups_past_the_places_in_flight() {
        const CALLERS: usize = IN_FLIGHT_MAX + 4;
        const PATIENCE: Duration = Duration::from_secs(10);
        let pool = Arc::new(LookupPool::new(WORKERS, IN_FLIGHT_MAX).expect("start the pool"));
        let silence = Silence::new();
        let callers: Vec<_> = (0..CALLERS)
            .map(|_| {
                let pool = Arc::clone(&pool);
                let lookup = silence.lookup();
                thread::spawn(move || pool.run(PATIENCE, lookup))
            })
            .collect();
        // The refused callers return at once; the admitted ones wait.
        let deadline = Instant::now() + Duration::from_secs(5);
        while pool.in_flight() < IN_FLIGHT_MAX && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(pool.in_flight(), IN_FLIGHT_MAX);
        silence.open.store(true, Ordering::Release);
        let outcomes: Vec<_> = callers.into_iter().map(|c| c.join().unwrap()).collect();
        let refused = outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(Refusal::Full { in_flight_max }) if *in_flight_max == IN_FLIGHT_MAX))
            .count();
        assert_eq!(refused, CALLERS - IN_FLIGHT_MAX);
    }

    #[test]
    fn an_answer_in_time_is_returned_and_gives_its_place_back() {
        let pool = LookupPool::new(WORKERS, IN_FLIGHT_MAX).expect("start the pool");
        assert_eq!(pool.run(Duration::from_secs(5), || 42), Ok(Some(42)));
        let deadline = Instant::now() + Duration::from_secs(5);
        while pool.in_flight() > 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(pool.in_flight(), 0);
    }

    #[test]
    fn a_panicking_lookup_neither_kills_the_worker_nor_keeps_its_place() {
        let pool = LookupPool::new(1, IN_FLIGHT_MAX).expect("start the pool");
        let outcome = pool.run(Duration::from_millis(200), || -> u32 {
            panic!("a failing resolver");
        });
        assert_eq!(outcome, Err(Refusal::Lost));
        assert_eq!(pool.run(Duration::from_secs(5), || 1), Ok(Some(1)));
        assert_eq!(pool.in_flight(), 0);
    }
}
