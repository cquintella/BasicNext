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
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub use bn_types::error_codes::exec::*;

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

/// Why a call produced `Error` rather than a result: the fields of the BN
/// `Error` (error.md) both backends build from it unchanged.
#[derive(Debug)]
pub struct Failure {
    pub code: i32,
    program: String,
    cause: String,
}

impl Failure {
    fn new(code: i32, cause: impl Into<String>) -> Self {
        Self {
            code,
            program: String::new(),
            cause: cause.into(),
        }
    }

    /// `Error.Operation`.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        "HOST.Exec.Run"
    }

    /// `Error.Message`: the program that could not run.
    #[must_use]
    pub fn message(&self) -> String {
        format!("cannot run \"{}\"", self.program)
    }

    /// `Error.Cause`: the rule, policy, or operating-system error.
    #[must_use]
    pub fn cause(&self) -> &str {
        &self.cause
    }
}

#[derive(Debug, Default)]
struct StreamCapture {
    bytes: Vec<u8>,
    overflow: bool,
}

/// Drains a captured stream. Keeps reading past the ceiling so the child
/// cannot block on a full pipe, but discards the bytes and reports overflow
/// through [`StreamCapture::overflow`]. Partial output is dropped on overflow.
fn read_pipe<R: Read>(mut pipe: R, capture_limit: usize) -> StreamCapture {
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
    let overflow = exceeded || total > capture_limit;
    if overflow {
        bytes.clear();
    }
    StreamCapture { bytes, overflow }
}

/// Runs `program` with `args` under `policy`.
///
/// # Errors
///
/// Returns a [`Failure`] with one of the portable codes: policy denial (11),
/// an empty program (1), spawn failures (2–4), wait failure (5), invalid
/// UTF-8 (7), capture overflow (8), timeout (9) or a child that could not be
/// terminated after the timeout (10).
pub fn run(program: &str, args: &[&str], policy: &Policy) -> Result<Output, Failure> {
    run_checked(program, args, policy).map_err(|failure| Failure {
        program: program.to_owned(),
        ..failure
    })
}

/// Policy and argument rules, before any process exists.
fn check(program: &str, args: &[&str], policy: &Policy) -> Result<(), Failure> {
    if !policy.allowed {
        return Err(Failure::new(
            POLICY_DENIED,
            "the execution policy denies HOST.Exec",
        ));
    }
    if program.is_empty() || program.contains('\0') {
        return Err(Failure::new(
            INVALID_ARGUMENT,
            "the program must be non-empty and contain no NUL",
        ));
    }
    if args.iter().any(|argument| argument.contains('\0')) {
        return Err(Failure::new(INVALID_ARGUMENT, "an argument contains NUL"));
    }
    Ok(())
}

/// Contains a child past its timeout: kill, then reap. A kill that fails
/// returns `TERMINATION_FAILED` at once; waiting on a child that is still
/// running would block past the timeout. `kill` is a parameter so a test can
/// make it fail.
fn terminate(child: &mut Child, kill: fn(&mut Child) -> io::Result<()>) -> Result<(), Failure> {
    kill(child).map_err(|error| {
        Failure::new(
            TERMINATION_FAILED,
            format!("failed to kill the child process after the timeout: {error}"),
        )
    })?;
    child.wait().map(drop).map_err(|error| {
        Failure::new(
            TERMINATION_FAILED,
            format!("failed to reap the child process after the timeout: {error}"),
        )
    })
}

#[allow(clippy::too_many_lines)]
fn run_checked(program: &str, args: &[&str], policy: &Policy) -> Result<Output, Failure> {
    check(program, args, policy)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(Failure::new(
                PROGRAM_NOT_FOUND,
                format!("no such program at that path or on PATH ({error})"),
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(Failure::new(PERMISSION_DENIED, error.to_string()));
        }
        Err(error) => return Err(Failure::new(SPAWN_FAILED, error.to_string())),
    };
    let capture_limit = policy.capture_limit;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();

    if let Some(pipe) = stdout {
        thread::spawn(move || {
            let res = read_pipe(pipe, capture_limit);
            let _ = out_tx.send(res);
        });
    } else {
        let _ = out_tx.send(StreamCapture::default());
    }

    if let Some(pipe) = stderr {
        thread::spawn(move || {
            let res = read_pipe(pipe, capture_limit);
            let _ = err_tx.send(res);
        });
    } else {
        let _ = err_tx.send(StreamCapture::default());
    }

    let deadline = Instant::now() + policy.timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                terminate(&mut child, Child::kill)?;
                // Return immediately without waiting for reader threads to prevent deadlock
                // if background grandchildren inherited pipe file descriptors.
                return Err(Failure::new(
                    TIMEOUT,
                    format!(
                        "the process ran past the {} ms execution timeout and was killed",
                        policy.timeout.as_millis()
                    ),
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(error) => return Err(Failure::new(WAIT_FAILED, error.to_string())),
        }
    };

    // Bounded timeout for pipe drain in case background grandchildren hold descriptors open.
    let stdout = out_rx
        .recv_timeout(Duration::from_millis(500))
        .unwrap_or_default();
    let stderr = err_rx
        .recv_timeout(Duration::from_millis(500))
        .unwrap_or_default();

    // D-H1-02: count bytes before UTF-8 validation; overflow is a stable Error, not truncation.
    if stdout.overflow || stderr.overflow {
        return Err(Failure::new(
            CAPTURE_LIMIT,
            format!("a stream wrote more than the {capture_limit} bytes captured per stream"),
        ));
    }
    let Ok(stdout) = String::from_utf8(stdout.bytes) else {
        return Err(Failure::new(
            INVALID_UTF8,
            "the program's stdout is not valid UTF-8",
        ));
    };
    let Ok(stderr) = String::from_utf8(stderr.bytes) else {
        return Err(Failure::new(
            INVALID_UTF8,
            "the program's stderr is not valid UTF-8",
        ));
    };
    // A Unix signal death has no exit code: report the negated signal.
    #[cfg(unix)]
    let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal().map(|signal| -i64::from(signal))
    };
    #[cfg(not(unix))]
    let signal = None;
    let return_code = status.code().map(i64::from).or(signal).unwrap_or(-1);
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

    /// A kill that fails during containment is `TERMINATION_FAILED`, returned
    /// at once: the child is still running, so waiting on it would block.
    #[test]
    fn failed_kill_during_containment_is_termination_failed() {
        fn refuse(_: &mut Child) -> io::Result<()> {
            Err(io::Error::other("injected kill failure"))
        }
        #[cfg(unix)]
        let mut command = Command::new("/bin/sh");
        #[cfg(unix)]
        command.args(["-c", "sleep 30"]);
        #[cfg(windows)]
        let mut command = Command::new("cmd");
        #[cfg(windows)]
        command.args(["/C", "ping -n 31 127.0.0.1 >NUL"]);
        let mut child = command.spawn().expect("spawn a long-running child");
        let start = Instant::now();
        let failure = terminate(&mut child, refuse).unwrap_err();
        assert_eq!(failure.code, TERMINATION_FAILED);
        assert!(failure.cause.contains("injected kill failure"));
        assert!(start.elapsed() < Duration::from_secs(5));
        child.kill().expect("clean up the child");
        child.wait().expect("reap the child");
    }

    #[test]
    fn containment_kills_and_reaps_a_running_child() {
        #[cfg(unix)]
        let mut child = Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .spawn()
            .expect("spawn");
        #[cfg(windows)]
        let mut child = Command::new("cmd")
            .args(["/C", "ping -n 31 127.0.0.1 >NUL"])
            .spawn()
            .expect("spawn");
        assert!(terminate(&mut child, Child::kill).is_ok());
        assert!(child.try_wait().expect("status").is_some());
    }
}
