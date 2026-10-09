//! Stable C ABI for `BNLog` resources used by compiled programs.
#![allow(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, c_char};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::log::{FileTransport, Level, Record, dispatch_log};
use super::log_error::LogFailure;
use super::policy::{POLICY_CONSOLE, POLICY_FILESYSTEM, allows};

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

#[derive(Clone)]
struct Logger {
    label: String,
    context: BTreeMap<String, String>,
    null_transports: Vec<Level>,
    console_transports: Vec<Level>,
    file_transports: Vec<FileTransport>,
    closed: bool,
}

struct Registry {
    next: AtomicU64,
    fields: Mutex<HashMap<u64, BTreeMap<String, String>>>,
    loggers: Mutex<HashMap<u64, Logger>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry {
        next: AtomicU64::new(1),
        fields: Mutex::new(HashMap::new()),
        loggers: Mutex::new(HashMap::new()),
    })
}

fn next_handle() -> u64 {
    registry().next.fetch_add(1, Ordering::Relaxed)
}

fn text(pointer: *const c_char) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .ok()
        .map(str::to_owned)
}

fn parse_level(value: i64) -> Result<Level, ()> {
    Level::from_i64(value).ok_or(())
}

fn with_fields<T>(operation: impl FnOnce(&mut HashMap<u64, BTreeMap<String, String>>) -> T) -> T {
    operation(
        &mut registry()
            .fields
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

fn with_loggers<T>(operation: impl FnOnce(&mut HashMap<u64, Logger>) -> T) -> T {
    operation(
        &mut registry()
            .loggers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_create() -> u64 {
    let handle = next_handle();
    with_fields(|fields| {
        fields.insert(handle, BTreeMap::new());
    });
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_create() -> u64 {
    let handle = next_handle();
    with_loggers(|loggers| {
        loggers.insert(
            handle,
            Logger {
                label: String::new(),
                context: BTreeMap::new(),
                null_transports: Vec::new(),
                console_transports: Vec::new(),
                file_transports: Vec::new(),
                closed: false,
            },
        );
    });
    handle
}

/// # Panics
///
/// Panics only if the newly allocated logger disappears from the registry
/// between allocation and initialization, which cannot occur while the
/// registry lock is held.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_new(label: *const c_char, out: *mut u64) -> i32 {
    let op = "BNLog.Logger.New";
    let Some(label) = text(label) else {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "logger label",
                len: 0,
                max: 128,
            },
        );
    };
    if out.is_null() {
        return failed(op, &LogFailure::InvalidHandle);
    }
    if label.is_empty() || label.len() > 128 {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "logger label",
                len: label.len(),
                max: 128,
            },
        );
    }
    let handle = bn_rt_log_logger_create();
    with_loggers(|loggers| loggers.get_mut(&handle).unwrap().label = label);
    unsafe { out.write(handle) };
    BN_LOG_OK
}

fn set_field(op: &str, handle: u64, key: *const c_char, value: String) -> i32 {
    let Some(key) = text(key) else {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "field key",
                len: 0,
                max: 128,
            },
        );
    };
    if key.is_empty() || key.len() > 128 {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "field key",
                len: key.len(),
                max: 128,
            },
        );
    }
    with_fields(|fields| {
        let Some(fields) = fields.get_mut(&handle) else {
            return failed(op, &LogFailure::InvalidHandle);
        };
        if fields.contains_key(&key) {
            return failed(op, &LogFailure::DuplicateKey(key));
        }
        if fields.len() >= 64 {
            return failed(
                op,
                &LogFailure::LimitExceeded {
                    what: "fields",
                    count: fields.len(),
                    max: 64,
                },
            );
        }
        fields.insert(key, value);
        BN_LOG_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_set_string(
    handle: u64,
    key: *const c_char,
    value: *const c_char,
) -> i32 {
    let op = "BNLog.Fields.SetString";
    let Some(val) = text(value) else {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "field value",
                len: 0,
                max: 4096,
            },
        );
    };
    set_field(op, handle, key, val)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_set_integer(handle: u64, key: *const c_char, value: i64) -> i32 {
    set_field("BNLog.Fields.SetInteger", handle, key, value.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_set_boolean(handle: u64, key: *const c_char, value: u8) -> i32 {
    set_field(
        "BNLog.Fields.SetBoolean",
        handle,
        key,
        if value != 0 { "TRUE" } else { "FALSE" }.into(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_count(handle: u64, out: *mut i32) -> i32 {
    let op = "BNLog.Fields.Count";
    if out.is_null() {
        return failed(op, &LogFailure::InvalidHandle);
    }
    with_fields(|fields| {
        let Some(fields) = fields.get(&handle) else {
            return failed(op, &LogFailure::InvalidHandle);
        };
        let Ok(count) = i32::try_from(fields.len()) else {
            return failed(
                op,
                &LogFailure::LimitExceeded {
                    what: "fields",
                    count: fields.len(),
                    max: 64,
                },
            );
        };
        unsafe { out.write(count) };
        BN_LOG_OK
    })
}

fn add_transport(op: &str, handle: u64, minimum: i64, kind: &str, path: Option<String>) -> i32 {
    let Ok(minimum_level) = parse_level(minimum) else {
        return failed(
            op,
            &LogFailure::OutOfRange {
                what: "log level",
                value: i128::from(minimum),
                min: 0,
                max: 6,
            },
        );
    };
    with_loggers(|loggers| {
        let Some(logger) = loggers.get_mut(&handle) else {
            return failed(op, &LogFailure::InvalidHandle);
        };
        if logger.closed {
            return failed(op, &LogFailure::Closed);
        }
        if logger.null_transports.len()
            + logger.console_transports.len()
            + logger.file_transports.len()
            >= 8
        {
            return failed(
                op,
                &LogFailure::LimitExceeded {
                    what: "transports",
                    count: 8,
                    max: 8,
                },
            );
        }
        match kind {
            "null" => logger.null_transports.push(minimum_level),
            "console" if allows(POLICY_CONSOLE) => logger.console_transports.push(minimum_level),
            "console" => return failed(op, &LogFailure::CapabilityRequired("HOST.Console")),
            "file" if !allows(POLICY_FILESYSTEM) => {
                return failed(op, &LogFailure::CapabilityRequired("HOST.FileSystem"));
            }
            "file" => {
                let Some(path_str) = path else {
                    return failed(
                        op,
                        &LogFailure::InvalidString {
                            what: "file path",
                            len: 0,
                            max: 4096,
                        },
                    );
                };
                if !super::policy::allows_path(Path::new(&path_str), true) {
                    return failed(
                        op,
                        &LogFailure::IoFailed("path is outside execution policy".into()),
                    );
                }
                logger.file_transports.push(FileTransport {
                    path: path_str,
                    minimum: minimum_level,
                });
            }
            _ => return failed(op, &LogFailure::InvalidTransport("unknown transport")),
        }
        BN_LOG_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_add_null(handle: u64, minimum: i64) -> i32 {
    add_transport("BNLog.Logger.AddNull", handle, minimum, "null", None)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_add_console(handle: u64, minimum: i64) -> i32 {
    add_transport("BNLog.Logger.AddConsole", handle, minimum, "console", None)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_add_file(handle: u64, path: *const c_char, minimum: i64) -> i32 {
    let op = "BNLog.Logger.AddFile";
    let Some(path) = text(path) else {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "file path",
                len: 0,
                max: 4096,
            },
        );
    };
    if path.is_empty() || path.len() > 4096 {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "file path",
                len: path.len(),
                max: 4096,
            },
        );
    }
    add_transport(op, handle, minimum, "file", Some(path))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_child(handle: u64, fields: u64, out: *mut u64) -> i32 {
    let op = "BNLog.Logger.Child";
    if out.is_null() {
        return failed(op, &LogFailure::InvalidHandle);
    }
    let Some(fields) = with_fields(|registry| registry.get(&fields).cloned()) else {
        return failed(op, &LogFailure::InvalidHandle);
    };
    with_loggers(|loggers| {
        let Some(parent) = loggers.get(&handle).cloned() else {
            return failed(op, &LogFailure::InvalidHandle);
        };
        if parent.closed {
            return failed(op, &LogFailure::Closed);
        }
        let child_handle = next_handle();
        let mut child = parent;
        child.context.extend(fields);
        child.closed = false;
        loggers.insert(child_handle, child);
        unsafe { out.write(child_handle) };
        BN_LOG_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_log(
    handle: u64,
    level: i64,
    message: *const c_char,
    fields: u64,
) -> i32 {
    let op = "BNLog.Logger.Log";
    let Ok(level) = parse_level(level) else {
        return failed(
            op,
            &LogFailure::OutOfRange {
                what: "log level",
                value: i128::from(level),
                min: 0,
                max: 6,
            },
        );
    };
    let Some(message) = text(message) else {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "log message",
                len: 0,
                max: 16 * 1024,
            },
        );
    };
    if message.len() > 16 * 1024 {
        return failed(
            op,
            &LogFailure::InvalidString {
                what: "log message",
                len: message.len(),
                max: 16 * 1024,
            },
        );
    }
    let Some(provided) = with_fields(|registry| registry.get(&fields).cloned()) else {
        return failed(op, &LogFailure::InvalidHandle);
    };
    let Some(logger) = with_loggers(|loggers| loggers.get(&handle).cloned()) else {
        return failed(op, &LogFailure::InvalidHandle);
    };
    if logger.closed {
        return failed(op, &LogFailure::Closed);
    }
    let record = Record::with_timestamp(
        crate::format_rfc3339(crate::timestamp_ms()),
        &logger.label,
        level,
        &message,
        &logger.context,
        &provided,
    );
    let Ok(line) = record.json_line() else {
        return failed(
            op,
            &LogFailure::RecordSerialization("record exceeds maximum size".into()),
        );
    };
    let dispatch_res = dispatch_log(
        &line,
        level,
        &logger.console_transports,
        &logger.file_transports,
        |text| {
            super::libc_write_stdout(text.as_bytes())?;
            super::libc_write_stdout(b"\n")
        },
        &|path| {
            super::policy::open_path(path, super::secure_fs::OpenMode::Append)
                .map(|file| Box::new(file) as Box<dyn std::io::Write>)
        },
    );
    if let Err(failure) = dispatch_res {
        return failed(op, &failure);
    }
    BN_LOG_OK
}

fn flush_or_close(handle: u64, timeout_ms: i64, close: bool) -> i32 {
    let op = if close {
        "BNLog.Logger.Close"
    } else {
        "BNLog.Logger.Flush"
    };
    if !(1..=60_000).contains(&timeout_ms) {
        return failed(
            op,
            &LogFailure::OutOfRange {
                what: "timeout",
                value: i128::from(timeout_ms),
                min: 1,
                max: 60_000,
            },
        );
    }
    with_loggers(|loggers| {
        let Some(logger) = loggers.get_mut(&handle) else {
            return failed(op, &LogFailure::InvalidHandle);
        };
        if logger.closed {
            return failed(op, &LogFailure::Closed);
        }
        for transport in &logger.file_transports {
            let result = super::policy::open_path(
                Path::new(&transport.path),
                super::secure_fs::OpenMode::Append,
            )
            .and_then(|file| file.sync_all());
            if let Err(error) = result {
                return failed(op, &LogFailure::IoFailed(error.to_string()));
            }
        }
        if !logger.console_transports.is_empty() && super::libc_fflush().is_err() {
            return failed(op, &LogFailure::IoFailed("cannot flush stdout".into()));
        }
        logger.closed = close;
        BN_LOG_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_flush(handle: u64, timeout_ms: i64) -> i32 {
    flush_or_close(handle, timeout_ms, false)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_close(handle: u64, timeout_ms: i64) -> i32 {
    flush_or_close(handle, timeout_ms, true)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_close(handle: u64) -> i32 {
    with_fields(|fields| {
        if fields.remove(&handle).is_some() {
            BN_LOG_OK
        } else {
            failed("BNLog.Fields.Close", &LogFailure::InvalidHandle)
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_delete(handle: u64) -> i32 {
    with_loggers(|loggers| {
        if loggers.remove(&handle).is_some() {
            BN_LOG_OK
        } else {
            failed("BNLog.Logger.Close", &LogFailure::InvalidHandle)
        }
    })
}

#[cfg(test)]
#[path = "log_abi_tests.rs"]
mod tests;
