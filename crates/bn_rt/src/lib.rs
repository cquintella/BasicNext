#![allow(clippy::not_unsafe_ptr_arg_deref)] // C ABI exports validate nullable out-pointers.
#![allow(clippy::single_match_else)]
#![allow(clippy::cast_precision_loss)]
// Random conversion intentionally uses the 53-bit mantissa.
// C ABI branches keep success/error writes symmetric.

// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Compiled-program runtime for HOST providers.
//!
//! Interpreter and native `bn build` binaries share these implementations.
//! LLVM emits calls to the `bn_rt_*` C ABI; `bn run` uses the Rust API.

use std::{
    ffi::{CStr, c_char},
    io::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

mod civil;
mod console;
pub mod crypto;
mod crypto_abi;
mod dataframe;
mod dataframe_abi;
mod dispatch_abi;
mod error_abi;
mod exec;
pub mod file;
mod file_abi;
mod file_error;
pub mod json;
pub mod json_abi;
mod log;
mod log_abi;
mod math;
pub mod net;
mod net_abi;
mod policy;
pub mod secure_fs;
mod stats;
mod terminal;
mod text_abi;
mod trap_abi;

pub use log::{Level as LogLevel, Record as LogRecord};
pub use log_abi::*;
pub use stats::{Reduction, reduce};

pub use console::{ConsoleError, beep, cls, num_cols, num_rows, print_at};
pub use dataframe::{
    DataFrameColumn, DataFrameJoin, DataFrameJoinConfig, DataFrameResource, DataProvider,
    StandardDataProvider, add_dataframe_column, append_columns, append_rows, column_name,
    convert_dataframe_column, copy_dataframe_column, dataframe_reduce_column,
    duplicate_column_names, frame_from_csv_rows, get_dataframe_cell, join_dataframes, parse_csv,
    select_dataframe, set_column_label, slice_dataframe, transpose_dataframe, zscore_column,
};
pub use dataframe_abi::*;
pub use dispatch_abi::*;
pub use error_abi::{
    bn_rt_error_code, bn_rt_error_field, bn_rt_error_print, bn_rt_error_take, bn_rt_error_wrap,
};
pub(crate) use error_abi::{set_error, set_error_report};
pub use exec::*;
pub use file_abi::*;
pub use net::{
    Address as NetAddress, AddressesHandle, NeighborError, PingError, PingReply, ReverseError,
    join_resolver_tasks, neighbor, ping, reverse_timeout,
};
pub use net_abi::*;
pub use policy::{
    FsPolicy, POLICY_ALL, POLICY_CLOCK, POLICY_CONSOLE, POLICY_DISPATCH, POLICY_EXEC,
    POLICY_FILESYSTEM, POLICY_INVALID, POLICY_NET, POLICY_OK, POLICY_RANDOM, POLICY_VERSION,
    Policy, PolicyError, bn_rt_policy_check, bn_rt_policy_init, bn_rt_policy_restrict,
};
pub use terminal::terminal_dimensions;
pub use text_abi::*;
pub use trap_abi::*;

pub use civil::format_rfc3339;

/// Milliseconds since Unix epoch for an arbitrary `SystemTime`.
#[must_use]
pub fn timestamp_ms_from(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        Err(error) => i64::try_from(error.duration().as_millis()).map_or(i64::MIN, |value| -value),
    }
}

/// Milliseconds since Unix epoch for the current wall clock.
#[must_use]
pub fn timestamp_ms() -> i64 {
    timestamp_ms_from(SystemTime::now())
}

/// Nanoseconds since process start (saturating at `i64::MAX`).
#[must_use]
pub fn monotonic_ns() -> i64 {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let origin = *ORIGIN.get_or_init(Instant::now);
    i64::try_from(origin.elapsed().as_nanos()).unwrap_or(i64::MAX)
}

fn fail(code: &str, message: &str) {
    eprintln!("{}", format_failure(code, message));
}

fn format_failure(code: &str, message: &str) -> String {
    format!("error[{code}]: {message}")
}

fn emit_console_error(error: &ConsoleError) {
    fail(error.code(), &error.message());
}

struct LibcStdout;

impl Write for LibcStdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        libc_write_stdout(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        libc_fflush()
    }
}

#[allow(unsafe_code)] // C ABI: write(1) after flushing libc stdout used by LLVM printf.
fn libc_write_stdout(buf: &[u8]) -> io::Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    libc_fflush()?;
    let mut written = 0;
    while written < buf.len() {
        // Unix: STDOUT_FILENO + size_t count. Windows CRT: fd 1 + c_uint count.
        #[cfg(unix)]
        let next = unsafe {
            libc::write(
                libc::STDOUT_FILENO,
                buf[written..].as_ptr().cast(),
                buf.len() - written,
            )
        };
        #[cfg(windows)]
        let next = unsafe {
            let remaining = buf.len() - written;
            let chunk = u32::try_from(remaining).unwrap_or(u32::MAX);
            libc::write(1, buf[written..].as_ptr().cast(), chunk)
        };
        #[cfg(not(any(unix, windows)))]
        compile_error!("libc_write_stdout requires unix or windows");
        if next <= 0 {
            return Err(if next < 0 {
                io::Error::last_os_error()
            } else {
                io::Error::other("write to stdout returned 0")
            });
        }
        written += usize::try_from(next).unwrap_or(0);
    }
    Ok(())
}

#[allow(unsafe_code)] // C ABI: flush libc stdout so PRINT and Console share order.
fn libc_fflush() -> io::Result<()> {
    // A null stream flushes all open output streams and avoids platform-specific
    // `stdout` symbols (`__stdoutp` on Darwin, unavailable on some libc targets).
    let rc = unsafe { libc::fflush(std::ptr::null_mut()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[allow(unsafe_code)] // C ABI: read a NUL-terminated LLVM string pointer.
fn c_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Clock.Now.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_clock_now() -> i64 {
    if !policy::allows(policy::POLICY_CLOCK) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Clock is denied by execution policy",
        );
        return -1;
    }
    timestamp_ms()
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Clock.Timer.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_clock_timer() -> i64 {
    if !policy::allows(policy::POLICY_CLOCK) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Clock is denied by execution policy",
        );
        return -1;
    }
    monotonic_ns()
}

static COMPILED_RANDOM_STATE: AtomicU64 = AtomicU64::new(1);

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Random.Seed.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_random_seed(seed: i64) -> i32 {
    if !policy::allows(policy::POLICY_RANDOM) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Random is denied by execution policy",
        );
        return 2;
    }
    COMPILED_RANDOM_STATE.store(seed.cast_unsigned().max(1), Ordering::Relaxed);
    0
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Random.Random.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_random_next() -> f64 {
    if !policy::allows(policy::POLICY_RANDOM) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Random is denied by execution policy",
        );
        return f64::NAN;
    }
    let mut current = COMPILED_RANDOM_STATE.load(Ordering::Relaxed);
    loop {
        let mut next = current;
        next ^= next >> 12;
        next ^= next << 25;
        next ^= next >> 27;
        next = next.wrapping_mul(0x2545_F491_4F6C_DD1D);
        match COMPILED_RANDOM_STATE.compare_exchange_weak(
            current,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return (next >> 11) as f64 / 9_007_199_254_740_992.0,
            Err(observed) => current = observed,
        }
    }
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.Cls.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_cls() -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Console is denied by execution policy",
        );
        return 2;
    }
    match cls(&mut LibcStdout) {
        Ok(()) => 0,
        Err(error) => {
            emit_console_error(&error);
            1
        }
    }
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.Beep.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_beep() -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Console is denied by execution policy",
        );
        return 2;
    }
    match beep(&mut LibcStdout) {
        Ok(()) => 0,
        Err(error) => {
            emit_console_error(&error);
            1
        }
    }
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.PrintAt.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_print_at(column: i32, row: i32, text: *const c_char) -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Console is denied by execution policy",
        );
        return 2;
    }
    let Some(text) = c_str(text) else {
        fail("TYPE_MISMATCH", "PrintAt expects STRING");
        return 1;
    };
    match print_at(&mut LibcStdout, i128::from(column), i128::from(row), text) {
        Ok(()) => 0,
        Err(error) => {
            emit_console_error(&error);
            1
        }
    }
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.NumCols.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_num_cols() -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Console is denied by execution policy",
        );
        return -2;
    }
    match num_cols() {
        Ok(value) => value,
        Err(error) => {
            emit_console_error(&error);
            -1
        }
    }
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.ABS on integers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_iabs(value: i64) -> i64 {
    math::iabs(value)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.SIGN on integers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_isign(value: i64) -> i64 {
    math::isign(value)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.MIN on integers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_imin(left: i64, right: i64) -> i64 {
    left.min(right)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.MAX on integers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_imax(left: i64, right: i64) -> i64 {
    left.max(right)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.TOHOUR.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_tohour(milliseconds: i64) -> i32 {
    math::tohour(milliseconds)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.TOWEEKDAY.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_toweekday(milliseconds: i64) -> i32 {
    math::toweekday(milliseconds)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.VAL.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_val(text: *const c_char) -> f64 {
    c_str(text).map_or(0.0, math::parse_val)
}

macro_rules! unary_f64 {
    ($export:ident, $body:expr) => {
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn $export(value: f64) -> f64 {
            $body(value)
        }
    };
}

macro_rules! binary_f64 {
    ($export:ident, $body:expr) => {
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn $export(left: f64, right: f64) -> f64 {
            $body(left, right)
        }
    };
}

unary_f64!(bn_rt_math_fabs, f64::abs);
unary_f64!(bn_rt_math_fsign, math::fsign);
unary_f64!(bn_rt_math_floor, f64::floor);
unary_f64!(bn_rt_math_ceil, f64::ceil);
unary_f64!(bn_rt_math_trunc, f64::trunc);
unary_f64!(bn_rt_math_exp, f64::exp);
unary_f64!(bn_rt_math_log, f64::ln);
unary_f64!(bn_rt_math_log10, f64::log10);
unary_f64!(bn_rt_math_log2, f64::log2);
unary_f64!(bn_rt_math_sin, f64::sin);
unary_f64!(bn_rt_math_cos, f64::cos);
unary_f64!(bn_rt_math_tan, f64::tan);
unary_f64!(bn_rt_math_asin, f64::asin);
unary_f64!(bn_rt_math_acos, f64::acos);
unary_f64!(bn_rt_math_atan, f64::atan);
unary_f64!(bn_rt_math_sqrt, f64::sqrt);

binary_f64!(bn_rt_math_pow, f64::powf);
binary_f64!(bn_rt_math_atan2, f64::atan2);
binary_f64!(bn_rt_math_hypot, f64::hypot);
binary_f64!(bn_rt_math_fmin, math::fmin);
binary_f64!(bn_rt_math_fmax, math::fmax);
binary_f64!(bn_rt_math_round, math::round_ties_even);

#[allow(unsafe_code)] // C ABI export for LLVM-emitted BNMath.FMA.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_fma(x: f64, y: f64, z: f64) -> f64 {
    x.mul_add(y, z)
}

#[allow(unsafe_code)] // C ABI: INTEGER[] MIN.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmin_i32(ptr: *const i32, len: i32) -> i32 {
    stats::vmin_i32(stats::i32_slice(ptr, len))
}

#[allow(unsafe_code)] // C ABI: FLOAT[] MIN.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmin_f64(ptr: *const f64, len: i32) -> f64 {
    float_slice(ptr, len)
        .iter()
        .copied()
        .reduce(|left, right| {
            if left.is_nan() || right.is_nan() {
                f64::NAN
            } else {
                left.min(right)
            }
        })
        .unwrap_or_else(|| {
            math::fail(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
            )
        })
}

#[allow(unsafe_code)] // C ABI: INTEGER[] MAX.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmax_i32(ptr: *const i32, len: i32) -> i32 {
    stats::vmax_i32(stats::i32_slice(ptr, len))
}

#[allow(unsafe_code)] // C ABI: FLOAT[] MAX.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmax_f64(ptr: *const f64, len: i32) -> f64 {
    float_slice(ptr, len)
        .iter()
        .copied()
        .reduce(|left, right| {
            if left.is_nan() || right.is_nan() {
                f64::NAN
            } else {
                left.max(right)
            }
        })
        .unwrap_or_else(|| {
            math::fail(
                "INDEX_OUT_OF_BOUNDS",
                "BNMath reduction received an empty vector",
            )
        })
}

fn reduce_i32(name: &str, ptr: *const i32, len: i32) -> f64 {
    match stats::reduce(name, stats::i32_slice(ptr, len)) {
        stats::Reduction::Float(value) => value,
        stats::Reduction::Na => f64::NAN,
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_mean_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("MEAN", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_median_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("MEDIAN", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_quartile1_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("QUARTILE1", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_quartile3_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("QUARTILE3", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_range_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("RANGE", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_stdev_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("STDEV", ptr, len)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_variance_i32(ptr: *const i32, len: i32) -> f64 {
    reduce_i32("VARIANCE", ptr, len)
}

#[allow(unsafe_code)] // C ABI: MODE writes *out and returns 1 for NA.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_mode_i32(ptr: *const i32, len: i32, out: *mut f64) -> i32 {
    match stats::reduce("MODE", stats::i32_slice(ptr, len)) {
        stats::Reduction::Na => 1,
        stats::Reduction::Float(value) => {
            if !out.is_null() {
                unsafe { out.write(value) };
            }
            0
        }
    }
}

fn reduce_f64(name: &str, ptr: *const f64, len: i32) -> stats::Reduction {
    stats::reduce_f64(name, float_slice(ptr, len))
}

#[allow(unsafe_code)] // C ABI: FLOAT[] buffer from LLVM alloca or interpreter adapter.
fn float_slice<'a>(ptr: *const f64, len: i32) -> &'a [f64] {
    if ptr.is_null() || len <= 0 {
        return &[];
    }
    unsafe { std::slice::from_raw_parts(ptr, usize::try_from(len).unwrap_or(0)) }
}

macro_rules! float_reduction {
    ($export:ident, $name:literal) => {
        #[allow(unsafe_code)]
        #[unsafe(no_mangle)]
        pub extern "C" fn $export(ptr: *const f64, len: i32) -> f64 {
            match reduce_f64($name, ptr, len) {
                stats::Reduction::Float(value) => value,
                stats::Reduction::Na => f64::NAN,
            }
        }
    };
}

float_reduction!(bn_rt_math_mean_f64, "MEAN");
float_reduction!(bn_rt_math_median_f64, "MEDIAN");
float_reduction!(bn_rt_math_quartile1_f64, "QUARTILE1");
float_reduction!(bn_rt_math_quartile3_f64, "QUARTILE3");
float_reduction!(bn_rt_math_range_f64, "RANGE");
float_reduction!(bn_rt_math_stdev_f64, "STDEV");
float_reduction!(bn_rt_math_variance_f64, "VARIANCE");

#[allow(unsafe_code)] // C ABI: MODE writes *out and returns 1 for NA.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_mode_f64(ptr: *const f64, len: i32, out: *mut f64) -> i32 {
    match reduce_f64("MODE", ptr, len) {
        stats::Reduction::Na => 1,
        stats::Reduction::Float(value) => {
            if !out.is_null() {
                unsafe { out.write(value) };
            }
            0
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_todate(timestamp: i64) -> i32 {
    civil::todate(timestamp)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_totime(timestamp: i64) -> i32 {
    civil::totime(timestamp)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_totimestamp(days: i32, millis: i32) -> i64 {
    civil::totimestamp(days, millis)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_date(days: i32) {
    let _ = LibcStdout.write_all(civil::format_date(days).as_bytes());
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_time(millis: i32) {
    let _ = LibcStdout.write_all(civil::format_time(millis).as_bytes());
}

#[allow(unsafe_code)] // C ABI: STRING equality.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_eq(left: *const c_char, right: *const c_char) -> i32 {
    match (c_str(left), c_str(right)) {
        (Some(left), Some(right)) => i32::from(left == right),
        _ => 0,
    }
}

#[allow(unsafe_code)] // C ABI: LEN of a UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_len(text: *const c_char) -> i32 {
    let Some(text) = c_str(text) else {
        return 0;
    };
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

#[allow(unsafe_code)] // C ABI: decode the first Unicode scalar from a borrowed STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_asc(text: *const c_char) -> i64 {
    c_str(text)
        .and_then(|text| text.chars().next())
        .map_or(-1, |character| i64::from(u32::from(character)))
}

/// Unicode lowercase (Rust `str::to_lowercase` — full Unicode case mapping,
/// not ASCII-only). Returns a freshly allocated NUL-terminated UTF-8 string.
/// Caller frees with the same allocator used for other owned rt strings
/// (`bn_rt_file_string_free` / libc free of the `c_string` allocation).
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_to_lower(text: *const c_char) -> *mut c_char {
    let Some(text) = c_str(text) else {
        return c_string("");
    };
    c_string(&text.to_lowercase())
}

/// Unicode uppercase (Rust `str::to_uppercase` — full Unicode case mapping,
/// not ASCII-only). May change scalar length (e.g. `ß` → `SS`).
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_to_upper(text: *const c_char) -> *mut c_char {
    let Some(text) = c_str(text) else {
        return c_string("");
    };
    c_string(&text.to_uppercase())
}

fn pack_utf8(character: char) -> u64 {
    let mut encoded = [0_u8; 4];
    let text = character.encode_utf8(&mut encoded);
    let mut bytes = [0_u8; size_of::<u64>()];
    bytes[..text.len()].copy_from_slice(text.as_bytes());
    u64::from_ne_bytes(bytes)
}

/// Returns one UTF-8 scalar packed in native byte order, including a trailing
/// NUL byte. `u64::MAX` represents an invalid Unicode scalar.
#[allow(unsafe_code)] // C ABI export for LLVM-emitted CHAR.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_char_utf8(code: i64) -> u64 {
    let Some(character) = u32::try_from(code).ok().and_then(char::from_u32) else {
        return u64::MAX;
    };
    pack_utf8(character)
}

#[allow(unsafe_code)] // C ABI: the column name is a borrowed NUL-terminated STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_add_integer_start(
    frame: u64,
    name: *const c_char,
    length: u32,
) -> i32 {
    let Some(name) = c_str(name) else {
        return -1;
    };
    dataframe_abi::add_integer_column_storage(frame, name.to_owned(), length)
        .and_then(|index| {
            i32::try_from(index).map_err(|_| dataframe_abi::BN_DATAFRAME_CONTRACT_ERROR)
        })
        .unwrap_or_else(|status| -i32::try_from(status).unwrap_or(1))
}

#[allow(unsafe_code)] // C ABI export; all arguments are scalar values.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_set_integer_cell(
    frame: u64,
    column: i32,
    row: u32,
    value: i64,
) -> u32 {
    let Ok(column) = u32::try_from(column) else {
        return dataframe_abi::BN_DATAFRAME_INVALID_ARGUMENT;
    };
    dataframe_abi::set_integer_cell_storage(frame, column, row, value)
}

#[allow(unsafe_code)] // C ABI export; returned storage is released by the LLVM caller.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_dataframe_column_name_owned(frame: u64, index: u32) -> *mut c_char {
    dataframe_abi::column_name_storage(frame, index)
        .map_or(std::ptr::null_mut(), |name| c_string(&name))
}

#[allow(unsafe_code)] // C ABI: STRING[index] as a freshly allocated 1-char string.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_index(text: *const c_char, index: i32) -> *mut c_char {
    let Some(text) = c_str(text) else {
        fail("INDEX_OUT_OF_BOUNDS", "index 0 is outside string length 0");
        std::process::exit(1);
    };
    let Ok(index_usize) = usize::try_from(index) else {
        fail("INDEX_OUT_OF_BOUNDS", "index cannot be negative");
        std::process::exit(1);
    };
    let len = text.chars().count();
    let Some(ch) = text.chars().nth(index_usize) else {
        fail(
            "INDEX_OUT_OF_BOUNDS",
            &format!("index {index_usize} is outside string length {len}"),
        );
        std::process::exit(1);
    };
    let mut bytes = ch.to_string().into_bytes();
    bytes.push(0);
    let mut boxed = bytes.into_boxed_slice();
    let ptr = boxed.as_mut_ptr().cast::<c_char>();
    std::mem::forget(boxed);
    ptr
}

/// Returns `STRING[index]` as one NUL-terminated UTF-8 scalar packed in native
/// byte order. This ABI lets the LLVM caller materialize function-local
/// storage, so indexing does not create heap ownership.
#[allow(unsafe_code)] // C ABI: STRING[index] from a borrowed UTF-8 string.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_index_utf8(text: *const c_char, index: i32) -> u64 {
    let Some(text) = c_str(text) else {
        fail("INDEX_OUT_OF_BOUNDS", "index 0 is outside string length 0");
        std::process::exit(1);
    };
    let Ok(index_usize) = usize::try_from(index) else {
        fail("INDEX_OUT_OF_BOUNDS", "index cannot be negative");
        std::process::exit(1);
    };
    let len = text.chars().count();
    let Some(character) = text.chars().nth(index_usize) else {
        fail(
            "INDEX_OUT_OF_BOUNDS",
            &format!("index {index_usize} is outside string length {len}"),
        );
        std::process::exit(1);
    };
    pack_utf8(character)
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.NumRows.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_num_rows() -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Console is denied by execution policy",
        );
        return -2;
    }
    match num_rows() {
        Ok(value) => value,
        Err(error) => {
            emit_console_error(&error);
            -1
        }
    }
}

#[allow(unsafe_code)] // C ABI: allocate a NUL-terminated copy for LLVM strings.
fn c_string(text: &str) -> *mut c_char {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    let mut boxed = bytes.into_boxed_slice();
    let ptr = boxed.as_mut_ptr().cast::<c_char>();
    std::mem::forget(boxed);
    ptr
}

#[cfg(test)]
pub(crate) fn network_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        // One environment-level bind failure must not turn every later,
        // independent network test into a mutex-poison cascade.
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::{POLICY_ALL, POLICY_CONSOLE, POLICY_NET, POLICY_OK, POLICY_VERSION};
    use super::{bn_rt_net_addresses_count, bn_rt_net_addresses_free, bn_rt_net_resolve};
    use super::{
        bn_rt_net_buffer_free, bn_rt_net_handle_close, bn_rt_net_string_free, bn_rt_net_udp_bind,
        bn_rt_net_udp_local_endpoint, bn_rt_net_udp_receive, bn_rt_net_udp_send_to,
    };
    use super::{
        bn_rt_net_tcp_accept, bn_rt_net_tcp_connect, bn_rt_net_tcp_listen_with_backlog,
        bn_rt_net_tcp_read, bn_rt_net_tcp_write,
    };
    use super::{monotonic_ns, timestamp_ms, timestamp_ms_from};
    use std::ffi::CString;
    use std::io::{Read, Write};
    use std::net::TcpStream as StdTcpStream;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]

    fn packed_string_index_preserves_ascii_and_multibyte_scalars() {
        let text = CString::new("Aé界").expect("literal has no NUL");
        for (index, expected) in ["A", "é", "界"].into_iter().enumerate() {
            let packed = super::bn_rt_str_index_utf8(
                text.as_ptr(),
                i32::try_from(index).expect("small index"),
            );
            let bytes = packed.to_ne_bytes();
            let nul = bytes.iter().position(|byte| *byte == 0).expect("NUL");
            assert_eq!(std::str::from_utf8(&bytes[..nul]), Ok(expected));
        }
    }

    #[test]
    fn packed_char_uses_the_same_native_byte_contract_as_string_index() {
        let text = CString::new("é").expect("literal has no NUL");
        assert_eq!(
            super::bn_rt_str_char_utf8(i64::from(u32::from('é'))),
            super::bn_rt_str_index_utf8(text.as_ptr(), 0)
        );
    }

    #[test]
    fn console_c_abi_rechecks_execution_policy_at_call_boundary() {
        let _policy = super::policy::reset_for_tests();
        assert_eq!(
            super::bn_rt_policy_init(POLICY_VERSION, POLICY_ALL),
            POLICY_OK
        );
        assert_eq!(
            super::bn_rt_policy_restrict(POLICY_ALL & !(POLICY_CONSOLE | POLICY_NET)),
            POLICY_OK
        );
        assert_eq!(super::bn_rt_console_beep(), 2);
        assert_eq!(super::bn_rt_net_addresses_count(std::ptr::null()), -2);
        assert_eq!(
            super::bn_rt_net_addresses_get(std::ptr::null(), 0, std::ptr::null_mut()),
            2
        );
    }

    #[test]
    fn timestamp_before_epoch_is_negative() {
        assert_eq!(timestamp_ms_from(UNIX_EPOCH - Duration::from_millis(1)), -1);
    }

    #[test]
    fn system_clocks_are_non_negative() {
        assert!(timestamp_ms() >= 0);
        let first = monotonic_ns();
        let second = monotonic_ns();
        assert!(first >= 0);
        assert!(second >= first);
    }

    #[test]
    fn resolve_c_abi_returns_bounded_handle() {
        let _lock = super::network_test_lock();
        let host = CString::new("localhost").expect("literal has no NUL");
        let mut handle = std::ptr::null_mut();
        let status = bn_rt_net_resolve(host.as_ptr(), 1_000, &raw mut handle);
        assert_eq!(status, 0);
        assert!(!handle.is_null());
        assert!((0..=64).contains(&bn_rt_net_addresses_count(handle)));
        bn_rt_net_addresses_free(handle);
    }

    #[test]
    fn udp_bind_c_abi_allocates_and_closes_handle() {
        let _lock = super::network_test_lock();
        let address = CString::new("127.0.0.1").expect("literal has no NUL");
        let mut handle = -1;
        assert_eq!(bn_rt_net_udp_bind(address.as_ptr(), 0, &raw mut handle), 0);
        assert!(handle >= 0);
        assert_eq!(bn_rt_net_handle_close(handle), 0);
        // Close is idempotent (host-net.md): a second Close is not an error.
        assert_eq!(bn_rt_net_handle_close(handle), 0);
    }

    #[test]
    fn udp_local_endpoint_c_abi_returns_bound_port() {
        let _lock = super::network_test_lock();
        let address = CString::new("127.0.0.1").expect("literal has no NUL");
        let mut handle = -1;
        assert_eq!(bn_rt_net_udp_bind(address.as_ptr(), 0, &raw mut handle), 0);
        let mut rendered = std::ptr::null_mut();
        let mut port = -1;
        assert_eq!(
            bn_rt_net_udp_local_endpoint(handle, &raw mut rendered, &raw mut port),
            0
        );
        assert!(!rendered.is_null());
        assert!(port > 0);
        assert_eq!(bn_rt_net_handle_close(handle), 0);
    }

    #[test]
    fn udp_receive_rejects_invalid_handle_and_bounds() {
        let _lock = super::network_test_lock();
        let mut data = std::ptr::null_mut();
        let mut length = -1;
        assert_eq!(
            bn_rt_net_udp_receive(
                -1,
                65_507,
                1_000,
                &raw mut data,
                &raw mut length,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ),
            1
        );
        assert!(data.is_null());
        assert_eq!(length, -1);
    }

    #[test]
    #[allow(unsafe_code)]
    fn udp_c_abi_round_trip_preserves_payload_and_source() {
        let _lock = super::network_test_lock();
        let address = CString::new("127.0.0.1").expect("literal has no NUL");
        let mut sender = -1;
        let mut receiver = -1;
        assert_eq!(bn_rt_net_udp_bind(address.as_ptr(), 0, &raw mut sender), 0);
        assert_eq!(
            bn_rt_net_udp_bind(address.as_ptr(), 0, &raw mut receiver),
            0
        );

        let mut receiver_address = std::ptr::null_mut();
        let mut receiver_port = -1;
        assert_eq!(
            bn_rt_net_udp_local_endpoint(
                receiver,
                &raw mut receiver_address,
                &raw mut receiver_port,
            ),
            0
        );
        let payload = b"ping";
        let mut written = -1;
        assert_eq!(
            bn_rt_net_udp_send_to(
                sender,
                receiver_address.cast(),
                receiver_port,
                payload.as_ptr(),
                i32::try_from(payload.len()).expect("small payload"),
                &raw mut written,
            ),
            0
        );
        assert_eq!(written, 4);

        let mut data = std::ptr::null_mut();
        let mut length = -1;
        let mut source = std::ptr::null_mut();
        let mut source_port = -1;
        let mut truncated = -1;
        assert_eq!(
            bn_rt_net_udp_receive(
                receiver,
                1024,
                1_000,
                &raw mut data,
                &raw mut length,
                &raw mut source,
                &raw mut source_port,
                &raw mut truncated,
            ),
            0
        );
        let payload_out = unsafe {
            std::slice::from_raw_parts(data, usize::try_from(length).expect("non-negative length"))
        };
        assert_eq!(payload_out, payload);
        assert!(source_port > 0);
        assert_eq!(truncated, 0);
        bn_rt_net_buffer_free(data, length);
        bn_rt_net_string_free(receiver_address.cast());
        bn_rt_net_string_free(source);
        assert_eq!(bn_rt_net_handle_close(sender), 0);
        assert_eq!(bn_rt_net_handle_close(receiver), 0);
    }

    #[test]
    #[allow(unsafe_code)]
    fn tcp_c_abi_round_trip_preserves_payload() {
        let _lock = super::network_test_lock();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let port = listener.local_addr().expect("listener address").port();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept client");
            let mut input = [0u8; 4];
            stream.read_exact(&mut input).expect("read request");
            assert_eq!(&input, b"ping");
            stream.write_all(b"pong").expect("write response");
        });
        let address = CString::new("127.0.0.1").expect("literal has no NUL");
        let mut handle = -1;
        assert_eq!(
            bn_rt_net_tcp_connect(address.as_ptr(), i32::from(port), 1_000, &raw mut handle),
            0
        );
        let mut written = -1;
        assert_eq!(
            bn_rt_net_tcp_write(handle, b"ping".as_ptr(), 4, &raw mut written),
            0
        );
        assert_eq!(written, 4);
        let mut output = [0u8; 4];
        let mut read = -1;
        assert_eq!(
            bn_rt_net_tcp_read(handle, output.as_mut_ptr(), 4, &raw mut read),
            0
        );
        assert_eq!(read, 4);
        assert_eq!(&output, b"pong");
        assert_eq!(bn_rt_net_handle_close(handle), 0);
        worker.join().expect("worker completed");
    }

    #[test]
    #[allow(unsafe_code)]
    fn tcp_listener_c_abi_accepts_connection() {
        let _lock = super::network_test_lock();
        let address = CString::new("127.0.0.1").expect("literal has no NUL");
        let mut listener = -1;
        assert_eq!(
            bn_rt_net_tcp_listen_with_backlog(address.as_ptr(), 0, 8, &raw mut listener),
            0
        );
        let mut rendered = std::ptr::null_mut();
        let mut port = -1;
        assert_eq!(
            super::bn_rt_net_tcp_listener_local_endpoint(
                listener,
                &raw mut rendered,
                &raw mut port
            ),
            0
        );
        let worker = std::thread::spawn(move || {
            StdTcpStream::connect(("127.0.0.1", u16::try_from(port).expect("valid port")))
                .expect("connect listener")
        });
        let mut accepted = -1;
        assert_eq!(bn_rt_net_tcp_accept(listener, 1_000, &raw mut accepted), 0);
        assert!(accepted >= 0);
        worker.join().expect("client thread");
        bn_rt_net_string_free(rendered.cast());
        assert_eq!(bn_rt_net_handle_close(accepted), 0);
        assert_eq!(bn_rt_net_handle_close(listener), 0);
    }

    #[test]
    #[allow(unsafe_code)]
    fn tcp_read_c_abi_reports_eof_as_zero_bytes() {
        let _lock = super::network_test_lock();
        let address = CString::new("127.0.0.1").expect("literal has no NUL");
        let mut listener = -1;
        assert_eq!(
            bn_rt_net_tcp_listen_with_backlog(address.as_ptr(), 0, 8, &raw mut listener),
            0
        );
        let mut rendered = std::ptr::null_mut();
        let mut port = -1;
        assert_eq!(
            super::bn_rt_net_tcp_listener_local_endpoint(
                listener,
                &raw mut rendered,
                &raw mut port
            ),
            0
        );
        let worker = std::thread::spawn(move || {
            let stream =
                StdTcpStream::connect(("127.0.0.1", u16::try_from(port).expect("valid port")))
                    .expect("connect listener");
            stream
                .shutdown(std::net::Shutdown::Both)
                .expect("shutdown client");
        });
        let mut accepted = -1;
        assert_eq!(bn_rt_net_tcp_accept(listener, 1_000, &raw mut accepted), 0);
        worker.join().expect("client");
        let mut buffer = [0_u8; 1];
        let mut read = -1;
        assert_eq!(
            bn_rt_net_tcp_read(accepted, buffer.as_mut_ptr(), 1, &raw mut read),
            0
        );
        assert_eq!(read, 0);
        bn_rt_net_string_free(rendered.cast());
        assert_eq!(bn_rt_net_handle_close(accepted), 0);
        assert_eq!(bn_rt_net_handle_close(listener), 0);
    }

    #[test]
    fn clock_functions_respect_policy_clock() {
        let _policy = super::policy::reset_for_tests();
        assert!(super::bn_rt_clock_now() > 0);
        assert!(super::bn_rt_clock_timer() >= 0);

        // Restrict policy to exclude POLICY_CLOCK
        super::policy::bn_rt_policy_restrict(super::policy::POLICY_CONSOLE);
        assert_eq!(super::bn_rt_clock_now(), -1);
        assert_eq!(super::bn_rt_clock_timer(), -1);
    }

    #[test]
    fn native_failure_bridge_has_stable_machine_prefix() {
        assert_eq!(
            super::format_failure("INVALID_JSON", "bad token"),
            "error[INVALID_JSON]: bad token"
        );
    }
}
