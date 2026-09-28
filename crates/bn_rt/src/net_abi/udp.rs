// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for `HOST.Net` UDP sockets and packets, and handle close. The
//! operations are the shared core (`net::udp`); a failure records its
//! `Error` (status 1, or 2 for a policy denial) for the emitted code to read.

use super::*;

/// Runs `f` on a clone of the socket behind `handle`, outside the
/// handle-table lock (a blocking Receive must not stall other sockets); a
/// closed or unknown handle is `None` (`Net.CLOSED` in the core).
fn with_socket<T>(
    handle: i64,
    f: impl FnOnce(Option<&net::UdpSocket>) -> Result<T, NetError>,
) -> Result<T, NetError> {
    let socket = usize::try_from(handle).ok().and_then(|index| {
        handles::with(index, |value| match value {
            Handle::UdpSocket(socket) => socket.try_clone().ok(),
            _ => None,
        })
        .ok()
        .flatten()
        .flatten()
    });
    f(socket.as_ref())
}

/// Runs `f` on the packet behind `handle` (packets do no I/O).
fn with_packet<T>(handle: i64, f: impl FnOnce(&net::UdpPacket) -> T) -> Option<T> {
    let index = usize::try_from(handle).ok()?;
    handles::with(index, |value| match value {
        Handle::UdpPacket(packet) => Some(f(packet)),
        _ => None,
    })
    .ok()
    .flatten()
    .flatten()
}

/// A byte count from emitted code, within the datagram bound.
fn datagram_bytes<'a>(
    bytes: *const u8,
    length: i32,
    operation: &'static str,
    action: &str,
) -> Result<&'a [u8], NetError> {
    let maximum = net::datagram_max();
    let length = usize::try_from(length)
        .ok()
        .filter(|length| *length <= maximum)
        .ok_or_else(|| {
            NetError::new(
                operation,
                action,
                Failure::InvalidArgument(format!(
                    "a datagram holds 0..{maximum} bytes; got {length}"
                )),
            )
        })?;
    if length == 0 || bytes.is_null() {
        return Ok(&[]);
    }
    // SAFETY: emitted code passes a live buffer of `length` bytes (the BYTE
    // vector's fat pointer).
    #[allow(unsafe_code)]
    Ok(unsafe { std::slice::from_raw_parts(bytes, length) })
}

/// `HOST.Net.UDPBind`: `out` receives the socket handle.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_bind(address: *const c_char, port: i32, out: *mut i64) -> i32 {
    const OPERATION: &str = "HOST.Net.UDPBind";
    status(
        endpoint_argument(address, port, OPERATION, "bind")
            .and_then(net::udp_bind)
            .and_then(|socket| store(Handle::UdpSocket(socket), OPERATION, "bind"))
            .map(|handle| write_out(out, handle)),
    )
}

/// `Close` of a stream, listener, or socket. Idempotent (host-net.md): a
/// handle that is already closed is not an error.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_handle_close(handle: i64) -> i32 {
    if let Ok(handle) = usize::try_from(handle) {
        let _ = handles::remove(handle);
    }
    0
}

/// `HOST.Net.UDPSocket.LocalEndpoint`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_local_endpoint(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    status(
        with_socket(handle, net::udp_local_endpoint)
            .map(|endpoint| write_endpoint(endpoint, out_address, out_port)),
    )
}

/// `HOST.Net.UDPSocket.SendTo`: `out_written` receives the bytes sent.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_send_to(
    handle: i64,
    address: *const c_char,
    port: i32,
    bytes: *const u8,
    length: i32,
    out_written: *mut i32,
) -> i32 {
    const OPERATION: &str = "HOST.Net.UDPSocket.SendTo";
    write_out(out_written, 0);
    status(
        endpoint_argument(address, port, OPERATION, "send a datagram")
            .and_then(|endpoint| {
                let data = datagram_bytes(bytes, length, OPERATION, "send a datagram")?;
                with_socket(handle, |socket| net::udp_send_to(socket, endpoint, data))
            })
            .map(|written| write_out(out_written, i32::try_from(written).unwrap_or(i32::MAX))),
    )
}

fn receive(handle: i64, maximum: i32, timeout_ms: i32) -> Result<net::UdpPacket, NetError> {
    const OPERATION: &str = "HOST.Net.UDPSocket.Receive";
    authorized(OPERATION, "receive a datagram")?;
    with_socket(handle, |socket| {
        net::udp_receive(socket, maximum.into(), timeout_ms.into())
    })
}

/// Receives one datagram into a new buffer, freed with
/// `bn_rt_net_buffer_free`.
#[allow(unsafe_code)] // C ABI export.
#[allow(clippy::too_many_arguments)]
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
    status(receive(handle, maximum, timeout_ms).map(|packet| {
        let mut data = packet.bytes().to_vec().into_boxed_slice();
        write_out(out_length, i32::try_from(data.len()).unwrap_or(i32::MAX));
        write_out(out_data, data.as_mut_ptr());
        std::mem::forget(data);
        write_endpoint(packet.source(), out_address, out_port);
        write_out(out_truncated, i32::from(packet.truncated()));
    }))
}

/// `HOST.Net.UDPSocket.Receive`: `out` receives a packet handle, released
/// with `bn_rt_net_handle_close`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_receive_handle(
    handle: i64,
    maximum: i32,
    timeout_ms: i32,
    out: *mut i64,
) -> i32 {
    status(
        receive(handle, maximum, timeout_ms)
            .and_then(|packet| {
                store(
                    Handle::UdpPacket(packet),
                    "HOST.Net.UDPSocket.Receive",
                    "keep the datagram",
                )
            })
            .map(|packet| write_out(out, packet)),
    )
}

/// `HOST.Net.UDPPacket.Size` (-1 for an unknown packet handle).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_size(handle: i64) -> i32 {
    with_packet(handle, |packet| {
        i32::try_from(packet.bytes().len()).unwrap_or(i32::MAX)
    })
    .unwrap_or(-1)
}

/// `HOST.Net.UDPPacket.Truncated` (-1 for an unknown packet handle).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_truncated(handle: i64) -> i32 {
    with_packet(handle, |packet| i32::from(packet.truncated())).unwrap_or(-1)
}

/// `HOST.Net.UDPPacket.CopyTo`: copies at most `length` bytes, as the
/// interpreter does; `out_copied` receives the count.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_copy_to(
    handle: i64,
    buffer: *mut u8,
    length: i32,
    out_copied: *mut i32,
) -> i32 {
    const OPERATION: &str = "HOST.Net.UDPPacket.CopyTo";
    write_out(out_copied, 0);
    let result = with_packet(handle, |packet| {
        let count = packet
            .bytes()
            .len()
            .min(usize::try_from(length).unwrap_or(0));
        if count > 0 && !buffer.is_null() {
            // SAFETY: emitted code passes a live buffer of at least `length`
            // bytes, and `count <= length`.
            unsafe { std::slice::from_raw_parts_mut(buffer, count) }
                .copy_from_slice(&packet.bytes()[..count]);
        }
        count
    })
    .ok_or_else(|| NetError::new(OPERATION, "copy the datagram", Failure::Closed));
    status(result.map(|count| write_out(out_copied, i32::try_from(count).unwrap_or(i32::MAX))))
}

/// `HOST.Net.UDPPacket.Source`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_packet_source(
    handle: i64,
    out_address: *mut *mut c_char,
    out_port: *mut i32,
) -> i32 {
    let result = with_packet(handle, net::UdpPacket::source).ok_or_else(|| {
        NetError::new(
            "HOST.Net.UDPPacket.Source",
            "read the datagram's source",
            Failure::Closed,
        )
    });
    status(result.map(|endpoint| write_endpoint(endpoint, out_address, out_port)))
}
