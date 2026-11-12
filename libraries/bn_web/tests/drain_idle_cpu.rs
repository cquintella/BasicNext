// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Server.Stop` waits on a condition: a drain that has nothing to do uses
//! almost no CPU, and it still ends as soon as the last connection is
//! released.

#![cfg(unix)]
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    reason = "integration tests and test helpers"
)]

use bn_web::{ServerOptions, ServerState, drain_server};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DRAIN_TIMEOUT_MS: i128 = 1000;
/// 5% of the drain timeout: a drain that waits on a condition, not a busy loop.
const CPU_CEILING: Duration = Duration::from_millis(50);
const RELEASE_AFTER: Duration = Duration::from_millis(200);
/// The drain must notice a release well before the timeout.
const PROMPT: Duration = Duration::from_millis(500);
const NANOSECONDS_PER_SECOND: u64 = 1_000_000_000;

/// CPU time of the calling thread, from the operating system.
#[allow(unsafe_code)]
fn thread_cpu_time() -> Duration {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a live, writable `timespec`; the clock id is a constant.
    let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut now) };
    assert_eq!(status, 0, "clock_gettime");
    let seconds = u64::try_from(now.tv_sec).expect("seconds");
    let nanoseconds = u64::try_from(now.tv_nsec).expect("nanoseconds");
    Duration::from_nanos(seconds * NANOSECONDS_PER_SECOND + nanoseconds)
}

fn started_server_with_one_connection() -> Arc<Mutex<ServerState>> {
    let state = Arc::new(Mutex::new(ServerState::new()));
    {
        let mut server = state.lock().expect("state");
        server
            .start_with_options(ServerOptions::default())
            .expect("start");
        server.admit_connection().expect("admit");
    }
    state
}

#[test]
fn an_idle_drain_uses_almost_no_cpu() {
    let state = started_server_with_one_connection();
    let before = thread_cpu_time();
    let result = drain_server(&state, DRAIN_TIMEOUT_MS, false);
    let used = thread_cpu_time().saturating_sub(before);
    assert_eq!(result, Err(bn_web::WebFailure::WorkerDrainTimeout));
    assert!(
        used < CPU_CEILING,
        "the drain used {used:?} of CPU in {DRAIN_TIMEOUT_MS} ms"
    );
}

#[test]
fn a_drain_ends_when_the_last_connection_is_released() {
    let state = started_server_with_one_connection();
    let releaser = {
        let state = Arc::clone(&state);
        thread::spawn(move || {
            thread::sleep(RELEASE_AFTER);
            state.lock().expect("state").release_connection();
        })
    };
    let started = Instant::now();
    drain_server(&state, DRAIN_TIMEOUT_MS, false).expect("the drain ends once released");
    assert!(
        started.elapsed() < PROMPT,
        "the drain took {:?}",
        started.elapsed()
    );
    releaser.join().expect("releaser");
}
