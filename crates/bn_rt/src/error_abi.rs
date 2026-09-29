// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! BN `Error` values in native builds (language/0.6/error.md). A native
//! `Error` is `{ i1 true, ptr, i64 code }`; its pointer is an error record
//! (`Message`, `Operation`, `Cause`) created here. A failing ABI call records
//! its message and cause on the thread; emitted code turns that into a record
//! (`bn_rt_error_take`) or wraps a message it already has
//! (`bn_rt_error_wrap`), and reads the fields with `bn_rt_error_field`.

use std::collections::HashSet;
use std::ffi::{CStr, c_char};
use std::sync::{Mutex, OnceLock, PoisonError};

use super::c_string;

/// The `Error` a failed ABI call reports: code, operation, message, cause.
#[derive(Default)]
struct Report {
    code: i32,
    operation: String,
    message: String,
    cause: String,
}

thread_local! {
    /// The report of the last failed ABI call on this thread.
    static LAST_ERROR: std::cell::RefCell<Report> =
        std::cell::RefCell::new(Report::default());
}

/// Records the failure of the current ABI call with only a message (code 1);
/// the emitted code reads it back into the `Error` it builds.
pub(crate) fn set_error(message: impl Into<String>) {
    set_error_report(1, "", message, "");
}

/// Records the full `Error` of the current ABI call.
pub(crate) fn set_error_report(
    code: i32,
    operation: &str,
    message: impl Into<String>,
    cause: impl Into<String>,
) {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = Report {
            code,
            operation: operation.to_owned(),
            message: message.into(),
            cause: cause.into(),
        };
    });
}

/// One native `Error`'s fields: its code and owned NUL-terminated texts.
#[repr(C)]
struct ErrorRecord {
    code: i64,
    message: *mut c_char,
    operation: *mut c_char,
    cause: *mut c_char,
}

/// Addresses of every record created, so a field read can tell a record from
/// a plain message pointer. Records live until the process exits, like the
/// runtime's other owned strings.
fn records() -> &'static Mutex<HashSet<usize>> {
    static RECORDS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    RECORDS.get_or_init(|| Mutex::new(HashSet::new()))
}

#[allow(unsafe_code)] // C ABI: a borrowed NUL-terminated string or null.
fn text(ptr: *const c_char) -> &'static str {
    if ptr.is_null() {
        return "";
    }
    // SAFETY: callers pass live NUL-terminated strings from emitted code or
    // this runtime.
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
}

fn new_record(code: i64, message: &str, operation: &str, cause: &str) -> *mut c_char {
    let record = Box::into_raw(Box::new(ErrorRecord {
        code,
        message: c_string(message),
        operation: c_string(operation),
        cause: c_string(cause),
    }));
    records()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(record as usize);
    record.cast()
}

/// The `Error` pointer for a HOST call: null when `failed` is 0, else a
/// record of the call's recorded report. The recorded operation wins over
/// `operation`; an unrecorded code is 1.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_error_take(failed: i32, operation: *const c_char) -> *mut c_char {
    if failed == 0 {
        return std::ptr::null_mut();
    }
    let report = LAST_ERROR.with(|slot| std::mem::take(&mut *slot.borrow_mut()));
    let operation = if report.operation.is_empty() {
        text(operation)
    } else {
        &report.operation
    };
    new_record(
        i64::from(report.code.max(1)),
        &report.message,
        operation,
        &report.cause,
    )
}

/// The pointer field of an alternative: `value` unchanged when `failed` is
/// false or `value` is already a record (an `Error` passed on), else a
/// record of the message `value` and `operation`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_error_wrap(
    failed: bool,
    value: *mut c_char,
    operation: *const c_char,
) -> *mut c_char {
    if !failed || is_record(value) {
        return value;
    }
    new_record(1, text(value), text(operation), "")
}

/// `Error.Code` of a record from `bn_rt_error_take` (1 for any other pointer).
#[allow(unsafe_code)] // C ABI: reads a record this module created.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_error_code(error: *const c_char) -> i64 {
    if !is_record(error) {
        return 1;
    }
    // SAFETY: a live, never-freed `ErrorRecord` from `new_record`.
    #[allow(clippy::cast_ptr_alignment)]
    let record = unsafe { &*error.cast::<ErrorRecord>() };
    record.code
}

fn is_record(pointer: *const c_char) -> bool {
    records()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&(pointer as usize))
}

/// Field `field` of an `Error`'s pointer: 0 `Message`, 1 `Operation`,
/// 2 `Cause`. A pointer that is not a record is a plain message (every field
/// but `Message` is empty); null is an empty `Message`.
#[allow(unsafe_code)] // C ABI: reads a record this module created.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_error_field(error: *const c_char, field: i32) -> *const c_char {
    if !is_record(error) {
        return if field == 0 && !error.is_null() {
            error
        } else {
            c"".as_ptr()
        };
    }
    // SAFETY: the address is a live, never-freed `ErrorRecord` from
    // `new_record` (a `Box`, so it is aligned for `ErrorRecord`).
    #[allow(clippy::cast_ptr_alignment)]
    let record = unsafe { &*error.cast::<ErrorRecord>() };
    match field {
        0 => record.message,
        1 => record.operation,
        _ => record.cause,
    }
}

/// The code and message of an `Error` a compiled worker returned: its record
/// when `error` is one, else `fallback_code` and `error` as a plain message.
pub(crate) fn code_and_message(error: *const c_char, fallback_code: i64) -> (i64, String) {
    let code = if is_record(error) {
        bn_rt_error_code(error)
    } else {
        fallback_code
    };
    (code, text(bn_rt_error_field(error, 0)).to_owned())
}

/// `PRINT` of an `Error` with `code` and pointer `error`, in the shared text
/// (`bn_types::text::error`).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_error_print(code: i64, error: *const c_char) {
    use std::io::Write as _;
    let line = bn_types::text::error(
        code,
        text(bn_rt_error_field(error, 1)),
        text(bn_rt_error_field(error, 0)),
        text(bn_rt_error_field(error, 2)),
    );
    let _ = super::LibcStdout.write_all(line.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::{
        bn_rt_error_code, bn_rt_error_field, bn_rt_error_take, bn_rt_error_wrap, set_error_report,
        text,
    };

    #[test]
    fn records_carry_three_fields_and_plain_messages_still_read() {
        assert!(bn_rt_error_take(0, c"HOST.X".as_ptr()).is_null());
        set_error_report(2, "HOST.FileSystem.Open", "cannot open", "not found");
        let record = bn_rt_error_take(1, std::ptr::null());
        assert_eq!(bn_rt_error_code(record), 2);
        assert_eq!(text(bn_rt_error_field(record, 0)), "cannot open");
        assert_eq!(text(bn_rt_error_field(record, 1)), "HOST.FileSystem.Open");
        assert_eq!(text(bn_rt_error_field(record, 2)), "not found");

        let plain = c"column not found".as_ptr().cast_mut();
        assert_eq!(bn_rt_error_wrap(false, plain, std::ptr::null()), plain);
        let wrapped = bn_rt_error_wrap(true, plain, std::ptr::null());
        assert_eq!(text(bn_rt_error_field(wrapped, 0)), "column not found");
        assert_eq!(text(bn_rt_error_field(wrapped, 2)), "");
        // Passing an `Error` on keeps its record.
        assert_eq!(bn_rt_error_wrap(true, wrapped, std::ptr::null()), wrapped);
        // A pointer from a producer that does not wrap reads as a message.
        assert_eq!(text(bn_rt_error_field(plain, 0)), "column not found");
        assert_eq!(text(bn_rt_error_field(plain, 1)), "");
        assert_eq!(text(bn_rt_error_field(std::ptr::null(), 0)), "");
    }
}
