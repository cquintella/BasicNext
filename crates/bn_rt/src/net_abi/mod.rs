// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The `HOST.Net` C ABI of native programs, over the shared `net` core.

#![allow(clippy::wildcard_imports)]

mod address;
mod tcp;
mod udp;

pub use address::*;
pub use tcp::*;
pub use udp::*;

use super::*;

use super::net::error::{Failure, NetError};
use super::net::handles::{self, Handle};

/// Records `error` for the `Error` the emitted code builds; the status: 2
/// for a policy denial (the call-boundary re-check, host-traits.md), else 1.
fn failed(error: &NetError) -> i32 {
    super::set_error_report(
        error.code(),
        error.operation(),
        error.message(),
        error.cause(),
    );
    if error.code() == bn_types::error_codes::net::POLICY_DENIED {
        2
    } else {
        1
    }
}

/// `Net.POLICY_DENIED` unless the execution policy allows `HOST.Net`.
fn authorized(operation: &'static str, action: &str) -> Result<(), NetError> {
    if policy::allows(policy::POLICY_NET) {
        Ok(())
    } else {
        Err(NetError::new(operation, action, Failure::PolicyDenied))
    }
}

/// A text argument from emitted code (always valid UTF-8 from BN strings).
fn text_argument<'a>(
    pointer: *const c_char,
    operation: &'static str,
    action: &str,
) -> Result<&'a str, NetError> {
    c_str(pointer).ok_or_else(|| {
        NetError::new(
            operation,
            action,
            Failure::InvalidArgument("the text is not valid UTF-8".into()),
        )
    })
}

/// Stores a new socket or packet; `Net.LIMIT` past the quota.
fn store(value: Handle, operation: &'static str, action: &str) -> Result<i64, NetError> {
    let index = handles::insert(value).map_err(|_| net::quota_exceeded(operation, action))?;
    Ok(i64::try_from(index).unwrap_or(i64::MAX))
}

/// An endpoint argument from emitted code, after the policy re-check.
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

/// The C status of `result`: 0, or the recorded failure's status.
fn status(result: Result<(), NetError>) -> i32 {
    result.map_or_else(|error| failed(&error), |()| 0)
}

fn write_endpoint(endpoint: net::Endpoint, out_address: *mut *mut c_char, out_port: *mut i32) {
    write_out(out_address, c_string(&endpoint.address().to_string()));
    write_out(out_port, i32::from(endpoint.port()));
}

/// Writes `value` to `out` when the caller supplied a slot.
#[allow(unsafe_code)] // C ABI out-parameter.
fn write_out<T>(out: *mut T, value: T) {
    if !out.is_null() {
        // SAFETY: emitted code passes a writable slot for `T` or null.
        unsafe { out.write(value) };
    }
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
