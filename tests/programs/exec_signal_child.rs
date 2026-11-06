// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A signal that ends a program also ends the process its `HOST.Exec.Run`
//! is waiting for, on both backends: a terminal Ctrl-C (SIGINT to the job's
//! process group) and a SIGTERM or SIGHUP to the program alone. The child
//! leads its own process group (so a timeout can end its whole tree), which
//! keeps the terminal's signals from reaching it directly. Unix only.

#![cfg(unix)]

use super::support::{TestDir, bni, compile_native, workspace_root};
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROGRAM: &str = "tests/host/exec_signal_child.bn";
/// How long the child may take to start and write its pid.
const START_GRACE: Duration = Duration::from_secs(10);
/// How long the program and its child may take to end after the signal.
const END_GRACE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

/// How the signal reaches the program.
#[derive(Clone, Copy, Debug)]
enum Delivery {
    /// A terminal Ctrl-C: SIGINT to the job's process group.
    TerminalInterrupt,
    /// SIGTERM to the program's pid only.
    Terminate,
    /// SIGHUP to the program's pid only (a closed terminal).
    HangUp,
}

fn kill(arguments: &[&str]) {
    let status = Command::new("kill")
        .args(arguments)
        .stderr(Stdio::null())
        .status()
        .expect("run kill");
    assert!(status.success(), "kill {arguments:?}");
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn wait_for(mut condition: impl FnMut() -> bool, limit: Duration) -> bool {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(POLL);
    }
    condition()
}

fn check(mut command: Command, delivery: Delivery, pid_file: &Path, label: &str) {
    // The program leads its own group, as a shell job does.
    let mut program = command
        .arg(pid_file)
        .process_group(0)
        .stdout(Stdio::null())
        .spawn()
        .expect("start the program");
    assert!(
        wait_for(
            || fs::read_to_string(pid_file).is_ok_and(|text| text.ends_with('\n')),
            START_GRACE
        ),
        "{label}: the child never started"
    );
    let child = fs::read_to_string(pid_file).expect("pid").trim().to_owned();
    let program_pid = program.id().to_string();
    match delivery {
        Delivery::TerminalInterrupt => kill(&["-INT", "--", &format!("-{program_pid}")]),
        Delivery::Terminate => kill(&["-TERM", &program_pid]),
        Delivery::HangUp => kill(&["-HUP", &program_pid]),
    }
    let ended = wait_for(
        || program.try_wait().is_ok_and(|status| status.is_some()),
        END_GRACE,
    );
    let child_gone = wait_for(|| !alive(&child), END_GRACE);
    if !child_gone {
        // Leave no 50-second sleeper behind a failed assertion.
        let _ = Command::new("kill").args(["-KILL", &child]).status();
    }
    if !ended {
        let _ = program.kill();
    }
    let _ = program.wait();
    assert!(ended, "{label}: {delivery:?} did not end the program");
    assert!(child_gone, "{label}: {delivery:?} left the child running");
}

const DELIVERIES: [Delivery; 3] = [
    Delivery::TerminalInterrupt,
    Delivery::Terminate,
    Delivery::HangUp,
];

#[test]
fn a_signal_that_ends_the_interpreter_ends_its_exec_child() {
    let source = workspace_root().join(PROGRAM);
    for delivery in DELIVERIES {
        let directory = TestDir::new("exec-signal-bni").expect("directory");
        let mut command = bni();
        command.arg("run").arg(&source).arg("--");
        check(command, delivery, &directory.join("child.pid"), "bni");
    }
}

#[test]
fn a_signal_that_ends_a_compiled_program_ends_its_exec_child() {
    let source = workspace_root().join(PROGRAM);
    let build = TestDir::new("exec-signal-bnc").expect("directory");
    let artifact = compile_native(&source, &build);
    for delivery in DELIVERIES {
        let directory = TestDir::new("exec-signal-run").expect("directory");
        check(
            Command::new(&artifact),
            delivery,
            &directory.join("child.pid"),
            "bnc",
        );
    }
}
