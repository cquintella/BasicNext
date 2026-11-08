// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `HOST.Exec.Run` timeout ends the whole process tree and leaves no
//! capture thread behind (the whole process group or job is killed). One test per
//! process, so the thread count is not disturbed by another test.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use bn_host_exec::{Policy, TIMEOUT, live_reader_threads};
use bn_os_process::OsProcessControl;
use std::fs;
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
const TIMEOUT_MS: u64 = 200;
#[cfg(windows)]
const TIMEOUT_MS: u64 = 1000;

#[cfg(unix)]
const RUNS: usize = 50;
#[cfg(windows)]
const RUNS: usize = 3;

const CAPTURE_LIMIT_BYTES: usize = 1024;
/// How long a killed process may take to disappear from the process table.
const REAP_GRACE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// The grandchild outlives the test unless the tree is killed.
#[cfg(unix)]
const GRANDCHILD_SCRIPT: &str = "sleep 60 & echo $! > \"$1\"; wait";
#[cfg(windows)]
const GRANDCHILD_SCRIPT: &str = "& { param($path) $p = Start-Process powershell -ArgumentList '-NoProfile -Command Start-Sleep 60' -PassThru; Set-Content -Path $path -Value $p.Id; Start-Sleep 60 }";

fn policy() -> Policy {
    Policy {
        allowed: true,
        timeout: Duration::from_millis(TIMEOUT_MS),
        capture_limit: CAPTURE_LIMIT_BYTES,
    }
}

/// Checks while the process exists.
#[cfg(unix)]
fn is_alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn is_alive(pid: &str) -> bool {
    let Ok(pid_num) = pid.parse::<u32>() else {
        return false;
    };
    // SAFETY: querying limited information on the given PID.
    let handle = unsafe {
        windows_sys::Win32::System::Threading::OpenProcess(
            windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid_num,
        )
    };
    if handle.is_null() {
        return false;
    }
    let mut exit_code = 0;
    // SAFETY: handle is valid; writing exit code to raw pointer.
    let success = unsafe {
        windows_sys::Win32::System::Threading::GetExitCodeProcess(handle, &raw mut exit_code)
    };
    // SAFETY: handle is valid and closed once.
    unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
    success != 0 && exit_code == windows_sys::Win32::Foundation::STILL_ACTIVE as u32
}

#[cfg(unix)]
fn kill_survivor(pid: &str) {
    let _ = Command::new("kill").args(["-KILL", pid]).status();
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn kill_survivor(pid: &str) {
    if let Ok(pid_num) = pid.parse::<u32>() {
        // SAFETY: opening process for termination rights.
        let handle = unsafe {
            windows_sys::Win32::System::Threading::OpenProcess(
                windows_sys::Win32::System::Threading::PROCESS_TERMINATE,
                0,
                pid_num,
            )
        };
        if !handle.is_null() {
            // SAFETY: terminating process with handle.
            unsafe { windows_sys::Win32::System::Threading::TerminateProcess(handle, 1) };
            // SAFETY: closing valid handle.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
        }
    }
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
        #[cfg(unix)]
        let failure = bn_host_exec::run(
            "/bin/sh",
            &["-c", GRANDCHILD_SCRIPT, "sh", pid_path],
            &policy(),
            &OsProcessControl,
        )
        .expect_err("the script runs past the timeout");
        #[cfg(windows)]
        let failure = bn_host_exec::run(
            "powershell.exe",
            &["-NoProfile", "-Command", GRANDCHILD_SCRIPT, pid_path],
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
        kill_survivor(pid);
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
