// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native C ABI for `HOST.Env`: `bn_rt_env_get` and `bn_rt_env_has`.
//! Validates C strings, reads process policy, and calls `bn_host_env`.

use std::ffi::{CStr, c_char};

pub use bn_host_env::{INVALID_NAME, INVALID_UTF8, NOT_SET, POLICY_DENIED};

#[allow(unsafe_code)]
fn input<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr).to_str().ok() }
}

#[allow(unsafe_code)]
fn write_owned(out: *mut *mut c_char, value: &str) -> i32 {
    let bytes = value.as_bytes();
    let ptr = unsafe { libc::malloc(bytes.len() + 1) }.cast::<u8>();
    if ptr.is_null() {
        crate::set_error("out of memory");
        return 1;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        ptr.add(bytes.len()).write(0);
        out.write(ptr.cast());
    }
    0
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_env_get(name: *const c_char, out_val: *mut *mut c_char) -> i32 {
    if out_val.is_null() {
        return INVALID_NAME;
    }
    unsafe { *out_val = std::ptr::null_mut() };
    let Some(name) = input(name) else {
        crate::set_error_report(
            INVALID_NAME,
            "HOST.Env.Get",
            "name is empty or contains '=' or NUL",
            "invalid name",
        );
        return INVALID_NAME;
    };
    let policy = crate::policy::env_policy();
    match bn_host_env::get(name, &policy) {
        Ok(value) => write_owned(out_val, &value),
        Err(failure) => {
            crate::set_error_report(
                failure.code,
                failure.operation,
                failure.message,
                failure.cause,
            );
            failure.code
        }
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_env_has(name: *const c_char, out_has: *mut i32) -> i32 {
    if out_has.is_null() {
        return INVALID_NAME;
    }
    unsafe { *out_has = 0 };
    let Some(name) = input(name) else {
        crate::set_error_report(
            INVALID_NAME,
            "HOST.Env.Has",
            "name is empty or contains '=' or NUL",
            "invalid name",
        );
        return INVALID_NAME;
    };
    let policy = crate::policy::env_policy();
    match bn_host_env::has(name, &policy) {
        Ok(present) => {
            unsafe { *out_has = i32::from(present) };
            0
        }
        Err(failure) => {
            crate::set_error_report(
                failure.code,
                failure.operation,
                failure.message,
                failure.cause,
            );
            failure.code
        }
    }
}
