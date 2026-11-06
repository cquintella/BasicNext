// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `HOST.Exec.Run` timeout ends the whole process tree and leaves no
//! capture thread behind (audit finding 5, D-1: group kill). One test per
//! process, so the thread count is not disturbed by another test.

#![cfg(unix)]

use bn_host_exec::{Policy, TIMEOUT, live_reader_threads};
use bn_os_process::OsProcessControl;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TIMEOUT_MS: u64 = 200;
const RUNS: usize = 50;
const CAPTURE_LIMIT_BYTES: usize = 1024;
/// How long a killed process may take to disappear from the process table.
const REAP_GRACE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// The grandchild outlives the test unless the tree is killed.
const GRANDCHILD_SCRIPT: &str = "sleep 60 & echo $! > \"$1\"; wait";

fn policy() -> Policy {
    Policy {
        allowed: true,
        timeout: Duration::from_millis(TIMEOUT_MS),
        capture_limit: CAPTURE_LIMIT_BYTES,
    }
}

/// `kill -0` succeeds while the process exists (a zombie counts, but a
/// grandchild whose parent is gone is reaped by init at once).
fn is_alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn wait_until_gone(pid: &str) -> bool {
    let deadline = Instant::now() + REAP_GRACE;
    while Instant::now() < deadline {
        if !is_alive(pid) {
            return true;
        }
        thread::sleep(POLL_INTERVAL);
    }
    !is_alive(pid)
}

#[test]
fn a_timeout_ends_the_grandchildren_and_the_capture_threads() {
    let directory = std::env::temp_dir().join(format!("bn_host_exec_tree_{}", std::process::id()));
    fs::create_dir_all(&directory).expect("directory");
    let before = live_reader_threads();
    let mut survivors = Vec::new();
    for index in 0..RUNS {
        let pid_file = directory.join(format!("pid_{index}"));
        let pid_path = pid_file.to_str().expect("utf-8 path");
        let failure = bn_host_exec::run(
            "/bin/sh",
            &["-c", GRANDCHILD_SCRIPT, "sh", pid_path],
            &policy(),
            &OsProcessControl,
        )
        .expect_err("the script runs past the timeout");
        assert_eq!(failure.code, TIMEOUT);
        let pid = fs::read_to_string(&pid_file).expect("the pid of the grandchild");
        let pid = pid.trim().to_owned();
        if !wait_until_gone(&pid) {
            survivors.push(pid);
        }
    }
    for pid in &survivors {
        // Do not leave 60-second sleepers behind when the assertion fails.
        let _ = Command::new("kill").args(["-KILL", pid]).status();
    }
    fs::remove_dir_all(&directory).expect("clean up");
    assert!(
        survivors.is_empty(),
        "{} of {RUNS} grandchildren survived the timeout",
        survivors.len()
    );
    let deadline = Instant::now() + REAP_GRACE;
    while live_reader_threads() > before && Instant::now() < deadline {
        thread::sleep(POLL_INTERVAL);
    }
    assert_eq!(
        live_reader_threads(),
        before,
        "capture threads left alive after {RUNS} timeouts"
    );
}
