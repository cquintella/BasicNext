//! Native C ABI for `HOST.Exec.Run`: converts the C strings, reads the
//! process-wide policy ceiling and hands the call to `bn_host_exec::run`; the
//! result lives behind an opaque handle (`bn_rt_exec_result_*`). No process
//! is spawned here.

use std::ffi::c_char;

pub use bn_host_exec::{
    CAPTURE_FAILED, CAPTURE_LIMIT, INVALID_ARGUMENT, INVALID_UTF8, PERMISSION_DENIED,
    POLICY_DENIED, PROGRAM_NOT_FOUND, SPAWN_FAILED, TERMINATION_FAILED, TIMEOUT, WAIT_FAILED,
};

struct ExecResult {
    return_code: i64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn input<'a>(ptr: *const c_char) -> Option<&'a str> {
    // A program or argument is a STRING. An interior NUL stays in the text so
    // `bn_host_exec` rejects it the same way `bni` does (G7-D4).
    std::str::from_utf8(crate::string::slice_from_raw(ptr)?).ok()
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_run(
    program: *const c_char,
    args: *const *const c_char,
    arg_count: u32,
    out_result: *mut u64,
) -> i32 {
    if out_result.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { *out_result = 0 };
    let Some(program) = input(program) else {
        return INVALID_ARGUMENT;
    };
    if arg_count > 0 && args.is_null() {
        return INVALID_ARGUMENT;
    }
    let mut arguments = Vec::with_capacity(arg_count as usize);
    if arg_count > 0 {
        // SAFETY: the pointer and the length describe a buffer the caller allocated and keeps alive for this call.
        let values = unsafe { std::slice::from_raw_parts(args, arg_count as usize) };
        for value in values {
            let Some(value) = input(*value) else {
                return INVALID_ARGUMENT;
            };
            arguments.push(value);
        }
    }
    let output = match bn_host_exec::run(program, &arguments, &crate::policy::exec_policy()) {
        Ok(output) => output,
        Err(failure) => {
            crate::set_error_report(
                failure.code,
                failure.operation(),
                failure.message(),
                failure.cause(),
            );
            return failure.code;
        }
    };
    let mut stdout = output.stdout.into_bytes();
    stdout.push(0);
    let mut stderr = output.stderr.into_bytes();
    stderr.push(0);
    let result = Box::new(ExecResult {
        return_code: output.return_code,
        stdout,
        stderr,
    });
    // SAFETY: the handle is the address this module stored with Box::into_raw and has not been freed.
    unsafe { *out_result = Box::into_raw(result) as u64 };
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_return_code(handle: u64) -> i64 {
    if handle == 0 {
        return 0;
    }
    // SAFETY: the handle is the address this module stored with Box::into_raw and has not been freed.
    unsafe { (*(handle as *const ExecResult)).return_code }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stdout(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    // SAFETY: the handle is the address this module stored with Box::into_raw and has not been freed.
    let res = unsafe { &*(handle as *const ExecResult) };
    let bytes = res.stdout.strip_suffix(&[0]).unwrap_or(&res.stdout);
    crate::string::c_string_from_bytes(bytes)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stderr(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    // SAFETY: the handle is the address this module stored with Box::into_raw and has not been freed.
    let res = unsafe { &*(handle as *const ExecResult) };
    let bytes = res.stderr.strip_suffix(&[0]).unwrap_or(&res.stderr);
    crate::string::c_string_from_bytes(bytes)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_close(handle: u64) -> i32 {
    if handle == 0 {
        return 0;
    }
    // SAFETY: the handle is the address this module stored with Box::into_raw and has not been freed.
    unsafe {
        drop(Box::from_raw(handle as *mut ExecResult));
    }
    0
}
