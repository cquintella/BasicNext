// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Exec.Run` — the one implementation both backends call (bucket
//! 0.5.1d, D-F2-04). This file holds the execution policy the caller hands
//! in, the portable failure codes, and [`run`]: spawn with a closed stdin,
//! capture both streams concurrently under a per-stream ceiling, enforce
//! the wall-clock timeout, and map the exit status. The interpreter provider
//! turns the result into BN values; `bn_rt::exec` turns it into a C-ABI
//! handle. Neither re-implements any of it.

use std::{
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Portable failure codes (`Error.Code` on both backends; D-H1-02).
pub const EXEC_INVALID_ARGUMENT: i32 = 1;
pub const EXEC_PROGRAM_NOT_FOUND: i32 = 2;
pub const EXEC_PERMISSION_DENIED: i32 = 3;
pub const EXEC_SPAWN_FAILED: i32 = 4;
pub const EXEC_WAIT_FAILED: i32 = 5;
pub const EXEC_CAPTURE_FAILED: i32 = 6;
pub const EXEC_INVALID_UTF8: i32 = 7;
pub const EXEC_CAPTURE_LIMIT: i32 = 8;
pub const EXEC_TIMEOUT: i32 = 9;
pub const EXEC_POLICY_DENIED: i32 = 11;

/// The execution policy in force for one call. The interpreter reads it from
/// `HostEnv`; the native runtime from its policy ceiling.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub allowed: bool,
    /// Wall-clock ceiling. Policy may reduce; never exceeds 60 s.
    pub timeout: Duration,
    /// Per-stream capture ceiling in bytes. Policy may reduce; never exceeds 16 MiB.
    pub capture_limit: usize,
}

/// A finished child process.
#[derive(Debug)]
pub struct Output {
    /// Exit code, or `-signal` when the child was killed by a signal (Unix),
    /// or `-1` when neither is known.
    pub return_code: i64,
    pub stdout: String,
    pub stderr: String,
}

/// Why a call produced `Error` rather than a result.
#[derive(Debug)]
pub struct Failure {
    pub code: i32,
    pub message: String,
}

impl Failure {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Drains a captured stream. Keeps reading past the ceiling so the child
/// cannot block on a full pipe, but discards the bytes and reports overflow
/// through a sentinel length (`capture_limit + 1`), which [`run`] maps to
/// [`EXEC_CAPTURE_LIMIT`]. `Error` has no stream fields, so partial output is
/// intentionally dropped (D-H1-02).
fn read_pipe<R: Read>(mut pipe: R, capture_limit: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut total = 0_usize;
    let mut exceeded = false;
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                total = total.saturating_add(count);
                if !exceeded && bytes.len() <= capture_limit {
                    bytes.extend_from_slice(&chunk[..count]);
                    if bytes.len() > capture_limit {
                        exceeded = true;
                        bytes.clear();
                    }
                } else {
                    exceeded = true;
                    bytes.clear();
                }
            }
        }
    }
    if exceeded || total > capture_limit {
        bytes.clear();
        bytes.resize(capture_limit.saturating_add(1), 0);
    }
    bytes
}

/// Runs `program` with `args` under `policy`.
///
/// # Errors
///
/// Returns a [`Failure`] with one of the portable codes: policy denial (11),
/// an empty program (1), spawn failures (2–4), wait failure (5), invalid
/// UTF-8 (7), capture overflow (8) or timeout (9).
pub fn run(program: &str, args: &[&str], policy: &Policy) -> Result<Output, Failure> {
    if !policy.allowed {
        return Err(Failure::new(
            EXEC_POLICY_DENIED,
            "HOST.Exec is denied by execution policy",
        ));
    }
    if program.is_empty() {
        return Err(Failure::new(
            EXEC_INVALID_ARGUMENT,
            "program must be non-empty and contain no NUL",
        ));
    }
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Failure::new(EXEC_PROGRAM_NOT_FOUND, error.to_string()));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(Failure::new(EXEC_PERMISSION_DENIED, error.to_string()));
        }
        Err(error) => return Err(Failure::new(EXEC_SPAWN_FAILED, error.to_string())),
    };
    let capture_limit = policy.capture_limit;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_thread = thread::spawn(move || stdout.map(|pipe| read_pipe(pipe, capture_limit)));
    let err_thread = thread::spawn(move || stderr.map(|pipe| read_pipe(pipe, capture_limit)));
    let deadline = Instant::now() + policy.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                return Err(Failure::new(
                    EXEC_TIMEOUT,
                    "process exceeded execution timeout",
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(error) => return Err(Failure::new(EXEC_WAIT_FAILED, error.to_string())),
        }
    };
    let stdout = out_thread.join().ok().flatten().unwrap_or_default();
    let stderr = err_thread.join().ok().flatten().unwrap_or_default();
    // D-H1-02: count bytes before UTF-8 validation; overflow is a stable Error, not truncation.
    if stdout.len() > capture_limit || stderr.len() > capture_limit {
        return Err(Failure::new(
            EXEC_CAPTURE_LIMIT,
            "captured output exceeded per-stream capture limit",
        ));
    }
    let Ok(stdout) = String::from_utf8(stdout) else {
        return Err(Failure::new(EXEC_INVALID_UTF8, "stdout is not valid UTF-8"));
    };
    let Ok(stderr) = String::from_utf8(stderr) else {
        return Err(Failure::new(EXEC_INVALID_UTF8, "stderr is not valid UTF-8"));
    };
    let return_code = {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        status
            .code()
            .map(i64::from)
            .or_else(|| {
                #[cfg(unix)]
                {
                    status.signal().map(|signal| -i64::from(signal))
                }
                #[cfg(not(unix))]
                {
                    None
                }
            })
            .unwrap_or(-1)
    };
    Ok(Output {
        return_code,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(failure.code, EXEC_POLICY_DENIED);
    }

    #[test]
    fn empty_program_is_invalid_argument() {
        assert_eq!(
            run("", &[], &policy()).unwrap_err().code,
            EXEC_INVALID_ARGUMENT
        );
    }

    #[test]
    fn missing_program_is_code_2() {
        let failure = run("bn-host-exec-no-such-program", &[], &policy()).unwrap_err();
        assert_eq!(failure.code, EXEC_PROGRAM_NOT_FOUND);
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
        assert_eq!(over.code, EXEC_CAPTURE_LIMIT);
        let slow = Policy {
            timeout: Duration::from_millis(50),
            ..policy()
        };
        assert_eq!(
            run("/bin/sh", &["-c", "sleep 5"], &slow).unwrap_err().code,
            EXEC_TIMEOUT
        );
    }
}
