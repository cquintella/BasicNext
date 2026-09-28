// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for `HOST.Net` TCP streams and listeners.

use super::*;

/// Connects a bounded TCP stream and returns an opaque handle in `out`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_tcp_connect(
    address: *const c_char,
    port: i32,
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
