// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Platform stdio for native programs: how emitted code reaches and locks the
// C `stdout` stream (POSIX libc or the Microsoft UCRT), and the binary-mode
// setup that keeps Windows output byte-identical to the interpreter.

/// Libc `FILE *stdout` symbol name for native PRINT synchronization on
/// POSIX C libraries (the Microsoft UCRT has no such global).
fn stdout_file_symbol() -> &'static str {
    if cfg!(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "tvos",
        target_os = "watchos",
        target_os = "freebsd"
    )) {
        "__stdoutp"
    } else {
        "stdout"
    }
}

/// Declarations used by native PRINT synchronization: the C `stdout` stream
/// and the functions that lock and unlock it.
pub(crate) fn stdout_lock_decls() -> String {
    if cfg!(windows) {
        "declare ptr @__acrt_iob_func(i32)\ndeclare void @_lock_file(ptr)\ndeclare void @_unlock_file(ptr)\n"
            .into()
    } else {
        format!(
            "@{} = external global ptr\ndeclare void @flockfile(ptr)\ndeclare void @funlockfile(ptr)\n",
            stdout_file_symbol()
        )
    }
}

/// IR that stores the C `stdout` stream in `dest`: a global on POSIX C
/// libraries, `__acrt_iob_func(1)` in the Microsoft UCRT.
pub(crate) fn stdout_stream_ir(dest: &str) -> String {
    if cfg!(windows) {
        format!("  {dest} = call ptr @__acrt_iob_func(i32 1)")
    } else {
        format!("  {dest} = load ptr, ptr @{}", stdout_file_symbol())
    }
}

/// Entry-block IR that puts stdin, stdout, and stderr in binary mode on
/// Windows (`_setmode(fd, _O_BINARY)`), so `\n` is not rewritten as `\r\n`
/// and native output matches the interpreter byte for byte. Empty for Wasm
/// and on other hosts.
pub(crate) fn windows_binary_stdio_ir(native: bool) -> &'static str {
    if native && cfg!(windows) {
        "  %winstdin = call i32 @_setmode(i32 0, i32 32768)\n  %winstdout = call i32 @_setmode(i32 1, i32 32768)\n  %winstderr = call i32 @_setmode(i32 2, i32 32768)\n"
    } else {
        ""
    }
}

/// Module-level declaration for [`windows_binary_stdio_ir`]. Empty elsewhere.
pub(crate) fn windows_binary_stdio_decl(native: bool) -> &'static str {
    if native && cfg!(windows) {
        "declare i32 @_setmode(i32, i32)\n"
    } else {
        ""
    }
}

/// The C functions that lock and unlock a `FILE *`.
pub(crate) fn stdout_lock_functions() -> (&'static str, &'static str) {
    if cfg!(windows) {
        ("_lock_file", "_unlock_file")
    } else {
        ("flockfile", "funlockfile")
    }
}
