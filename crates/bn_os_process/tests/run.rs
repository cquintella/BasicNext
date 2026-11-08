// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bn_host_exec::run` with the operating system's process control: policy
//! and argument rules, capture, ceilings, timeouts and pipes held open by
//! descendants.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

#[cfg(unix)]
use bn_host_exec::{CAPTURE_LIMIT, TIMEOUT};
use bn_host_exec::{INVALID_ARGUMENT, Output, POLICY_DENIED, PROGRAM_NOT_FOUND, Policy};
use bn_os_process::OsProcessControl;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

fn run(program: &str, args: &[&str], policy: &Policy) -> Result<Output, bn_host_exec::Failure> {
    bn_host_exec::run(program, args, policy, &OsProcessControl)
}

fn policy() -> Policy {
    Policy {
        allowed: true,
        timeout: Duration::from_secs(5),
        capture_limit: 64,
    }
}

#[test]
fn denied_policy_is_code_11_before_any_spawn() {
    let denied = Policy {
        allowed: false,
        ..policy()
    };
    let failure = run("definitely-not-a-program", &[], &denied).unwrap_err();
    assert_eq!(failure.code, POLICY_DENIED);
}

#[test]
fn empty_program_is_invalid_argument() {
    assert_eq!(run("", &[], &policy()).unwrap_err().code, INVALID_ARGUMENT);
}

#[test]
fn missing_program_is_code_2() {
    let failure = run("bn-host-exec-no-such-program", &[], &policy()).unwrap_err();
    assert_eq!(failure.code, PROGRAM_NOT_FOUND);
    assert_eq!(failure.operation(), "HOST.Exec.Run");
    assert_eq!(
        failure.message(),
        "cannot run \"bn-host-exec-no-such-program\""
    );
    assert!(
        failure
            .cause()
            .starts_with("no such program at that path or on PATH")
    );
}

#[cfg(unix)]
#[test]
fn captures_streams_status_and_enforces_the_ceiling() {
    let ok = run(
        "/bin/sh",
        &["-c", "printf out; printf err >&2; exit 7"],
        &policy(),
    )
    .unwrap();
    assert_eq!(
        (ok.return_code, ok.stdout.as_str(), ok.stderr.as_str()),
        (7, "out", "err")
    );
    let over = run("/bin/sh", &["-c", "head -c 65 /dev/zero"], &policy()).unwrap_err();
    assert_eq!(over.code, CAPTURE_LIMIT);
    let slow = Policy {
        timeout: Duration::from_millis(50),
        ..policy()
    };
    assert_eq!(
        run("/bin/sh", &["-c", "sleep 5"], &slow).unwrap_err().code,
        TIMEOUT
    );
}

#[cfg(unix)]
#[test]
fn background_grandchild_pipe_does_not_deadlock_execution() {
    let fast = Policy {
        timeout: Duration::from_secs(3),
        ..policy()
    };
    let start = Instant::now();
    let res = run(
        "/bin/sh",
        &["-c", "(sleep 5 >/dev/null 2>&1 &) && printf finished"],
        &fast,
    )
    .unwrap();
    assert_eq!(res.stdout, "finished");
    assert!(start.elapsed() < Duration::from_secs(2));
}

/// A process the child started keeps stdout open after the child exits:
/// its output is kept, not dropped as an empty
/// success.
#[cfg(unix)]
#[test]
fn output_held_open_by_a_grandchild_is_kept() {
    let held = run("/bin/sh", &["-c", "printf hello; sleep 1 &"], &policy()).unwrap();
    assert_eq!((held.return_code, held.stdout.as_str()), (0, "hello"));
}

/// A pipe still open at the deadline ends the call with `TIMEOUT` (0.6.md,
/// `HOST.Exec` rule 5: the wall clock runs through pipe end of file).
#[cfg(unix)]
#[test]
fn output_held_open_past_the_deadline_is_timeout() {
    let short = Policy {
        timeout: Duration::from_millis(300),
        ..policy()
    };
    let start = Instant::now();
    let failure = run("/bin/sh", &["-c", "printf hello; sleep 3 &"], &short).unwrap_err();
    assert_eq!(failure.code, TIMEOUT);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[cfg(windows)]
#[test]
fn captures_streams_status_windows() {
    let ok = run("cmd.exe", &["/c", "cmd.exe /c exit 7"], &policy()).unwrap();
    assert_eq!(ok.return_code, 7);
}
