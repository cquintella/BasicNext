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

/// The live `HOST.Exec.Result` values. An id is generation-checked
/// (`crate::handle_table`): a closed result is never read or freed twice.
/// No ceiling: a result lives only as long as the binding that holds it.
static RESULTS: std::sync::Mutex<crate::handle_table::GenerationTable<ExecResult>> =
    std::sync::Mutex::new(crate::handle_table::GenerationTable::new());

/// Runs `f` on the live result `handle`. A closed or unknown handle is a
/// `USE_AFTER_RELEASE` failure that ends the process, never another result.
fn with_result<T>(handle: u64, f: impl FnOnce(&ExecResult) -> T) -> T {
    let results = RESULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    i64::try_from(handle)
        .ok()
        .and_then(|id| results.get(id))
        .map_or_else(|| crate::arc_abi::abort_use_after_release(), f)
}

fn input<'a>(ptr: *const c_char) -> Option<&'a str> {
    // A program or argument is a STRING. An interior NUL stays in the text so
    // `bn_host_exec` rejects it the same way `bni` does.
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
    let result = ExecResult {
        return_code: output.return_code,
        stdout,
        stderr,
    };
    let id = RESULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(result, usize::MAX)
        .unwrap_or_else(|| crate::arc_abi::abort_use_after_release());
    // SAFETY: `out_result` was checked non-null above and points to the
    // caller's result slot.
    unsafe { *out_result = id.cast_unsigned() };
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_return_code(handle: u64) -> i64 {
    if handle == 0 {
        return 0;
    }
    with_result(handle, |result| result.return_code)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stdout(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    with_result(handle, |result| {
        let bytes = result.stdout.strip_suffix(&[0]).unwrap_or(&result.stdout);
        crate::string::c_string_from_bytes(bytes)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stderr(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    with_result(handle, |result| {
        let bytes = result.stderr.strip_suffix(&[0]).unwrap_or(&result.stderr);
        crate::string::c_string_from_bytes(bytes)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_close(handle: u64) -> i32 {
    if handle == 0 {
        return 0;
    }
    // Idempotent: a second close of the same id finds nothing to free.
    if let Ok(id) = i64::try_from(handle) {
        drop(
            RESULTS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(id),
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{ExecResult, RESULTS, bn_rt_exec_result_close};

    /// A second close of the same result finds nothing to free (it was a
    /// double free on a raw pointer before the generation table).
    #[test]
    fn closing_a_result_twice_frees_it_once() {
        let id = RESULTS
            .lock()
            .expect("results")
            .insert(
                ExecResult {
                    return_code: 0,
                    stdout: vec![0],
                    stderr: vec![0],
                },
                usize::MAX,
            )
            .expect("insert");
        let handle = id.cast_unsigned();
        assert_eq!(bn_rt_exec_result_close(handle), 0);
        assert_eq!(bn_rt_exec_result_close(handle), 0);
        assert!(RESULTS.lock().expect("results").get(id).is_none());
    }
}
