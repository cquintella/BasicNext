// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Server.Stop` / `Server.Close`: stop accepting, join the listener and
//! drain the connections within the timeout, without holding the state lock
//! while waiting (moved from `bn_lib_web`; both backends use it).

use super::ServerState;

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
) -> Result<(), &'static str> {
    let timeout = std::time::Duration::from_millis(
        u64::try_from(timeout_ms).map_err(|_| "stop timeout is outside 1..60000 ms")?,
    );
    let deadline = std::time::Instant::now() + timeout;
    let mut listener = {
        let mut server = state.lock().map_err(|_| "server state unavailable")?;
        server.begin_stop(timeout_ms)?;
        server.take_listener()
    };
    while let Some(handle) = listener {
        if handle.is_finished() {
            if handle.join().is_err() {
                if let Ok(mut server) = state.lock() {
                    server.mark_failed();
                }
                return Err("server listener join failed");
            }
            break;
        }
        if std::time::Instant::now() >= deadline {
            let mut server = state.lock().map_err(|_| "server state unavailable")?;
            server
                .install_listener(handle)
                .map_err(|_| "server listener is already installed")?;
            return Err("server listener join timed out");
        }
        std::thread::yield_now();
        listener = Some(handle);
    }
    loop {
        let result = {
            let mut server = state.lock().map_err(|_| "server state unavailable")?;
            server.finish_stop()
        };
        match result {
            Ok(()) => {
                let workers_finished = state
                    .lock()
                    .map_err(|_| "server state unavailable")?
                    .workers_finished();
                if !workers_finished {
                    if std::time::Instant::now() >= deadline {
                        return Err("server worker drain timed out");
                    }
                    std::thread::yield_now();
                    continue;
                }
                if close {
                    state
                        .lock()
                        .map_err(|_| "server state unavailable")?
                        .mark_closed();
                }
                return Ok(());
            }
            Err("server drain timed out with active connections") => {
                if std::time::Instant::now() >= deadline {
                    return Err("server drain timed out with active connections");
                }
            }
            Err(message) => return Err(message),
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::drain_server;
    use crate::web::ServerState;
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
        let worker_release = release.clone();
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
            Err("server drain timed out with active connections")
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
        let listener_release = release.clone();
        let listener = std::thread::spawn(move || {
            while !listener_release.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        state.lock().unwrap().install_listener(listener).unwrap();

        assert_eq!(
            drain_server(&state, 1, false),
            Err("server listener join timed out")
        );
        release.store(true, Ordering::Release);
        drain_server(&state, 1000, false).unwrap();
    }
}
