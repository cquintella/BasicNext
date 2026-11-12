// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Server.Stop` / `Server.Close`: stop accepting, join the listener and
//! drain the connections within the timeout, without holding the state lock
//! while waiting (moved from `bn_lib_web`; both backends use it).

use super::ServerState;
use super::server::StopPending;
use crate::web_error::WebFailure;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The longest a drain sleeps without a signal. A thread that has returned
/// from its closure is not yet `is_finished`, so a notice sent from inside
/// the closure can arrive just before the state it announces; the drain
/// looks again at this interval, which is far from a busy wait.
const DRAIN_RECHECK: Duration = Duration::from_millis(10);

/// What a drain waits on: whatever it is waiting for (a connection, a handler
/// task, a worker or the listener ending) calls [`DrainSignal::notify`]. The
/// generation counter means a notice sent between the drain's check and its
/// wait is not lost.
#[derive(Debug, Default)]
pub struct DrainSignal {
    generation: Mutex<u64>,
    changed: Condvar,
}

impl DrainSignal {
    /// Announces that something a drain may wait for has changed.
    pub fn notify(&self) {
        let mut generation = self
            .generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *generation = generation.wrapping_add(1);
        self.changed.notify_all();
    }

    /// A guard that announces the end of the thread that holds it.
    #[must_use]
    pub fn notify_on_drop(self: &Arc<Self>) -> DrainNotice {
        DrainNotice(Arc::clone(self))
    }

    fn generation(&self) -> u64 {
        *self
            .generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Sleeps until a notice after `seen`, the recheck interval, or `deadline`.
    fn wait_for_change(&self, seen: u64, deadline: Instant) {
        let limit = deadline
            .saturating_duration_since(Instant::now())
            .min(DRAIN_RECHECK);
        let generation = self
            .generation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if *generation == seen && !limit.is_zero() {
            drop(
                self.changed
                    .wait_timeout(generation, limit)
                    .unwrap_or_else(PoisonError::into_inner),
            );
        }
    }
}

/// The handler tasks still running, counted so `Stop` can wait for them; each
/// end announces itself to the drain.
#[derive(Debug)]
pub struct HandlerTasks {
    running: std::sync::atomic::AtomicUsize,
    signal: Arc<DrainSignal>,
}

impl HandlerTasks {
    #[must_use]
    pub fn new(signal: Arc<DrainSignal>) -> Self {
        Self {
            running: std::sync::atomic::AtomicUsize::new(0),
            signal,
        }
    }

    /// A task starts.
    pub fn begin(&self) {
        self.running
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }

    /// A task ends, or is dropped unrun.
    pub fn end(&self) {
        self.running
            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        self.signal.notify();
    }

    #[must_use]
    pub fn running(&self) -> usize {
        self.running.load(std::sync::atomic::Ordering::Acquire)
    }
}

/// Calls [`DrainSignal::notify`] when dropped.
#[derive(Debug)]
pub struct DrainNotice(Arc<DrainSignal>);

impl Drop for DrainNotice {
    fn drop(&mut self) {
        self.0.notify();
    }
}

/// Stops `state` and waits at most `timeout_ms` for its listener and
/// connections; `close` also releases the server.
///
/// # Errors
///
/// A timeout outside `1..60000` ms, a server that cannot stop, a listener
/// that failed or did not join in time, or connections still active at the
/// deadline.
pub fn drain_server(
    state: &std::sync::Arc<std::sync::Mutex<ServerState>>,
    timeout_ms: i128,
    close: bool,
) -> Result<(), WebFailure> {
    let timeout = std::time::Duration::from_millis(
        u64::try_from(timeout_ms).map_err(|_| super::server::stop_timeout_outside_bounds())?,
    );
    let deadline = std::time::Instant::now() + timeout;
    let (mut listener, signal) = {
        let mut server = state
            .lock()
            .map_err(|_| WebFailure::ServerStateUnavailable)?;
        server.begin_stop(timeout_ms)?;
        (server.take_listener(), server.drain_signal())
    };
    while let Some(handle) = listener {
        let seen = signal.generation();
        if handle.is_finished() {
            if handle.join().is_err() {
                if let Ok(mut server) = state.lock() {
                    server.mark_failed();
                }
                return Err(WebFailure::ListenerJoinFailed);
            }
            break;
        }
        if std::time::Instant::now() >= deadline {
            let mut server = state
                .lock()
                .map_err(|_| WebFailure::ServerStateUnavailable)?;
            server.install_listener(handle).map_err(|_| {
                WebFailure::InvalidArgument("server listener is already installed".into())
            })?;
            return Err(WebFailure::ListenerJoinTimeout);
        }
        signal.wait_for_change(seen, deadline);
        listener = Some(handle);
    }
    loop {
        let seen = signal.generation();
        let result = {
            let mut server = state
                .lock()
                .map_err(|_| WebFailure::ServerStateUnavailable)?;
            server.finish_stop()
        };
        match result {
            Ok(()) => {
                let workers_finished = state
                    .lock()
                    .map_err(|_| WebFailure::ServerStateUnavailable)?
                    .workers_finished();
                if !workers_finished {
                    if std::time::Instant::now() >= deadline {
                        return Err(WebFailure::WorkerDrainTimeout);
                    }
                    signal.wait_for_change(seen, deadline);
                    continue;
                }
                if close {
                    state
                        .lock()
                        .map_err(|_| WebFailure::ServerStateUnavailable)?
                        .mark_closed();
                }
                return Ok(());
            }
            Err(StopPending::Connections) => {
                if std::time::Instant::now() >= deadline {
                    return Err(StopPending::Connections.failure());
                }
            }
            Err(pending) => return Err(pending.failure()),
        }
        signal.wait_for_change(seen, deadline);
    }
}

#[cfg(test)]
mod tests {
    use super::drain_server;
    use crate::web::ServerState;
    use crate::web_error::WebFailure;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    #[test]
    fn drain_server_times_out_without_holding_the_state_lock() {
        let state = Arc::new(Mutex::new(ServerState::new()));
        let release = Arc::new(AtomicBool::new(false));
        {
            let mut server = state.lock().unwrap();
            server.start().unwrap();
            server.admit_connection().unwrap();
        }
        let worker_release = std::sync::Arc::clone(&release);
        let worker = std::thread::spawn(move || {
            while !worker_release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        {
            let mut server = state.lock().unwrap();
            server.track_connection_worker(worker);
        }
        let started = Instant::now();
        assert_eq!(
            drain_server(&state, 1, false),
            Err(WebFailure::WorkerDrainTimeout)
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(1));

        release.store(true, Ordering::Release);
        state.lock().unwrap().release_connection();
        loop {
            let finished = {
                let mut server = state.lock().unwrap();
                server.reap_finished_workers();
                server.tracked_worker_count() == 0
            };
            if finished {
                break;
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn drain_server_bounds_listener_join_at_the_minimum_timeout() {
        let state = Arc::new(Mutex::new(ServerState::new()));
        let release = Arc::new(AtomicBool::new(false));
        let listener_release = std::sync::Arc::clone(&release);
        let listener = std::thread::spawn(move || {
            while !listener_release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        state.lock().unwrap().install_listener(listener).unwrap();

        assert_eq!(
            drain_server(&state, 1, false),
            Err(WebFailure::ListenerJoinTimeout)
        );
        release.store(true, Ordering::Release);
        drain_server(&state, 1000, false).unwrap();
    }
}
