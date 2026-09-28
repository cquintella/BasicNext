// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for `HOST.Net` TCP streams and listeners. The operations are the
//! shared core (`net::tcp`); a failure records its `Error` (status 1, or 2
//! for a policy denial) for the emitted code to read.

use super::*;

use net::handles::{self, Handle};

/// Runs `f` on the stream behind `handle`; a closed or unknown handle is
/// `None` (`Net.CLOSED` in the core).
fn with_stream<T>(
    handle: i64,
    f: impl FnOnce(Option<&mut net::TcpStream>) -> Result<T, NetError>,
) -> Result<T, NetError> {
    let mut f = Some(f);
    if let Ok(index) = usize::try_from(handle)
        && let Ok(Some(Some(result))) = handles::with_mut(index, |value| match value {
            Handle::TcpStream(stream) => f.take().map(|f| f(Some(stream))),
            _ => None,
        })
    {
        return result;
    }
    f.take().expect("stream operation not yet run")(None)
}

/// Runs `f` on the listener behind `handle`, as a one-listener set.
fn with_listener<T>(
    handle: i64,
    f: impl FnOnce(Option<&[net::TcpListener]>) -> Result<T, NetError>,
) -> Result<T, NetError> {
    let mut f = Some(f);
    if let Ok(index) = usize::try_from(handle)
        && let Ok(Some(Some(result))) = handles::with(index, |value| match value {
            Handle::TcpListener(listener) => {
                f.take().map(|f| f(Some(std::slice::from_ref(listener))))
            }
            _ => None,
        })
    {
        return result;
    }
    f.take().expect("listener operation not yet run")(None)
}

/// Stores a new socket; `Net.LIMIT` past the quota.
fn store(value: Handle, operation: &'static str, action: &str) -> Result<i64, NetError> {
    let index = handles::insert(value).map_err(|_| net::quota_exceeded(operation, action))?;
    Ok(i64::try_from(index).unwrap_or(i64::MAX))
}

fn endpoint_argument(
    address: *const c_char,
    port: i32,
    operation: &'static str,
    action: &str,
) -> Result<net::Endpoint, NetError> {
    authorized(operation, action)?;
    let address = net::parse_address(text_argument(address, operation, action)?)?;
    let port = u16::try_from(port).map_err(|_| {
        NetError::new(
            operation,
            action,
            Failure::InvalidArgument(format!("the port must be within 0..65535; got {port}")),
        )
    })?;
    Ok(net::Endpoint::new(address, port))
}

fn status(result: Result<(), NetError>) -> i32 {
    result.map_or_else(|error| failed(&error), |()| 0)
}

/// `HOST.Net.TCPConnect`: `out` receives the stream handle.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_connect(
    address: *const c_char,
    port: i32,
    timeout_ms: i32,
    out: *mut i64,
) -> i32 {
    const OPERATION: &str = "HOST.Net.TCPConnect";
    status(
        endpoint_argument(address, port, OPERATION, "connect")
            .and_then(|endpoint| net::tcp_connect(endpoint, timeout_ms.into()))
            .and_then(|stream| store(Handle::TcpStream(stream), OPERATION, "connect"))
            .map(|handle| write_out(out, handle)),
    )
}

/// `HOST.Net.TCPListen` on one endpoint: `out` receives the listener handle.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_listen_with_backlog(
    address: *const c_char,
    port: i32,
    backlog: i32,
    out: *mut i64,
) -> i32 {
    const OPERATION: &str = "HOST.Net.TCPListen";
    status(
        endpoint_argument(address, port, OPERATION, "listen")
            .and_then(|endpoint| net::tcp_listen(&[endpoint], backlog.into()))
            .and_then(|listeners| {
                let listener = listeners.into_iter().next().ok_or_else(|| {
                    NetError::new(
                        OPERATION,
                        "listen",
                        Failure::InvalidArgument("the listener set is empty".into()),
                    )
                })?;
                store(Handle::TcpListener(listener), OPERATION, "listen")
            })
            .map(|handle| write_out(out, handle)),
    )
}

/// `HOST.Net.TCPListener.Accept`: `out` receives the stream handle.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_accept(handle: i64, timeout_ms: i32, out: *mut i64) -> i32 {
    const OPERATION: &str = "HOST.Net.TCPListener.Accept";
    status(
        authorized(OPERATION, "accept a connection")
            .and_then(|()| {
                with_listener(handle, |listeners| {
                    net::tcp_accept(listeners, timeout_ms.into())
                })
            })
            .and_then(|stream| store(Handle::TcpStream(stream), OPERATION, "accept a connection"))
            .map(|handle| write_out(out, handle)),
    )
}

fn write_endpoint(endpoint: net::Endpoint, out_address: *mut *mut c_char, out_port: *mut i32) {
    write_out(out_address, c_string(&endpoint.address().to_string()));
    write_out(out_port, i32::from(endpoint.port()));
}

/// `HOST.Net.TCPListener.LocalEndpoint`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_listener_local_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    status(
        with_listener(handle, net::listener_endpoint)
            .map(|endpoint| write_endpoint(endpoint, out_address, out_port)),
    )
}

/// `HOST.Net.TCPStream.LocalEndpoint`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_stream_local_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    status(
        with_stream(handle, |stream| {
            net::tcp_endpoint(stream.map(|stream| &*stream), false)
        })
        .map(|endpoint| write_endpoint(endpoint, out_address, out_port)),
    )
}

/// `HOST.Net.TCPStream.RemoteEndpoint`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_stream_remote_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    status(
        with_stream(handle, |stream| {
            net::tcp_endpoint(stream.map(|stream| &*stream), true)
        })
        .map(|endpoint| write_endpoint(endpoint, out_address, out_port)),
    )
}

/// A byte buffer from emitted code of `length` bytes, within the transfer
/// bound.
fn transfer_length(length: i32, operation: &'static str, action: &str) -> Result<usize, NetError> {
    usize::try_from(length)
        .ok()
        .filter(|length| *length <= net::TRANSFER_MAX)
        .ok_or_else(|| {
            NetError::new(
                operation,
                action,
                Failure::InvalidArgument(format!(
                    "the byte count must be within 0..{}; got {length}",
                    net::TRANSFER_MAX
                )),
            )
        })
}

/// `HOST.Net.TCPStream.Read`: `out_read` receives the byte count (0 is EOF).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_read(
    handle: i64,
    buffer: *mut u8,
    length: i32,
    out_read: *mut i32,
) -> i32 {
    const OPERATION: &str = "HOST.Net.TCPStream.Read";
    write_out(out_read, 0);
    status(
        authorized(OPERATION, "read from the TCP stream")
            .and_then(|()| transfer_length(length, OPERATION, "read from the TCP stream"))
            .and_then(|length| {
                let slice: &mut [u8] = if length == 0 || buffer.is_null() {
                    &mut []
                } else {
                    // SAFETY: emitted code passes a live buffer of `length`
                    // bytes (the BYTE vector's fat pointer).
                    unsafe { std::slice::from_raw_parts_mut(buffer, length) }
                };
                with_stream(handle, |stream| net::tcp_read(stream, slice))
            })
            .map(|read| write_out(out_read, i32::try_from(read).unwrap_or(i32::MAX))),
    )
}

/// `HOST.Net.TCPStream.Write`: `out_written` receives the bytes written.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_write(
    handle: i64,
    buffer: *const u8,
    length: i32,
    out_written: *mut i32,
) -> i32 {
    const OPERATION: &str = "HOST.Net.TCPStream.Write";
    write_out(out_written, 0);
    status(
        authorized(OPERATION, "write to the TCP stream")
            .and_then(|()| transfer_length(length, OPERATION, "write to the TCP stream"))
            .and_then(|length| {
                let slice: &[u8] = if length == 0 || buffer.is_null() {
                    &[]
                } else {
                    // SAFETY: emitted code passes a live buffer of `length`
                    // bytes (the BYTE vector's fat pointer).
                    unsafe { std::slice::from_raw_parts(buffer, length) }
                };
                with_stream(handle, |stream| net::tcp_write(stream, slice))
            })
            .map(|written| write_out(out_written, i32::try_from(written).unwrap_or(i32::MAX))),
    )
}
