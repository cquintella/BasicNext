// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for `HOST.Net` addressing: address parsing, ICMP ping, reverse
//! and forward resolution, neighbor lookup, and address collections.

use super::*;

/// Parses an IP address. Writes a malloc'd IP or error message to `out`.
///
/// Returns 0 on success and 1 on error.
#[allow(unsafe_code)] // C ABI for HOST.Net.Address.Parse.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_address_parse(text: *const c_char, out: *mut *mut c_char) -> i32 {
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return -2;
    }
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
    if !policy::allows(policy::POLICY_NET) {
        fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.Net is denied by execution policy",
        );
        return 2;
    }
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
