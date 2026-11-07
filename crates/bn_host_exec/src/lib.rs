// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Exec.Run` — the one implementation both backends call. This file
//! holds the execution policy the caller hands
//! in, the portable failure codes, and [`run`]: spawn with a closed stdin,
//! capture both streams concurrently under a per-stream ceiling, enforce
//! the wall-clock timeout, and map the exit status. The interpreter provider
//! turns the result into BN values; `bn_rt::exec` turns it into a C-ABI
//! handle. Neither re-implements any of it.

use std::{
    io::{self, Read},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

pub use bn_types::error_codes::exec::*;

/// How long the call waits, after it ended the process tree, for the capture
/// threads to see end of file. They end at once unless a descendant left the
/// tree (see `bn_os_process`), and then the call must not wait for it.
const READER_GRACE: Duration = Duration::from_secs(2);

/// Reader threads alive now. A reader ends when every holder of its pipe has
/// closed it, so after a timeout that ends the whole process tree the count
/// returns to what it was before the call.
static LIVE_READERS: AtomicUsize = AtomicUsize::new(0);

/// Counts a reader thread from before it starts until it ends.
struct ReaderGuard;

impl ReaderGuard {
    fn new() -> Self {
        LIVE_READERS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for ReaderGuard {
    fn drop(&mut self) {
        LIVE_READERS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The number of capture threads alive. It exists so a test can show that
/// repeated timeouts leave none behind.
#[doc(hidden)]
#[must_use]
pub fn live_reader_threads() -> usize {
    LIVE_READERS.load(Ordering::SeqCst)
}

/// A process tree the platform contains: the child and what it starts.
pub trait ContainedTree {
    /// Ends every process of the tree; a tree already gone is success.
    ///
    /// # Errors
    ///
    /// The operating-system error.
    fn kill(&self) -> io::Result<()>;
}

/// The operating system's side of a call (process groups, job objects,
/// signals): the core stays safe Rust, and each adapter hands it the
/// implementation (`bn_os_process::OsProcessControl`), as it hands the
/// [`Policy`].
pub trait ProcessControl {
    /// Puts a child that was just started, in a process group of its own on
    /// Unix, under containment until the returned tree is dropped.
    ///
    /// # Errors
    ///
    /// The operating-system error when the child cannot be contained.
    fn contain(&self, child: &Child) -> io::Result<Box<dyn ContainedTree>>;
}

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
        bn_catalog::fill("exec", "run.message", &[("program", &self.program)])
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
    /// The read error that ended the capture before end of file.
    failure: Option<String>,
}

/// Drains a captured stream. Keeps reading past the ceiling so the child
/// cannot block on a full pipe, but discards the bytes and reports overflow
/// through [`StreamCapture::overflow`]. Partial output is dropped on overflow.
fn read_pipe<R: Read>(mut pipe: R, capture_limit: usize) -> StreamCapture {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    let mut total = 0_usize;
    let mut exceeded = false;
    let mut failure = None;
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                failure = Some(error.to_string());
                break;
            }
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
    StreamCapture {
        bytes,
        overflow,
        failure,
    }
}

/// Waits for one stream's capture until `deadline`. The wall clock runs
/// through pipe end of file (0.6.md, `HOST.Exec` rule 5): a process the child
/// started may keep the pipe open after the child exits, and its output is
/// never dropped — the call ends with `TIMEOUT` instead.
fn collect(
    capture: &mpsc::Receiver<StreamCapture>,
    deadline: Instant,
    policy: &Policy,
) -> Result<StreamCapture, Failure> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    let stream = capture
        .recv_timeout(remaining)
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => Failure::new(
                TIMEOUT,
                bn_catalog::fill(
                    "exec",
                    "run.pipe_timeout",
                    &[("timeout_ms", &policy.timeout.as_millis())],
                ),
            ),
            mpsc::RecvTimeoutError::Disconnected => {
                Failure::new(CAPTURE_FAILED, bn_catalog::text("exec", "run.reader_lost"))
            }
        })?;
    match stream.failure {
        Some(cause) => Err(Failure::new(CAPTURE_FAILED, cause)),
        None => Ok(stream),
    }
}

/// Runs `program` with `args` under `policy`.
///
/// # Errors
///
/// Returns a [`Failure`] with one of the portable codes: policy denial (11),
/// an empty program (1), spawn failures (2–4), wait failure (5), invalid
/// UTF-8 (7), capture overflow (8), timeout (9) or a child that could not be
/// terminated after the timeout (10).
pub fn run(
    program: &str,
    args: &[&str],
    policy: &Policy,
    control: &dyn ProcessControl,
) -> Result<Output, Failure> {
    run_checked(program, args, policy, control).map_err(|failure| Failure {
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

/// Ends the whole tree past the timeout, reaps the child, and waits a short,
/// fixed time for the capture threads: with the tree gone every pipe end is
/// closed, so they finish at once.
fn end_tree(
    tree: &dyn ContainedTree,
    child: &mut Child,
    readers: &[&mpsc::Receiver<StreamCapture>],
) -> Result<(), Failure> {
    // The child is killed and reaped even when the tree could not be ended,
    // so a failure never leaves it running unreaped.
    let tree_ended = tree.kill();
    terminate(child, Child::kill)?;
    tree_ended.map_err(|error| {
        Failure::new(
            TERMINATION_FAILED,
            bn_catalog::fill("exec", "run.tree_kill_failed", &[("cause", &error)]),
        )
    })?;
    let deadline = Instant::now() + READER_GRACE;
    for reader in readers {
        let _ = reader.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run_checked(
    program: &str,
    args: &[&str],
    policy: &Policy,
    control: &dyn ProcessControl,
) -> Result<Output, Failure> {
    check(program, args, policy)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // A process group of its own, whose id is the child's pid, so the
    // timeout can end the child and everything it starts.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
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
    let tree = match control.contain(&child) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Failure::new(
                SPAWN_FAILED,
                bn_catalog::fill("exec", "run.tree_adopt_failed", &[("cause", &error)]),
            ));
        }
    };
    let capture_limit = policy.capture_limit;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let (out_tx, out_rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel();

    if let Some(pipe) = stdout {
        let alive = ReaderGuard::new();
        thread::spawn(move || {
            let _alive = alive;
            let res = read_pipe(pipe, capture_limit);
            let _ = out_tx.send(res);
        });
    } else {
        let _ = out_tx.send(StreamCapture::default());
    }

    if let Some(pipe) = stderr {
        let alive = ReaderGuard::new();
        thread::spawn(move || {
            let _alive = alive;
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
                end_tree(tree.as_ref(), &mut child, &[&out_rx, &err_rx])?;
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

    let collected = collect(&out_rx, deadline, policy)
        .and_then(|stdout| collect(&err_rx, deadline, policy).map(|stderr| (stdout, stderr)));
    let (stdout, stderr) = match collected {
        Ok(streams) => streams,
        Err(failure) => {
            // A process the child started still holds a pipe: it is in the
            // tree, and the timeout ends it too.
            if failure.code == TIMEOUT {
                end_tree(tree.as_ref(), &mut child, &[&out_rx, &err_rx])?;
            }
            return Err(failure);
        }
    };

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
