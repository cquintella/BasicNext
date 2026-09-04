#![allow(clippy::not_unsafe_ptr_arg_deref)] // C ABI exports validate nullable out-pointers.
#![allow(clippy::single_match_else)]
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
    time::{Instant, SystemTime, UNIX_EPOCH},
};

mod civil;
mod console;
mod math;
mod net;
mod stats;
mod terminal;

pub use console::{ConsoleError, beep, cls, num_cols, num_rows, print_at};
pub use net::{
    Address as NetAddress, AddressesHandle, NeighborError, PingError, PingReply, ReverseError,
    join_resolver_tasks, neighbor, ping, reverse_timeout,
};
pub use terminal::terminal_dimensions;

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
    eprintln!("error[{code}]: {message}");
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
        let next = unsafe {
            libc::write(
                libc::STDOUT_FILENO,
                buf[written..].as_ptr().cast(),
                buf.len() - written,
            )
        };
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
    timestamp_ms()
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Clock.Timer.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_clock_timer() -> i64 {
    monotonic_ns()
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.Cls.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_cls() -> i32 {
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

#[allow(unsafe_code)] // C ABI: INTEGER[] MAX.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_math_vmax_i32(ptr: *const i32, len: i32) -> i32 {
    stats::vmax_i32(stats::i32_slice(ptr, len))
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

fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "NAN".into();
    }
    if value == f64::INFINITY {
        return "INF".into();
    }
    if value == f64::NEG_INFINITY {
        return "-INF".into();
    }
    let mut text = value.to_string();
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    text
}

#[allow(unsafe_code)] // C ABI: PRINT FLOAT with interpreter formatting.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_float(value: f64) {
    let text = format_float(value);
    let _ = LibcStdout.write_all(text.as_bytes());
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

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.NumRows.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_num_rows() -> i32 {
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

/// Parses an IP address. Writes a malloc'd IP or error message to `out`.
///
/// Returns 0 on success and 1 on error.
#[allow(unsafe_code)] // C ABI for HOST.Net.Address.Parse.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_address_parse(text: *const c_char, out: *mut *mut c_char) -> i32 {
    let Some(text) = c_str(text) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
        }
        return 1;
    };
    match net::Address::parse(text) {
        Ok(address) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&address.to_string());
                }
            }
            0
        }
        Err(_) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string("invalid IP address");
                }
            }
            1
        }
    }
}

/// ICMP Echo. On success `out` is the reply address and `out_rtt` the RTT in µs.
///
/// Returns 0 on success and 1 on error (`out` then holds the message).
#[allow(unsafe_code)] // C ABI for HOST.Net.Ping.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_ping(
    address: *const c_char,
    timeout_ms: i32,
    out: *mut *mut c_char,
    out_rtt: *mut i64,
) -> i32 {
    let Some(text) = c_str(address) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
            if !out_rtt.is_null() {
                *out_rtt = 0;
            }
        }
        return 1;
    };
    let Ok(parsed) = net::Address::parse(text) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
            if !out_rtt.is_null() {
                *out_rtt = 0;
            }
        }
        return 1;
    };
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    match ping(parsed, timeout) {
        Ok(reply) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&reply.address.to_string());
                }
                if !out_rtt.is_null() {
                    *out_rtt = reply.round_trip_microseconds;
                }
            }
            0
        }
        Err(error) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&error.message());
                }
                if !out_rtt.is_null() {
                    *out_rtt = 0;
                }
            }
            1
        }
    }
}

/// Reverse DNS. Writes the host name or error message to `out`.
///
/// Returns 0 on success and 1 on error.
#[allow(unsafe_code)] // C ABI for HOST.Net.Reverse.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_reverse(
    address: *const c_char,
    timeout_ms: i32,
    out: *mut *mut c_char,
) -> i32 {
    let Some(text) = c_str(address) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
        }
        return 1;
    };
    let Ok(parsed) = net::Address::parse(text) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
        }
        return 1;
    };
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    match reverse_timeout(parsed, timeout) {
        Ok(name) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&name);
                }
            }
            0
        }
        Err(error) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&error.message());
                }
            }
            1
        }
    }
}

/// Neighbor lookup. Writes the neighbor address or error message to `out`.
///
/// Returns 0 on success and 1 on error.
#[allow(unsafe_code)] // C ABI for HOST.Net.Neighbor.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_neighbor(address: *const c_char, out: *mut *mut c_char) -> i32 {
    let Some(text) = c_str(address) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
        }
        return 1;
    };
    let Ok(parsed) = net::Address::parse(text) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid IP address");
            }
        }
        return 1;
    };
    match neighbor(parsed) {
        Ok(found) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&found.to_string());
                }
            }
            0
        }
        Err(error) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&error.message());
                }
            }
            1
        }
    }
}

/// Forward DNS resolution. `out` receives an opaque `AddressesHandle` on success
/// or an allocated diagnostic string on failure. Returns 0 on success.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_resolve(
    host: *const c_char,
    timeout_ms: i32,
    out: *mut *mut std::ffi::c_void,
) -> i32 {
    let Some(host) = c_str(host) else {
        unsafe {
            if !out.is_null() {
                *out = c_string("invalid host").cast();
            }
        }
        return 1;
    };
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    match AddressesHandle::resolve_timeout(host, 0, 64, timeout) {
        Ok(Some(handle)) => {
            let pointer = Box::into_raw(Box::new(handle)).cast::<std::ffi::c_void>();
            unsafe {
                if !out.is_null() {
                    *out = pointer;
                }
            }
            0
        }
        Ok(None) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string("resolver timeout").cast();
                }
            }
            1
        }
        Err(error) => {
            unsafe {
                if !out.is_null() {
                    *out = c_string(&error.to_string()).cast();
                }
            }
            1
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_addresses_count(handle: *const std::ffi::c_void) -> i32 {
    if handle.is_null() {
        return -1;
    }
    let handle = unsafe { &*handle.cast::<AddressesHandle>() };
    i32::try_from(handle.len()).unwrap_or(i32::MAX)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_addresses_get(
    handle: *const std::ffi::c_void,
    index: i32,
    out: *mut *mut c_char,
) -> i32 {
    if handle.is_null() || index < 0 {
        return 1;
    }
    let handle = unsafe { &*handle.cast::<AddressesHandle>() };
    let Ok(index) = usize::try_from(index) else {
        return 1;
    };
    let Some(address) = handle.get(index) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = c_string(&address.to_string());
        }
    }
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_addresses_free(handle: *mut std::ffi::c_void) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle.cast::<AddressesHandle>()));
        }
    }
}

/// Binds an UDP socket and returns its opaque runtime handle in `out`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_bind(address: *const c_char, port: i32, out: *mut i64) -> i32 {
    let Some(address) = c_str(address) else {
        return 1;
    };
    let Ok(address) = net::Address::parse(address) else {
        return 1;
    };
    let Ok(port) = u16::try_from(port) else {
        return 1;
    };
    let Ok(socket) = net::UdpSocket::bind(net::Endpoint::new(address, port)) else {
        return 1;
    };
    let Ok(handle) = net::handles::insert(net::handles::Handle::UdpSocket(socket)) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = i64::try_from(handle).unwrap_or(i64::MAX);
        }
    }
    0
}

/// Closes an opaque network handle. Returns 0 when a live handle was removed.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_handle_close(handle: i64) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    match net::handles::remove(handle) {
        Ok(Some(_)) => 0,
        Ok(None) | Err(_) => 1,
    }
}

/// Returns the local endpoint of an UDP handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_local_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpSocket(socket) => socket.local_endpoint(),
        _ => Err(std::io::Error::other("handle is not an UDP socket")),
    });
    let Ok(Some(Ok(endpoint))) = result else {
        return 1;
    };
    unsafe {
        if !out_address.is_null() {
            *out_address = c_string(&endpoint.address().to_string());
        }
        if !out_port.is_null() {
            *out_port = i32::from(endpoint.port());
        }
    }
    0
}

/// Sends one bounded UDP datagram to an address and port.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_send_to(
    handle: i64,
    address: *const c_char,
    port: i32,
    bytes: *const u8,
    length: i32,
    out_written: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let Some(address) = c_str(address) else {
        return 1;
    };
    let Ok(address) = net::Address::parse(address) else {
        return 1;
    };
    let Ok(port) = u16::try_from(port) else {
        return 1;
    };
    let Ok(length) = usize::try_from(length) else {
        return 1;
    };
    if length > 65_507 || (length != 0 && bytes.is_null()) {
        return 1;
    }
    let data = unsafe { std::slice::from_raw_parts(bytes, length) };
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpSocket(socket) => {
            socket.send_to(net::Endpoint::new(address, port), data)
        }
        _ => Err(std::io::Error::other("handle is not an UDP socket")),
    });
    let Ok(Some(Ok(written))) = result else {
        return 1;
    };
    unsafe {
        if !out_written.is_null() {
            *out_written = i32::try_from(written).unwrap_or(i32::MAX);
        }
    }
    0
}

/// Receives one bounded UDP datagram. The returned buffer is freed with
/// `bn_rt_net_buffer_free`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_receive(
    handle: i64,
    maximum: i32,
    timeout_ms: i32,
    out_data: *mut *mut u8,
    out_length: *mut i32,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
    out_truncated: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let Ok(maximum) = usize::try_from(maximum) else {
        return 1;
    };
    if maximum == 0 || maximum > 65_507 {
        return 1;
    }
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpSocket(socket) => {
            socket.set_read_timeout(timeout)?;
            socket.receive(maximum)
        }
        _ => Err(std::io::Error::other("handle is not an UDP socket")),
    });
    let Ok(Some(Ok(packet))) = result else {
        return 1;
    };
    let mut data = packet.bytes().to_vec().into_boxed_slice();
    let data_ptr = data.as_mut_ptr();
    let data_len = i32::try_from(data.len()).unwrap_or(i32::MAX);
    std::mem::forget(data);
    let source = packet.source();
    unsafe {
        if !out_data.is_null() {
            *out_data = data_ptr;
        }
        if !out_length.is_null() {
            *out_length = data_len;
        }
        if !out_address.is_null() {
            *out_address = c_string(&source.address().to_string());
        }
        if !out_port.is_null() {
            *out_port = i32::from(source.port());
        }
        if !out_truncated.is_null() {
            *out_truncated = i32::from(packet.truncated());
        }
    }
    0
}

/// Receives one UDP packet and returns an opaque packet handle in `out`.
/// The handle is released with `bn_rt_net_handle_close`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_receive_handle(
    handle: i64,
    maximum: i32,
    timeout_ms: i32,
    out: *mut i64,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let Ok(maximum) = usize::try_from(maximum) else {
        return 1;
    };
    if maximum == 0 || maximum > 65_507 {
        return 1;
    }
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpSocket(socket) => {
            socket.set_read_timeout(timeout)?;
            socket.receive(maximum)
        }
        _ => Err(std::io::Error::other("handle is not an UDP socket")),
    });
    let Ok(Some(Ok(packet))) = result else {
        return 1;
    };
    let Ok(packet_handle) = net::handles::insert(net::handles::Handle::UdpPacket(packet)) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = i64::try_from(packet_handle).unwrap_or(i64::MAX);
        }
    }
    0
}

/// Returns packet payload size.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_size(handle: i64) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return -1;
    };
    match net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpPacket(packet) => {
            i32::try_from(packet.bytes().len()).unwrap_or(i32::MAX)
        }
        _ => -1,
    }) {
        Ok(Some(size)) => size,
        _ => -1,
    }
}

/// Returns whether a packet was truncated at the requested receive bound.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_truncated(handle: i64) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return -1;
    };
    match net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpPacket(packet) => i32::from(packet.truncated()),
        _ => -1,
    }) {
        Ok(Some(value)) => value,
        _ => -1,
    }
}

/// Copies packet bytes into a caller-provided buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_copy_to(
    handle: i64,
    buffer: *mut u8,
    length: i32,
    out_copied: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let Ok(length) = usize::try_from(length) else {
        return 1;
    };
    if length > 65_507 || (length != 0 && buffer.is_null()) {
        return 1;
    }
    let target = unsafe { std::slice::from_raw_parts_mut(buffer, length) };
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpPacket(packet) => {
            if length < packet.bytes().len() {
                return Err(std::io::Error::other("buffer is too small"));
            }
            target[..packet.bytes().len()].copy_from_slice(packet.bytes());
            Ok(packet.bytes().len())
        }
        _ => Err(std::io::Error::other("handle is not an UDP packet")),
    });
    let Ok(Some(Ok(copied))) = result else {
        return 1;
    };
    unsafe {
        if !out_copied.is_null() {
            *out_copied = i32::try_from(copied).unwrap_or(i32::MAX);
        }
    }
    0
}

/// Returns the source endpoint of a received packet.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_source(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::UdpPacket(packet) => Ok(packet.source()),
        _ => Err(std::io::Error::other("handle is not an UDP packet")),
    });
    let Ok(Some(Ok(endpoint))) = result else {
        return 1;
    };
    unsafe {
        if !out_address.is_null() {
            *out_address = c_string(&endpoint.address().to_string());
        }
        if !out_port.is_null() {
            *out_port = i32::from(endpoint.port());
        }
    }
    0
}

/// Connects a bounded TCP stream and returns an opaque handle in `out`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_connect(
    address: *const c_char,
    port: i32,
    timeout_ms: i32,
    out: *mut i64,
) -> i32 {
    let Some(address) = c_str(address) else {
        return 1;
    };
    let Ok(address) = net::Address::parse(address) else {
        return 1;
    };
    let Ok(port) = u16::try_from(port) else {
        return 1;
    };
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    let Ok(stream) = net::TcpStream::connect(net::Endpoint::new(address, port), timeout) else {
        return 1;
    };
    let Ok(handle) = net::handles::insert(net::handles::Handle::TcpStream(stream)) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = i64::try_from(handle).unwrap_or(i64::MAX);
        }
    }
    0
}

/// Binds a TCP listener and returns an opaque runtime handle in `out`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_listen(address: *const c_char, port: i32, out: *mut i64) -> i32 {
    let Some(address) = c_str(address) else {
        return 1;
    };
    let Ok(address) = net::Address::parse(address) else {
        return 1;
    };
    let Ok(port) = u16::try_from(port) else {
        return 1;
    };
    let Ok(listener) = net::TcpListener::bind(net::Endpoint::new(address, port)) else {
        return 1;
    };
    let Ok(handle) = net::handles::insert(net::handles::Handle::TcpListener(listener)) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = i64::try_from(handle).unwrap_or(i64::MAX);
        }
    }
    0
}

/// Binds a TCP listener with an explicit bounded backlog.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_listen_with_backlog(
    address: *const c_char,
    port: i32,
    backlog: i32,
    out: *mut i64,
) -> i32 {
    let Some(address) = c_str(address) else {
        return 1;
    };
    let Ok(address) = net::Address::parse(address) else {
        return 1;
    };
    let Ok(port) = u16::try_from(port) else {
        return 1;
    };
    let Ok(backlog) = usize::try_from(backlog) else {
        return 1;
    };
    if !(1..=128).contains(&backlog) {
        return 1;
    }
    let Ok(listener) =
        net::TcpListener::bind_with_backlog(net::Endpoint::new(address, port), backlog)
    else {
        return 1;
    };
    let Ok(handle) = net::handles::insert(net::handles::Handle::TcpListener(listener)) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = i64::try_from(handle).unwrap_or(i64::MAX);
        }
    }
    0
}

/// Accepts one TCP connection, returning a stream handle in `out`; timeout is success with no stream.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_accept(handle: i64, timeout_ms: i32, out: *mut i64) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let timeout = std::time::Duration::from_millis(u64::try_from(timeout_ms.max(0)).unwrap_or(0));
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::TcpListener(listener) => listener.accept_timeout(timeout),
        _ => Err(std::io::Error::other("handle is not a TCP listener")),
    });
    let Ok(Some(Ok(Some(stream)))) = result else {
        return 1;
    };
    let Ok(stream_handle) = net::handles::insert(net::handles::Handle::TcpStream(stream)) else {
        return 1;
    };
    unsafe {
        if !out.is_null() {
            *out = i64::try_from(stream_handle).unwrap_or(i64::MAX);
        }
    }
    0
}

/// Returns the local endpoint of a TCP listener.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_listener_local_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::TcpListener(listener) => listener.local_endpoint(),
        _ => Err(std::io::Error::other("handle is not a TCP listener")),
    });
    let Ok(Some(Ok(endpoint))) = result else {
        return 1;
    };
    unsafe {
        if !out_address.is_null() {
            *out_address = c_string(&endpoint.address().to_string());
        }
        if !out_port.is_null() {
            *out_port = i32::from(endpoint.port());
        }
    }
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_stream_local_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    tcp_stream_endpoint(handle, out_address, out_port, false)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_stream_remote_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    tcp_stream_endpoint(handle, out_address, out_port, true)
}

#[allow(unsafe_code)]
fn tcp_stream_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
    remote: bool,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let result = net::handles::with(handle, |value| match value {
        net::handles::Handle::TcpStream(stream) => {
            if remote {
                stream.remote_endpoint()
            } else {
                stream.local_endpoint()
            }
        }
        _ => Err(std::io::Error::other("handle is not a TCP stream")),
    });
    let Ok(Some(Ok(endpoint))) = result else {
        return 1;
    };
    unsafe {
        if !out_address.is_null() {
            *out_address = c_string(&endpoint.address().to_string());
        }
        if !out_port.is_null() {
            *out_port = i32::from(endpoint.port());
        }
    }
    0
}

/// Reads up to `length` bytes from a TCP handle into `buffer`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_read(
    handle: i64,
    buffer: *mut u8,
    length: i32,
    out_read: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let Ok(length) = usize::try_from(length) else {
        return 1;
    };
    if length > 65_507 || (length != 0 && buffer.is_null()) {
        return 1;
    }
    let slice = unsafe { std::slice::from_raw_parts_mut(buffer, length) };
    let result = net::handles::with_mut(handle, |value| match value {
        net::handles::Handle::TcpStream(stream) => stream.read_bounded(slice),
        _ => Err(std::io::Error::other("handle is not a TCP stream")),
    });
    let Ok(Some(Ok(read))) = result else { return 1 };
    unsafe {
        if !out_read.is_null() {
            *out_read = i32::try_from(read).unwrap_or(i32::MAX);
        }
    }
    0
}

/// Writes up to `length` bytes to a TCP handle.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_write(
    handle: i64,
    buffer: *const u8,
    length: i32,
    out_written: *mut i32,
) -> i32 {
    let Ok(handle) = usize::try_from(handle) else {
        return 1;
    };
    let Ok(length) = usize::try_from(length) else {
        return 1;
    };
    if length > 65_507 || (length != 0 && buffer.is_null()) {
        return 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(buffer, length) };
    let result = net::handles::with_mut(handle, |value| match value {
        net::handles::Handle::TcpStream(stream) => stream.write_bounded(slice),
        _ => Err(std::io::Error::other("handle is not a TCP stream")),
    });
    let Ok(Some(Ok(written))) = result else {
        return 1;
    };
    unsafe {
        if !out_written.is_null() {
            *out_written = i32::try_from(written).unwrap_or(i32::MAX);
        }
    }
    0
}

/// Frees a byte buffer returned by `bn_rt_net_udp_receive`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_buffer_free(data: *mut u8, length: i32) {
    let Ok(length) = usize::try_from(length) else {
        return;
    };
    if !data.is_null() {
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                data, length,
            )));
        }
    }
}

/// Frees a NUL-terminated string returned by a network C ABI operation.
#[allow(unsafe_code)]
#[allow(clippy::same_length_and_capacity)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_string_free(data: *mut c_char) {
    if !data.is_null() {
        unsafe {
            let length = CStr::from_ptr(data).to_bytes_with_nul().len();
            drop(Vec::from_raw_parts(data.cast::<u8>(), length, length));
        }
    }
}

#[cfg(test)]
pub(crate) fn network_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .expect("network test lock")
}

#[cfg(test)]
mod tests {
    use super::{bn_rt_net_addresses_count, bn_rt_net_addresses_free, bn_rt_net_resolve};
    use super::{
        bn_rt_net_buffer_free, bn_rt_net_handle_close, bn_rt_net_string_free, bn_rt_net_udp_bind,
        bn_rt_net_udp_local_endpoint, bn_rt_net_udp_receive, bn_rt_net_udp_send_to,
    };
    use super::{
        bn_rt_net_tcp_accept, bn_rt_net_tcp_connect, bn_rt_net_tcp_listen, bn_rt_net_tcp_read,
        bn_rt_net_tcp_write,
    };
    use super::{monotonic_ns, timestamp_ms, timestamp_ms_from};
    use std::ffi::CString;
    use std::io::{Read, Write};
    use std::net::TcpStream as StdTcpStream;
    use std::time::{Duration, UNIX_EPOCH};

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
        assert_eq!(bn_rt_net_handle_close(handle), 1);
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
            bn_rt_net_tcp_listen(address.as_ptr(), 0, &raw mut listener),
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
            bn_rt_net_tcp_listen(address.as_ptr(), 0, &raw mut listener),
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
}
