// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The message channel of HOST `Error` values in native builds: a failing
//! ABI call records its message on the thread, and emitted code reads it back
//! when it builds the `Error`.

use std::ffi::c_char;

use super::c_string;

thread_local! {
    /// Message of the last failed ABI call on this thread: the `Message` of
    /// the `Error` the emitted code builds from that call's status.
    static LAST_ERROR: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

pub(crate) fn set_error(message: impl Into<String>) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = message.into());
}

/// `Error.Message` for a HOST call that returned `status`: null on success
/// (no allocation), else the recorded message, empty when the call recorded
/// none. Owned like other runtime strings.
#[allow(unsafe_code)] // C ABI: owned UTF-8 STRING.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_error_message(status: i32) -> *mut c_char {
    if status == 0 {
        return std::ptr::null_mut();
    }
    c_string(&LAST_ERROR.with(|slot| std::mem::take(&mut *slot.borrow_mut())))
}
