// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for `HOST.Net` addressing: address parsing, ICMP ping, reverse
//! and forward resolution, neighbor lookup, and address collections. The
//! operations are the shared core (`net::addressing`, `net::lookup`); a
//! failure records its `Error` (status 1) for the emitted code to read.

use super::*;

/// `HOST.Net.Address.Parse`: `out` receives the canonical address text.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_address_parse(text: *const c_char, out: *mut *mut c_char) -> i32 {
    const OPERATION: &str = "HOST.Net.Address.Parse";
    let parsed = authorized(OPERATION, "parse an IP address")
        .and_then(|()| text_argument(text, OPERATION, "parse an IP address"))
        .and_then(net::parse_address);
    match parsed {
        Ok(address) => {
            write_out(out, c_string(&address.to_string()));
            0
        }
        Err(error) => failed(&error),
    }
}

/// An address argument of `operation`.
fn address_argument(
    pointer: *const c_char,
    operation: &'static str,
    action: &str,
) -> Result<net::Address, NetError> {
    authorized(operation, action)?;
    net::parse_address(text_argument(pointer, operation, action)?)
}

/// `HOST.Net.Ping`: `out` receives the reply address, `out_rtt` the RTT in µs.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_ping(
    address: *const c_char,
    timeout_ms: i32,
    out: *mut *mut c_char,
    out_rtt: *mut i64,
) -> i32 {
    write_out(out_rtt, 0);
    let reply = address_argument(address, "HOST.Net.Ping", "ping an address")
        .and_then(|address| net::ping_address(address, timeout_ms.into()));
    match reply {
        Ok(reply) => {
            write_out(out, c_string(&reply.address.to_string()));
            write_out(out_rtt, reply.round_trip_microseconds);
            0
        }
        Err(error) => failed(&error),
    }
}

/// `HOST.Net.Reverse`: `out` receives the host name.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_reverse(
    address: *const c_char,
    timeout_ms: i32,
    out: *mut *mut c_char,
) -> i32 {
    let name = address_argument(address, "HOST.Net.Reverse", "find the name of an address")
        .and_then(|address| net::reverse_lookup(address, timeout_ms.into()));
    match name {
        Ok(name) => {
            write_out(out, c_string(&name));
            0
        }
        Err(error) => failed(&error),
    }
}

/// `HOST.Net.Neighbor`: `out` receives the neighbor address.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_neighbor(address: *const c_char, out: *mut *mut c_char) -> i32 {
    let found = address_argument(address, "HOST.Net.Neighbor", "look up a neighbor")
        .and_then(net::neighbor_of);
    match found {
        Ok(found) => {
            write_out(out, c_string(&found.to_string()));
            0
        }
        Err(error) => failed(&error),
    }
}

/// `HOST.Net.Resolve`: `out` receives an opaque `AddressesHandle`, freed with
/// `bn_rt_net_addresses_free`. The bound on addresses is the interpreter's
/// (`bn_limits`).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_resolve(
    host: *const c_char,
    timeout_ms: i32,
    out: *mut *mut std::ffi::c_void,
) -> i32 {
    const OPERATION: &str = "HOST.Net.Resolve";
    let addresses = authorized(OPERATION, "resolve a host")
        .and_then(|()| text_argument(host, OPERATION, "resolve a host"))
        .and_then(|host| {
            net::resolve_host(
                host,
                timeout_ms.into(),
                bn_limits::web_limits().resolved_addresses_max,
            )
        });
    match addresses {
        Ok(addresses) => {
            let handle = AddressesHandle::from_addresses(addresses);
            write_out(
                out,
                Box::into_raw(Box::new(handle)).cast::<std::ffi::c_void>(),
            );
            0
        }
        Err(error) => failed(&error),
    }
}

/// `HOST.Net.Addresses.Count`; -1 for a null collection, -2 when the
/// execution policy denies `HOST.Net` (re-checked at every call).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_addresses_count(handle: *const std::ffi::c_void) -> i32 {
    if let Err(error) = authorized("HOST.Net.Addresses.Count", "count addresses") {
        failed(&error);
        return -2;
    }
    if handle.is_null() {
        return -1;
    }
    // SAFETY: `handle` is a live `AddressesHandle` from `bn_rt_net_resolve`.
    let handle = unsafe { &*handle.cast::<AddressesHandle>() };
    i32::try_from(handle.len()).unwrap_or(i32::MAX)
}

/// `HOST.Net.Addresses.Get`: `out` receives the address at `index`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_addresses_get(
    handle: *const std::ffi::c_void,
    index: i32,
    out: *mut *mut c_char,
) -> i32 {
    if let Err(error) = authorized("HOST.Net.Addresses.Get", "get an address") {
        return failed(&error);
    }
    // SAFETY: `handle` is null or a live `AddressesHandle` from
    // `bn_rt_net_resolve`.
    let addresses = unsafe { handle.cast::<AddressesHandle>().as_ref() };
    let count = addresses.map_or(0, AddressesHandle::len);
    let found = usize::try_from(index)
        .ok()
        .and_then(|index| addresses.and_then(|addresses| addresses.get(index)));
    if let Some(address) = found {
        write_out(out, c_string(&address.to_string()));
        return 0;
    }
    let upper = count.saturating_sub(1);
    failed(&NetError::new(
        "HOST.Net.Addresses.Get",
        format!("get address {index}"),
        Failure::InvalidArgument(format!("the index must be within 0..{upper}; got {index}")),
    ))
}

#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_net_addresses_free(handle: *mut std::ffi::c_void) {
    if !handle.is_null() {
        // SAFETY: `handle` came from `Box::into_raw` in `bn_rt_net_resolve`.
        unsafe {
            drop(Box::from_raw(handle.cast::<AddressesHandle>()));
        }
    }
}
