// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Exec.Result` and `HOST.Net.Addresses` are counted handles: a copy
//! holds its own count, the end of the last binding frees the value, and a
//! `Stdout` kept past its result stays valid. A compiled loop of runs keeps
//! its memory flat (each result was kept until exit before). Unix only: the
//! programs run `/bin/sh`.

#![cfg(unix)]

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const OWNERSHIP_PROGRAM: &str = "tests/host/exec_and_address_ownership.bn";
const OWNERSHIP_OUTPUT: &str = "one\n0\nkept\nTRUE\n2\n";
/// 200 runs of 1 MB each: a result kept per run is at least 200 MB.
const RUNS: u32 = 200;
const RESIDENT_CEILING_BYTES: i64 = 64 * 1024 * 1024;

const LOOP_PROGRAM: &str = "IMPORT HOST.Exec AS Exec

FUNCTION Run() AS INTEGER
    LET args AS STRING[2]
    args[0] = \"-c\"
    args[1] = \"head -c 1000000 /dev/zero\"
    LET result AS Exec.Result OR Error = Exec.Run(\"/bin/sh\", args)
    IF result IS Error THEN
        RETURN -1
    END IF
    RETURN LEN(result.Stdout)
END FUNCTION

FUNCTION Start() AS VOID
    LET total AS INTEGER = 0
    FOR i AS INTEGER = 1 TO __RUNS__
        total = total + Run()
    END FOR
    PRINT total
END FUNCTION
";

#[test]
fn exec_results_and_address_lists_keep_their_counts_on_both_backends() {
    let source = workspace_root().join(OWNERSHIP_PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        interpreted.status.success(),
        "{}",
        String::from_utf8_lossy(&interpreted.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        OWNERSHIP_OUTPUT
    );
    let directory = TestDir::new("host-handle-ownership").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), OWNERSHIP_OUTPUT);
}

/// Peak resident memory of one child, from `wait4`.
#[allow(
    unsafe_code,
    // `ru_maxrss` is `c_long`: `i64` on 64-bit targets, `i32` on 32-bit ones.
    clippy::useless_conversion,
    // `wait4` below reaps the child; clippy only recognises `Child::wait`.
    clippy::zombie_processes
)]
fn peak_resident_bytes(command: &mut Command) -> (i32, i64) {
    let child = command.spawn().expect("start the program");
    let pid = libc::pid_t::try_from(child.id()).expect("pid");
    let mut status = 0;
    // SAFETY: an all-zero `rusage` is a valid value for `wait4` to overwrite.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `pid` is our child, not yet waited for; the out pointers are live.
    let waited = unsafe { libc::wait4(pid, &raw mut status, 0, &raw mut usage) };
    assert_eq!(waited, pid, "wait4");
    // `ru_maxrss` is bytes on macOS and kilobytes on Linux.
    let scale = if cfg!(target_os = "macos") { 1 } else { 1024 };
    (status, i64::from(usage.ru_maxrss) * scale)
}

#[test]
fn a_compiled_loop_of_exec_runs_keeps_memory_flat() {
    let directory = TestDir::new("exec-run-loop").expect("directory");
    let source = directory.join("loop.bn");
    fs::write(&source, LOOP_PROGRAM.replace("__RUNS__", &RUNS.to_string())).expect("source");
    let artifact = compile_native(&source, &directory);
    let (status, peak) =
        peak_resident_bytes(Command::new(&artifact).stdout(std::process::Stdio::null()));
    assert_eq!(status, 0, "the program exited cleanly");
    assert!(
        peak < RESIDENT_CEILING_BYTES,
        "peak resident {peak} bytes after {RUNS} runs of 1 MB"
    );
}
