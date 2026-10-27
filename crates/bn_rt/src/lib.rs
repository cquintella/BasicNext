#![allow(clippy::not_unsafe_ptr_arg_deref)] // C ABI exports validate nullable out-pointers.
#![allow(clippy::single_match_else)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
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
    ffi::c_char,
    io::{self, Write},
    sync::atomic::{AtomicU64, Ordering},
};

pub mod arc;
pub mod arc_abi;
pub mod civil;
mod clock;
mod console;
pub mod cpu;
pub mod env;
pub mod error_abi;
mod exec;
pub mod file;
pub mod file_abi;
mod file_error;
mod stack_abi;
#[cfg(feature = "net")]
pub use bn_core_net as net;
#[cfg(feature = "net")]
mod net_abi;
pub mod policy;
pub mod secure_fs;
pub mod string;
mod terminal;
pub mod text_abi;
pub mod trap_abi;
mod vector_abi;

pub use stack_abi::bn_rt_stack_check;
pub use vector_abi::{Row, bn_rt_vector_nest};

pub use console::{ConsoleError, beep, cls, num_cols, num_rows, print_at};
pub use error_abi::{
    bn_rt_error_code, bn_rt_error_field, bn_rt_error_print, bn_rt_error_take, bn_rt_error_wrap,
    set_error, set_error_report, take_error_report,
};
pub use exec::*;
pub use file_abi::*;
#[cfg(feature = "net")]
pub use net::{
    Address as NetAddress, AddressesHandle, NeighborError, PingError, PingReply, ReverseError,
    join_resolver_tasks, neighbor, ping, reverse_timeout,
};
#[cfg(feature = "net")]
pub use net_abi::*;
pub use policy::{
    FsPolicy, POLICY_FILESYSTEM, Policy, PolicyError, bn_rt_policy_check, bn_rt_policy_init,
};
pub use text_abi::*;
pub use trap_abi::*;

pub use civil::format_rfc3339;
pub use clock::{monotonic_ns, timestamp_ms, timestamp_ms_from};

/// The call-boundary policy re-check denied `capability` (host-traits.md).
fn denied(capability: &str) {
    trap_abi::record_failure(RuntimeFailure {
        code: "EXECUTION_POLICY_DENIED",
        facts: vec![(
            "detail",
            format!("{capability} is denied by execution policy"),
        )],
    });
}

fn emit_console_error(error: &ConsoleError) {
    trap_abi::record_failure(error.failure());
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
pub fn libc_write_stdout(buf: &[u8]) -> io::Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    libc_fflush()?;
    let mut written = 0;
    while written < buf.len() {
        // Unix: STDOUT_FILENO + size_t count. Windows CRT: fd 1 + c_uint count.
        #[cfg(any(unix, target_os = "wasi"))]
        // SAFETY: the slice is borrowed for this call; fflush of a null stream flushes the C library's open output streams.
        let next = unsafe {
            libc::write(
                libc::STDOUT_FILENO,
                buf[written..].as_ptr().cast(),
                buf.len() - written,
            )
        };
        #[cfg(windows)]
        // SAFETY: the slice is borrowed for this call; fflush of a null stream flushes the C library's open output streams.
        let next = unsafe {
            let remaining = buf.len() - written;
            let chunk = u32::try_from(remaining).unwrap_or(u32::MAX);
            libc::write(1, buf[written..].as_ptr().cast(), chunk)
        };
        #[cfg(not(any(unix, windows, target_os = "wasi")))]
        compile_error!(concat!(
            "libc_write_stdout",
            " requires ",
            "unix",
            ", ",
            "windows",
            ", or ",
            "wasi"
        ));
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
pub fn libc_fflush() -> io::Result<()> {
    // A null stream flushes all open output streams and avoids platform-specific
    // `stdout` symbols (`__stdoutp` on Darwin, unavailable on some libc targets).
    // SAFETY: the slice is borrowed for this call; fflush of a null stream flushes the C library's open output streams.
    let rc = unsafe { libc::fflush(std::ptr::null_mut()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[allow(unsafe_code)] // C ABI: read a Basic Next STRING pointer using header length (G7-D1).
pub fn c_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    let bytes = crate::string::slice_from_raw(ptr)?;
    std::str::from_utf8(bytes).ok()
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.NumProcs.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_host_num_procs(out: *mut i32) -> i32 {
    let count = std::thread::available_parallelism()
        .map_or(1, |c| i32::try_from(c.get()).unwrap_or(i32::MAX));
    if !out.is_null() {
        // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
        unsafe { *out = count };
    }
    0
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Clock.Now.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_clock_now() -> i64 {
    if !policy::allows(policy::POLICY_CLOCK) {
        denied("HOST.Clock");
        return -1;
    }
    timestamp_ms()
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Clock.Timer.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_clock_timer() -> i64 {
    if !policy::allows(policy::POLICY_CLOCK) {
        denied("HOST.Clock");
        return -1;
    }
    monotonic_ns()
}

static COMPILED_RANDOM_STATE: AtomicU64 = AtomicU64::new(0);

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Random.Seed.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_random_seed(seed: i64) -> i32 {
    if !policy::allows(policy::POLICY_RANDOM) {
        denied("HOST.Random");
        return 2;
    }
    COMPILED_RANDOM_STATE.store(bn_types::random::seed_state(seed), Ordering::Relaxed);
    0
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Random.Random.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_random_next() -> f64 {
    if !policy::allows(policy::POLICY_RANDOM) {
        denied("HOST.Random");
        return f64::NAN;
    }
    let mut current = COMPILED_RANDOM_STATE.load(Ordering::Relaxed);
    if current == 0 {
        let initial = (monotonic_ns().cast_unsigned() ^ u64::from(std::process::id())).max(1);
        if let Err(observed) =
            COMPILED_RANDOM_STATE.compare_exchange(0, initial, Ordering::Relaxed, Ordering::Relaxed)
        {
            current = observed;
        } else {
            current = initial;
        }
    }
    loop {
        let next = bn_types::random::next_state(current);
        match COMPILED_RANDOM_STATE.compare_exchange_weak(
            current,
            next,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return bn_types::random::unit_interval(next),
            Err(observed) => current = observed,
        }
    }
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.Cls.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_cls() -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        denied("HOST.Console");
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
        denied("HOST.Console");
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
        denied("HOST.Console");
        return 2;
    }
    let Some(text) = c_str(text) else {
        trap_abi::record_failure(RuntimeFailure {
            code: "TYPE_MISMATCH",
            facts: vec![
                ("expected", "STRING".into()),
                ("actual", "text that is not UTF-8".into()),
                ("context", "HOST.Console.PrintAt".into()),
            ],
        });
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
        denied("HOST.Console");
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

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_date(days: i32) {
    let _ = LibcStdout.write_all(civil::format_date(days).as_bytes());
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_time(millis: i32) {
    let _ = LibcStdout.write_all(civil::format_time(u32::try_from(millis).unwrap_or(0)).as_bytes());
}

#[allow(unsafe_code)] // C ABI export for LLVM-emitted HOST.Console.NumRows.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_console_num_rows() -> i32 {
    if !policy::allows(policy::POLICY_CONSOLE) {
        denied("HOST.Console");
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
pub fn c_string(text: &str) -> *mut c_char {
    string::c_string_from_bytes(text.as_bytes())
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
    use super::policy::{POLICY_ALL, POLICY_CONSOLE, POLICY_NET, POLICY_OK, POLICY_VERSION};
    use super::{bn_rt_net_addresses_count, bn_rt_net_addresses_free, bn_rt_net_resolve};
    use super::{
        bn_rt_net_handle_close, bn_rt_net_udp_bind, bn_rt_net_udp_local_endpoint,
        bn_rt_net_udp_packet_copy_to, bn_rt_net_udp_packet_size, bn_rt_net_udp_packet_source,
        bn_rt_net_udp_packet_truncated, bn_rt_net_udp_receive_handle, bn_rt_net_udp_send_to,
    };
    use super::{
        bn_rt_net_tcp_accept, bn_rt_net_tcp_connect, bn_rt_net_tcp_listen_with_backlog,
        bn_rt_net_tcp_read, bn_rt_net_tcp_write,
    };
    use super::{monotonic_ns, timestamp_ms, timestamp_ms_from};
    use std::io::{Read, Write};
    use std::net::TcpStream as StdTcpStream;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]

    fn packed_string_index_preserves_ascii_and_multibyte_scalars() {
        let text = super::string::c_string_from_bytes("Aé界".as_bytes());
        for (index, expected) in ["A", "é", "界"].into_iter().enumerate() {
            let packed = super::string::bn_rt_str_index_utf8(
                text,
                i32::try_from(index).expect("small index"),
                std::ptr::null(),
            );
            let bytes = packed.to_ne_bytes();
            let nul = bytes.iter().position(|byte| *byte == 0).expect("NUL");
            assert_eq!(std::str::from_utf8(&bytes[..nul]), Ok(expected));
        }
        assert_eq!(super::string::bn_rt_str_release(text), 0);
    }

    #[test]
    fn packed_char_uses_the_same_native_byte_contract_as_string_index() {
        let text = super::string::c_string_from_bytes("é".as_bytes());
        assert_eq!(
            super::string::bn_rt_str_char_utf8(i64::from(u32::from('é'))),
            super::string::bn_rt_str_index_utf8(text, 0, std::ptr::null())
        );
        assert_eq!(super::string::bn_rt_str_release(text), 0);
    }

    #[test]
    fn console_c_abi_rechecks_execution_policy_at_call_boundary() {
        let _policy = super::policy::reset_for_tests();
        assert_eq!(
            super::bn_rt_policy_init(POLICY_VERSION, POLICY_ALL),
            POLICY_OK
        );
        super::policy::restrict_for_tests(POLICY_ALL & !(POLICY_CONSOLE | POLICY_NET));
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
        let host = super::c_string("localhost");
        let mut handle = std::ptr::null_mut();
        let status = bn_rt_net_resolve(host, 1_000, &raw mut handle);
        assert_eq!(status, 0);
        assert!(!handle.is_null());
        assert!((0..=64).contains(&bn_rt_net_addresses_count(handle)));
        bn_rt_net_addresses_free(handle);
        crate::string::bn_rt_str_release(host);
    }

    #[test]
    fn udp_bind_c_abi_allocates_and_closes_handle() {
        let _lock = super::network_test_lock();
        let address = super::c_string("127.0.0.1");
        let mut handle = -1;
        assert_eq!(bn_rt_net_udp_bind(address, 0, &raw mut handle), 0);
        assert!(handle >= 0);
        assert_eq!(bn_rt_net_handle_close(handle), 0);
        // Close is idempotent (host-net.md): a second Close is not an error.
        assert_eq!(bn_rt_net_handle_close(handle), 0);
        crate::string::bn_rt_str_release(address);
    }

    #[test]
    fn udp_local_endpoint_c_abi_returns_bound_port() {
        let _lock = super::network_test_lock();
        let address = super::c_string("127.0.0.1");
        let mut handle = -1;
        assert_eq!(bn_rt_net_udp_bind(address, 0, &raw mut handle), 0);
        let mut rendered = std::ptr::null_mut();
        let mut port = -1;
        assert_eq!(
            bn_rt_net_udp_local_endpoint(handle, &raw mut rendered, &raw mut port),
            0
        );
        assert!(!rendered.is_null());
        assert!(port > 0);
        assert_eq!(bn_rt_net_handle_close(handle), 0);
        crate::string::bn_rt_str_release(address);
    }

    #[test]
    fn udp_receive_rejects_invalid_handle_and_bounds() {
        let _lock = super::network_test_lock();
        let mut packet = -1;
        assert_eq!(
            bn_rt_net_udp_receive_handle(-1, 65_507, 1_000, &raw mut packet),
            1
        );
        assert_eq!(packet, -1);
        assert_eq!(bn_rt_net_udp_packet_size(-1), -1);
    }

    #[test]
    #[allow(unsafe_code)]
    fn udp_c_abi_round_trip_preserves_payload_and_source() {
        let _lock = super::network_test_lock();
        let address = super::c_string("127.0.0.1");
        let mut sender = -1;
        let mut receiver = -1;
        assert_eq!(bn_rt_net_udp_bind(address, 0, &raw mut sender), 0);
        assert_eq!(bn_rt_net_udp_bind(address, 0, &raw mut receiver), 0);

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

        // The path `bnc` emits: a packet handle, then its members.
        let mut packet = -1;
        assert_eq!(
            bn_rt_net_udp_receive_handle(receiver, 1024, 1_000, &raw mut packet),
            0
        );
        assert_eq!(bn_rt_net_udp_packet_size(packet), 4);
        assert_eq!(bn_rt_net_udp_packet_truncated(packet), 0);
        let mut payload_out = [0u8; 8];
        let mut copied = -1;
        assert_eq!(
            bn_rt_net_udp_packet_copy_to(packet, payload_out.as_mut_ptr(), 8, &raw mut copied),
            0
        );
        assert_eq!(&payload_out[..4], payload);
        assert_eq!(copied, 4);
        let mut source = std::ptr::null_mut();
        let mut source_port = -1;
        assert_eq!(
            bn_rt_net_udp_packet_source(packet, &raw mut source, &raw mut source_port),
            0
        );
        assert!(source_port > 0);
        assert_eq!(bn_rt_net_handle_close(packet), 0);
        crate::string::bn_rt_str_release(receiver_address.cast());
        crate::string::bn_rt_str_release(source);
        assert_eq!(bn_rt_net_handle_close(sender), 0);
        assert_eq!(bn_rt_net_handle_close(receiver), 0);
        crate::string::bn_rt_str_release(address);
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
        let address = super::c_string("127.0.0.1");
        let mut handle = -1;
        assert_eq!(
            bn_rt_net_tcp_connect(address, i32::from(port), 1_000, &raw mut handle),
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
        crate::string::bn_rt_str_release(address);
        worker.join().expect("worker completed");
    }

    #[test]
    #[allow(unsafe_code)]
    fn tcp_listener_c_abi_accepts_connection() {
        let _lock = super::network_test_lock();
        let address = super::c_string("127.0.0.1");
        let mut listener = -1;
        assert_eq!(
            bn_rt_net_tcp_listen_with_backlog(address, 0, 8, &raw mut listener),
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
        crate::string::bn_rt_str_release(rendered.cast());
        assert_eq!(bn_rt_net_handle_close(accepted), 0);
        assert_eq!(bn_rt_net_handle_close(listener), 0);
        crate::string::bn_rt_str_release(address);
    }

    #[test]
    #[allow(unsafe_code)]
    fn tcp_read_c_abi_reports_eof_as_zero_bytes() {
        let _lock = super::network_test_lock();
        let address = super::c_string("127.0.0.1");
        let mut listener = -1;
        assert_eq!(
            bn_rt_net_tcp_listen_with_backlog(address, 0, 8, &raw mut listener),
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
        crate::string::bn_rt_str_release(rendered.cast());
        assert_eq!(bn_rt_net_handle_close(accepted), 0);
        assert_eq!(bn_rt_net_handle_close(listener), 0);
        crate::string::bn_rt_str_release(address);
    }

    #[test]
    fn clock_functions_respect_policy_clock() {
        let _policy = super::policy::reset_for_tests();
        assert!(super::bn_rt_clock_now() > 0);
        assert!(super::bn_rt_clock_timer() >= 0);

        // Restrict policy to exclude POLICY_CLOCK
        super::policy::restrict_for_tests(super::policy::POLICY_CONSOLE);
        assert_eq!(super::bn_rt_clock_now(), -1);
        assert_eq!(super::bn_rt_clock_timer(), -1);
    }
}
