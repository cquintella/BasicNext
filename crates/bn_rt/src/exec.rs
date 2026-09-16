//! Native ABI for `HOST.Exec.Run`.

use std::{
    ffi::{CStr, CString, c_char},
    io::Read,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

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

struct ExecResult {
    return_code: i64,
    stdout: CString,
    stderr: CString,
}

/// Drains a captured stream. Mirrors the interpreter reference: it keeps reading
/// past the ceiling so the child cannot block on a full pipe, but discards the
/// bytes and reports overflow through a sentinel length (`capture_limit + 1`),
/// which the caller maps to `EXEC_CAPTURE_LIMIT`. `Error` has no stream fields,
/// so partial output is intentionally dropped (D-H1-02).
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

#[allow(unsafe_code)]
fn input<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

#[allow(unsafe_code, clippy::too_many_lines)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_run(
    program: *const c_char,
    args: *const *const c_char,
    arg_count: u32,
    out_result: *mut u64,
) -> i32 {
    if out_result.is_null() {
        return EXEC_INVALID_ARGUMENT;
    }
    unsafe { *out_result = 0 };
    if !crate::policy::allows(crate::policy::POLICY_EXEC) {
        return EXEC_POLICY_DENIED;
    }
    let Some(program) = input(program) else {
        return EXEC_INVALID_ARGUMENT;
    };
    if program.is_empty() || (arg_count > 0 && args.is_null()) {
        return EXEC_INVALID_ARGUMENT;
    }
    let mut command = Command::new(program);
    if arg_count > 0 {
        let values = unsafe { std::slice::from_raw_parts(args, arg_count as usize) };
        for value in values {
            let Some(value) = input(*value) else {
                return EXEC_INVALID_ARGUMENT;
            };
            command.arg(value);
        }
    }
    let mut child = match command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return EXEC_PROGRAM_NOT_FOUND;
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return EXEC_PERMISSION_DENIED;
        }
        Err(_) => return EXEC_SPAWN_FAILED,
    };
    let capture_limit = crate::policy::exec_capture_limit();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_thread = thread::spawn(move || stdout.map(|pipe| read_pipe(pipe, capture_limit)));
    let err_thread = thread::spawn(move || stderr.map(|pipe| read_pipe(pipe, capture_limit)));
    let deadline = Instant::now() + crate::policy::exec_timeout();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = out_thread.join();
                let _ = err_thread.join();
                return EXEC_TIMEOUT;
            }
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(_) => return EXEC_WAIT_FAILED,
        }
    };
    let stdout = out_thread.join().ok().flatten().unwrap_or_default();
    let stderr = err_thread.join().ok().flatten().unwrap_or_default();
    // D-H1-02: count bytes before UTF-8 validation; overflow is a stable Error, not truncation.
    if stdout.len() > capture_limit || stderr.len() > capture_limit {
        return EXEC_CAPTURE_LIMIT;
    }
    let (Ok(stdout), Ok(stderr)) = (String::from_utf8(stdout), String::from_utf8(stderr)) else {
        return EXEC_INVALID_UTF8;
    };
    let Ok(stdout) = CString::new(stdout) else {
        return EXEC_CAPTURE_FAILED;
    };
    let Ok(stderr) = CString::new(stderr) else {
        return EXEC_CAPTURE_FAILED;
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
    let result = Box::new(ExecResult {
        return_code,
        stdout,
        stderr,
    });
    unsafe { *out_result = Box::into_raw(result) as u64 };
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_return_code(handle: u64) -> i64 {
    if handle == 0 {
        return 0;
    }
    unsafe { (*(handle as *const ExecResult)).return_code }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stdout(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    unsafe { (*(handle as *const ExecResult)).stdout.as_ptr() }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stderr(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    unsafe { (*(handle as *const ExecResult)).stderr.as_ptr() }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_close(handle: u64) -> i32 {
    if handle == 0 {
        return 0;
    }
    unsafe {
        drop(Box::from_raw(handle as *mut ExecResult));
    }
    0
}
