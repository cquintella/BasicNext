//! Native C ABI for `HOST.Exec.Run`: converts the C strings, reads the
//! process-wide policy ceiling and hands the call to `bn_host_exec::run`; the
//! result lives behind an opaque handle (`bn_rt_exec_result_*`). No process
//! is spawned here.
//!
//! A result is a counted handle, like `FS.File`: emitted code retains it for
//! each binding that holds it and releases it when the binding ends; the last
//! release frees it. `Stdout` and `Stderr` are STRING values the result owns;
//! reading one lends that STRING, so a read allocates nothing and a binding
//! that keeps the text retains it.

use std::ffi::c_char;

pub use bn_host_exec::{
    CAPTURE_FAILED, CAPTURE_LIMIT, INVALID_ARGUMENT, INVALID_UTF8, PERMISSION_DENIED,
    POLICY_DENIED, PROGRAM_NOT_FOUND, SPAWN_FAILED, TERMINATION_FAILED, TIMEOUT, WAIT_FAILED,
};

/// A STRING the result owns one reference to, kept as its address so the
/// table stays `Send` (the STRING count is atomic).
struct OwnedText(usize);

impl OwnedText {
    fn new(bytes: &[u8]) -> Self {
        Self(crate::string::c_string_from_bytes(bytes).expose_provenance())
    }

    fn lend(&self) -> *const c_char {
        std::ptr::with_exposed_provenance(self.0)
    }
}

impl Drop for OwnedText {
    fn drop(&mut self) {
        crate::string::bn_rt_str_release(self.lend());
    }
}

struct ExecResult {
    return_code: i64,
    stdout: OwnedText,
    stderr: OwnedText,
    /// Bindings that hold the result; the release that reaches 0 frees it.
    holders: u32,
}

/// The live `HOST.Exec.Result` values. An id is generation-checked
/// (`crate::handle_table_abi`): a released result is never read or freed twice.
static RESULTS: std::sync::Mutex<crate::handle_table_abi::GenerationTable<ExecResult>> =
    std::sync::Mutex::new(crate::handle_table_abi::GenerationTable::new());

/// Runs `f` on the live result `handle`. A released or unknown handle is a
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
    let output = match bn_host_exec::run(
        program,
        &arguments,
        &crate::policy::exec_policy(),
        &bn_os_process::OsProcessControl,
    ) {
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
    let stdout = OwnedText::new(output.stdout.as_bytes());
    let stderr = OwnedText::new(output.stderr.as_bytes());
    let result = ExecResult {
        return_code: output.return_code,
        stdout,
        stderr,
        holders: 1,
    };
    let id = RESULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(result, usize::MAX)
        // Refused only past 2^32 live results, each holding heap memory: that is
        // allocation failure, reported as such.
        .unwrap_or_else(|| std::alloc::handle_alloc_error(std::alloc::Layout::new::<ExecResult>()));
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

/// `Result.Stdout`: lends the STRING the result owns (a field read borrows).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stdout(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    with_result(handle, |result| result.stdout.lend())
}

/// `Result.Stderr`: lends the STRING the result owns.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_stderr(handle: u64) -> *const c_char {
    if handle == 0 {
        return std::ptr::null();
    }
    with_result(handle, |result| result.stderr.lend())
}

/// One more binding holds the result.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_retain(handle: u64) -> i32 {
    if handle == 0 {
        return 0;
    }
    let mut results = RESULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match i64::try_from(handle)
        .ok()
        .and_then(|id| results.get_mut(id))
    {
        Some(result) => {
            result.holders = result.holders.saturating_add(1);
            0
        }
        None => crate::arc_abi::abort_use_after_release(),
    }
}

/// One binding fewer: the last one frees the result and its texts. A
/// released id finds nothing (a stale id never frees another result).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_exec_result_close(handle: u64) -> i32 {
    let Ok(id) = i64::try_from(handle) else {
        return 0;
    };
    let mut results = RESULTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let last = results.get_mut(id).is_some_and(|result| {
        result.holders -= 1;
        result.holders == 0
    });
    if last {
        let result = results.remove(id);
        // The texts are released after the lock is given back.
        drop(results);
        drop(result);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::{
        ExecResult, OwnedText, RESULTS, bn_rt_exec_result_close, bn_rt_exec_result_retain,
        bn_rt_exec_result_stdout,
    };

    fn stored(text: &str) -> i64 {
        RESULTS
            .lock()
            .expect("results")
            .insert(
                ExecResult {
                    return_code: 0,
                    stdout: OwnedText::new(text.as_bytes()),
                    stderr: OwnedText::new(b""),
                    holders: 1,
                },
                usize::MAX,
            )
            .expect("insert")
    }

    /// The last of the holders frees the result; a close past the last
    /// finds nothing (it was a double free on a raw pointer before).
    #[test]
    fn the_last_holder_frees_the_result_once() {
        let id = stored("out");
        let handle = id.cast_unsigned();
        assert_eq!(bn_rt_exec_result_retain(handle), 0);
        assert_eq!(bn_rt_exec_result_close(handle), 0);
        assert!(
            RESULTS.lock().expect("results").get(id).is_some(),
            "one holder left"
        );
        assert_eq!(bn_rt_exec_result_close(handle), 0);
        assert!(RESULTS.lock().expect("results").get(id).is_none());
        assert_eq!(
            bn_rt_exec_result_close(handle),
            0,
            "a stale close is harmless"
        );
    }

    /// Reading `Stdout` lends the same STRING each time: nothing is allocated
    /// per read, and the result keeps its one reference.
    #[test]
    fn reading_stdout_lends_the_text_the_result_owns() {
        let id = stored("out");
        let handle = id.cast_unsigned();
        let first = bn_rt_exec_result_stdout(handle);
        assert_eq!(first, bn_rt_exec_result_stdout(handle));
        assert_eq!(crate::string::slice_from_raw(first), Some(&b"out"[..]));
        assert_eq!(bn_rt_exec_result_close(handle), 0);
    }
}
