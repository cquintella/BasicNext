// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for `HOST.Net` UDP sockets and packets, and handle close.

use super::*;

/// Binds an UDP socket and returns its opaque runtime handle in `out`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_udp_bind(address: *const c_char, port: i32, out: *mut i64) -> i32 {
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
