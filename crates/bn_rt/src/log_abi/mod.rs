// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stable C ABI for `BNLog` loggers used by compiled programs (bnlog.md,
//! "API (0.6.5)"). The logger is the shared core (`bn_core_log`); a record's
//! STRUCT arrives as its fields, in declaration order. A failure records its
//! `Error` and returns its code.
#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::policy::{POLICY_CONSOLE, POLICY_FILESYSTEM, allows};
use bn_core_log::{
    Apache, Event, FieldMap, Imports, Level, LogFailure, Logger, Operation, TIMEOUT_MAX_MS,
};

pub const BN_LOG_OK: i32 = 0;

fn failed(operation: &str, failure: &LogFailure) -> i32 {
    crate::set_error_report(
        failure.code(),
        operation,
        failure.message(),
        failure.cause(),
    );
    failure.code()
}

fn operation(key: &str) -> &'static str {
    bn_catalog::text("bnlog", key)
}

/// Where compiled programs write: stdout through libc and files under the
/// runtime filesystem policy.
struct RuntimeSink;

impl bn_core_log::LogSink for RuntimeSink {
    fn timestamp(&self) -> String {
        crate::format_rfc3339(crate::timestamp_ms())
    }
    fn console(&self, line: &str) -> std::io::Result<()> {
        super::libc_write_stdout(line.as_bytes())?;
        super::libc_write_stdout(b"\n")
    }
    fn flush_console(&self) -> std::io::Result<()> {
        super::libc_fflush()
    }
    fn open_append(&self, path: &std::path::Path) -> std::io::Result<Box<dyn std::io::Write>> {
        super::policy::open_path(path, super::secure_fs::OpenMode::Append)
            .map(|file| Box::new(file) as Box<dyn std::io::Write>)
    }
}

/// The loggers of the module copy that runs on this thread
/// (`bn_core_copy`): a task's close their destinations when the task ends.
struct Registry {
    next: AtomicU64,
    /// Each logger with the number of bindings that hold it (K61).
    loggers: Mutex<HashMap<u64, (Logger, usize)>>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            next: AtomicU64::new(1),
            loggers: Mutex::new(HashMap::new()),
        }
    }
}

fn registry() -> std::sync::Arc<Registry> {
    bn_core_copy::table::<Registry>()
}

fn with_loggers<T>(body: impl FnOnce(&mut HashMap<u64, (Logger, usize)>) -> T) -> T {
    body(
        &mut registry()
            .loggers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

/// Runs `body` on the logger behind `handle`; an unknown handle is
/// `InvalidHandle`.
fn with_logger(
    handle: u64,
    operation: &str,
    body: impl FnOnce(&mut Logger) -> Result<(), LogFailure>,
) -> i32 {
    let result = with_loggers(|loggers| {
        loggers
            .get_mut(&handle)
            .ok_or(LogFailure::InvalidHandle)
            .and_then(|(logger, _)| body(logger))
    });
    result.map_or_else(|failure| failed(operation, &failure), |()| BN_LOG_OK)
}

/// A BN string from emitted code; a null pointer reads as `""`.
fn text(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    crate::string::string_text(pointer).unwrap_or_else(|| {
        // Fallback for non-BN static strings
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    })
}

/// The fields of a `Log.Event` as emitted code passes them.
fn event_fields(
    service: *const c_char,
    request_id: *const c_char,
    route: *const c_char,
    user: *const c_char,
    detail: *const c_char,
    code: i64,
) -> FieldMap {
    Event {
        service: text(service),
        request_id: text(request_id),
        route: text(route),
        user: text(user),
        detail: text(detail),
        code,
    }
    .fields()
}

/// The fields of a `Log.Apache` as emitted code passes them.
#[allow(clippy::too_many_arguments)]
fn apache_fields(
    remote: *const c_char,
    method: *const c_char,
    path: *const c_char,
    protocol: *const c_char,
    status: i64,
    bytes: i64,
    referrer: *const c_char,
    agent: *const c_char,
) -> FieldMap {
    Apache {
        remote: text(remote),
        method: text(method),
        path: text(path),
        protocol: text(protocol),
        status,
        bytes,
        referrer: text(referrer),
        agent: text(agent),
    }
    .fields()
}

/// `Logger.New(label, format, destination)`: `out` receives the handle.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_new(
    label: *const c_char,
    format: *const c_char,
    destination: *const c_char,
    out: *mut u64,
) -> i32 {
    let op = Operation::New.name();
    if out.is_null() {
        return failed(op, &LogFailure::InvalidHandle);
    }
    let imports = Imports {
        console: allows(POLICY_CONSOLE),
        filesystem: allows(POLICY_FILESYSTEM),
    };
    let opened = Logger::open(
        &text(label),
        &text(format),
        &text(destination),
        imports,
        &RuntimeSink,
    );
    match opened {
        Ok(logger) => {
            let handle = registry().next.fetch_add(1, Ordering::Relaxed);
            with_loggers(|loggers| loggers.insert(handle, (logger, 1)));
            // SAFETY: `out` was checked non-null; emitted code passes a
            // writable `i64` slot.
            unsafe { out.write(handle) };
            BN_LOG_OK
        }
        Err(failure) => failed(op, &failure),
    }
}

/// `Logger.Error` … `Logger.Silly`: one record at `level` (`0..6`).
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_write(
    handle: u64,
    level: i64,
    message: *const c_char,
    service: *const c_char,
    request_id: *const c_char,
    route: *const c_char,
    user: *const c_char,
    detail: *const c_char,
    code: i64,
) -> i32 {
    let Some(level) = Level::from_i64(level) else {
        return failed(
            operation("op.logger.info"),
            &LogFailure::OutOfRange {
                what: operation("what.level"),
                value: i128::from(level),
                min: Level::Error as i128,
                max: Level::Silly as i128,
            },
        );
    };
    let event = event_fields(service, request_id, route, user, detail, code);
    let message = text(message);
    with_logger(handle, level.operation(), |logger| {
        logger.write_event(&RuntimeSink, level, &message, &event)
    })
}

/// `Logger.Access`: one access record.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_access(
    handle: u64,
    remote: *const c_char,
    method: *const c_char,
    path: *const c_char,
    protocol: *const c_char,
    status: i64,
    bytes: i64,
    referrer: *const c_char,
    agent: *const c_char,
) -> i32 {
    let request = apache_fields(
        remote, method, path, protocol, status, bytes, referrer, agent,
    );
    with_logger(handle, Operation::Access.name(), |logger| {
        logger.write_access(&RuntimeSink, &request)
    })
}

/// `Logger.SetLevel`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_set_level(handle: u64, minimum: i64) -> i32 {
    with_logger(handle, Operation::SetLevel.name(), |logger| {
        logger.set_level(i128::from(minimum))
    })
}

/// `Logger.SetDefaults`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_set_defaults(
    handle: u64,
    service: *const c_char,
    request_id: *const c_char,
    route: *const c_char,
    user: *const c_char,
    detail: *const c_char,
    code: i64,
) -> i32 {
    let defaults = event_fields(service, request_id, route, user, detail, code);
    with_logger(handle, Operation::SetDefaults.name(), |logger| {
        logger.set_event_defaults(defaults)
    })
}

/// `Logger.SetAccessDefaults`.
#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_set_access_defaults(
    handle: u64,
    remote: *const c_char,
    method: *const c_char,
    path: *const c_char,
    protocol: *const c_char,
    status: i64,
    bytes: i64,
    referrer: *const c_char,
    agent: *const c_char,
) -> i32 {
    let defaults = apache_fields(
        remote, method, path, protocol, status, bytes, referrer, agent,
    );
    with_logger(handle, Operation::SetAccessDefaults.name(), |logger| {
        logger.set_access_defaults(defaults)
    })
}

/// `Logger.Flush`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_flush(handle: u64, timeout_ms: i64) -> i32 {
    with_logger(handle, Operation::Flush.name(), |logger| {
        logger.flush(&RuntimeSink, i128::from(timeout_ms))
    })
}

/// `Logger.Close`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_close(handle: u64, timeout_ms: i64) -> i32 {
    with_logger(handle, Operation::Close.name(), |logger| {
        logger.close(&RuntimeSink, i128::from(timeout_ms))
    })
}

/// One more binding holds the logger behind `handle` (a copy). Handle 0 is
/// the error case of `Logger OR Error`, which holds no logger.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_retain(handle: u64) -> i32 {
    if handle == 0 {
        return BN_LOG_OK;
    }
    let held = with_loggers(|loggers| {
        loggers
            .get_mut(&handle)
            .map(|(_, holders)| *holders += 1)
            .is_some()
    });
    if held {
        BN_LOG_OK
    } else {
        failed(Operation::Close.name(), &LogFailure::InvalidHandle)
    }
}

/// One binding fewer holds the logger behind `handle`; the last one destroys
/// it, flushing and closing its destination (records are written when
/// received). Handle 0 holds no logger.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_delete(handle: u64) -> i32 {
    if handle == 0 {
        return BN_LOG_OK;
    }
    let destroyed = with_loggers(|loggers| {
        let (_, holders) = loggers.get_mut(&handle)?;
        *holders -= 1;
        if *holders == 0 {
            loggers.remove(&handle).map(|(logger, _)| logger)
        } else {
            None
        }
    });
    if let Some(mut logger) = destroyed {
        let _ = logger.close(&RuntimeSink, TIMEOUT_MAX_MS);
    }
    BN_LOG_OK
}

#[cfg(test)]
#[path = "abi_tests.rs"]
mod tests;
