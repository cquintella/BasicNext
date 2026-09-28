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
