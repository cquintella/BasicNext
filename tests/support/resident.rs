// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Peak resident memory of a child process, for the leak checks of the
//! compiled programs (`host_handle_release`, `crypto_handle_release`).
#![cfg(unix)]

use std::process::Command;

/// Runs `command` to its end and returns its exit status and its peak
/// resident set in bytes.
#[allow(
    unsafe_code,
    // `ru_maxrss` is `c_long`: `i64` on 64-bit targets, `i32` on 32-bit ones.
    clippy::useless_conversion,
    // `wait4` below reaps the child; clippy only recognises `Child::wait`.
    clippy::zombie_processes
)]
pub(crate) fn peak_resident_bytes(command: &mut Command) -> (i32, i64) {
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
