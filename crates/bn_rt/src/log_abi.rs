//! Stable C ABI for `BNLog` resources used by compiled programs.
#![allow(unsafe_code)]

use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, c_char};
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::log::{Level, Record};
use super::policy::{POLICY_CONSOLE, POLICY_FILESYSTEM, allows};

pub const BN_LOG_OK: i32 = 0;
pub const BN_LOG_INVALID_ARGUMENT: i32 = 1;
pub const BN_LOG_INVALID_HANDLE: i32 = 2;
pub const BN_LOG_CLOSED: i32 = 3;
pub const BN_LOG_POLICY_DENIED: i32 = 4;
pub const BN_LOG_TRANSPORT_ERROR: i32 = 5;
pub const BN_LOG_LIMIT_EXCEEDED: i32 = 6;
pub const BN_LOG_DUPLICATE_FIELD: i32 = 7;

#[derive(Clone)]
struct FileTransport {
    path: String,
    minimum: Level,
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

fn parse_level(value: i64) -> Result<Level, i32> {
    Level::from_i64(value).ok_or(BN_LOG_INVALID_ARGUMENT)
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
    let Some(label) = text(label) else {
        return BN_LOG_INVALID_ARGUMENT;
    };
    if out.is_null() || label.is_empty() || label.len() > 128 {
        return BN_LOG_INVALID_ARGUMENT;
    }
    let handle = bn_rt_log_logger_create();
    with_loggers(|loggers| loggers.get_mut(&handle).unwrap().label = label);
    unsafe { out.write(handle) };
    BN_LOG_OK
}

fn set_field(handle: u64, key: *const c_char, value: String) -> i32 {
    let Some(key) = text(key) else {
        return BN_LOG_INVALID_ARGUMENT;
    };
    if key.is_empty() || key.len() > 128 {
        return BN_LOG_INVALID_ARGUMENT;
    }
    with_fields(|fields| {
        let Some(fields) = fields.get_mut(&handle) else {
            return BN_LOG_INVALID_HANDLE;
        };
        if fields.contains_key(&key) {
            return BN_LOG_DUPLICATE_FIELD;
        }
        if fields.len() >= 64 {
            return BN_LOG_LIMIT_EXCEEDED;
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
    text(value).map_or(BN_LOG_INVALID_ARGUMENT, |value| {
        set_field(handle, key, value)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_set_integer(handle: u64, key: *const c_char, value: i64) -> i32 {
    set_field(handle, key, value.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_set_boolean(handle: u64, key: *const c_char, value: u8) -> i32 {
    set_field(
        handle,
        key,
        if value != 0 { "TRUE" } else { "FALSE" }.into(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_fields_count(handle: u64, out: *mut i32) -> i32 {
    if out.is_null() {
        return BN_LOG_INVALID_ARGUMENT;
    }
    with_fields(|fields| {
        let Some(fields) = fields.get(&handle) else {
            return BN_LOG_INVALID_HANDLE;
        };
        let Ok(count) = i32::try_from(fields.len()) else {
            return BN_LOG_LIMIT_EXCEEDED;
        };
        unsafe { out.write(count) };
        BN_LOG_OK
    })
}

fn add_transport(handle: u64, minimum: i64, kind: &str, path: Option<String>) -> i32 {
    let Ok(minimum) = parse_level(minimum) else {
        return BN_LOG_INVALID_ARGUMENT;
    };
    with_loggers(|loggers| {
        let Some(logger) = loggers.get_mut(&handle) else {
            return BN_LOG_INVALID_HANDLE;
        };
        if logger.closed {
            return BN_LOG_CLOSED;
        }
        if logger.null_transports.len()
            + logger.console_transports.len()
            + logger.file_transports.len()
            >= 8
        {
            return BN_LOG_LIMIT_EXCEEDED;
        }
        match kind {
            "null" => logger.null_transports.push(minimum),
            "console" if allows(POLICY_CONSOLE) => logger.console_transports.push(minimum),
            "file"
                if allows(POLICY_FILESYSTEM)
                    && path
                        .as_deref()
                        .is_some_and(|path| super::policy::allows_path(Path::new(path), true)) =>
            {
                logger.file_transports.push(FileTransport {
                    path: path.expect("file transport has path"),
                    minimum,
                });
            }
            "console" | "file" => return BN_LOG_POLICY_DENIED,
            _ => return BN_LOG_INVALID_ARGUMENT,
        }
        BN_LOG_OK
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_add_null(handle: u64, minimum: i64) -> i32 {
    add_transport(handle, minimum, "null", None)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_add_console(handle: u64, minimum: i64) -> i32 {
    add_transport(handle, minimum, "console", None)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_add_file(handle: u64, path: *const c_char, minimum: i64) -> i32 {
    let Some(path) = text(path) else {
        return BN_LOG_INVALID_ARGUMENT;
    };
    if path.is_empty() || path.len() > 4096 {
        return BN_LOG_INVALID_ARGUMENT;
    }
    add_transport(handle, minimum, "file", Some(path))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_child(handle: u64, fields: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_LOG_INVALID_ARGUMENT;
    }
    let Some(fields) = with_fields(|registry| registry.get(&fields).cloned()) else {
        return BN_LOG_INVALID_HANDLE;
    };
    with_loggers(|loggers| {
        let Some(parent) = loggers.get(&handle).cloned() else {
            return BN_LOG_INVALID_HANDLE;
        };
        if parent.closed {
            return BN_LOG_CLOSED;
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
    let (Ok(level), Some(message)) = (parse_level(level), text(message)) else {
        return BN_LOG_INVALID_ARGUMENT;
    };
    if message.len() > 16 * 1024 {
        return BN_LOG_LIMIT_EXCEEDED;
    }
    let Some(provided) = with_fields(|registry| registry.get(&fields).cloned()) else {
        return BN_LOG_INVALID_HANDLE;
    };
    let Some(logger) = with_loggers(|loggers| loggers.get(&handle).cloned()) else {
        return BN_LOG_INVALID_HANDLE;
    };
    if logger.closed {
        return BN_LOG_CLOSED;
    }
    let mut fields = logger.context;
    fields.extend(provided);
    let record = Record {
        timestamp: format!("{:?}", std::time::SystemTime::now()),
        label: logger.label,
        level,
        message,
        fields,
    };
    let Ok(line) = record.json_line() else {
        return BN_LOG_LIMIT_EXCEEDED;
    };
    if logger
        .console_transports
        .iter()
        .any(|minimum| level <= *minimum)
        && (super::libc_write_stdout(line.as_bytes()).is_err()
            || super::libc_write_stdout(b"\n").is_err())
    {
        return BN_LOG_TRANSPORT_ERROR;
    }
    for transport in logger
        .file_transports
        .iter()
        .filter(|transport| level <= transport.minimum)
    {
        let result = super::policy::open_path(
            Path::new(&transport.path),
            super::secure_fs::OpenMode::Append,
        )
        .and_then(|mut file| {
            file.write_all(line.as_bytes())?;
            file.write_all(b"\n")
        });
        if let Err(error) = result {
            return if error.kind() == std::io::ErrorKind::PermissionDenied {
                BN_LOG_POLICY_DENIED
            } else {
                BN_LOG_TRANSPORT_ERROR
            };
        }
    }
    BN_LOG_OK
}

fn flush_or_close(handle: u64, timeout_ms: i64, close: bool) -> i32 {
    if !(1..=60_000).contains(&timeout_ms) {
        return BN_LOG_INVALID_ARGUMENT;
    }
    with_loggers(|loggers| {
        let Some(logger) = loggers.get_mut(&handle) else {
            return BN_LOG_INVALID_HANDLE;
        };
        if logger.closed {
            return BN_LOG_CLOSED;
        }
        for transport in &logger.file_transports {
            let result = super::policy::open_path(
                Path::new(&transport.path),
                super::secure_fs::OpenMode::Append,
            )
            .and_then(|file| file.sync_all());
            if let Err(error) = result {
                return if error.kind() == std::io::ErrorKind::PermissionDenied {
                    BN_LOG_POLICY_DENIED
                } else {
                    BN_LOG_TRANSPORT_ERROR
                };
            }
        }
        if !logger.console_transports.is_empty() && super::libc_fflush().is_err() {
            return BN_LOG_TRANSPORT_ERROR;
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
            BN_LOG_INVALID_HANDLE
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_log_logger_delete(handle: u64) -> i32 {
    with_loggers(|loggers| {
        if loggers.remove(&handle).is_some() {
            BN_LOG_OK
        } else {
            BN_LOG_INVALID_HANDLE
        }
    })
}

#[cfg(test)]
#[path = "log_abi_tests.rs"]
mod tests;
