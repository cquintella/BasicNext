// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI exports for `BNFfi` standard module native calls.

#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::cast_possible_truncation)]

use std::ffi::{c_char, c_void};

use super::ffi::{
    CValuePayload, ERROR_INVALID_ARGUMENT, TYPE_ARGS, allocate_record, args_add, args_count,
    array_read_from, array_write_to, call_c_function_with_args, get_handle_ptr, get_record,
    read_c_string, release_record, update_record_payload,
};

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_new_record(type_code: u32) -> u64 {
    let payload = match type_code {
        super::ffi::TYPE_INT8 => CValuePayload::Int8(0),
        super::ffi::TYPE_UINT8 => CValuePayload::UInt8(0),
        super::ffi::TYPE_INT16 => CValuePayload::Int16(0),
        super::ffi::TYPE_UINT16 => CValuePayload::UInt16(0),
        super::ffi::TYPE_INT32 | super::ffi::TYPE_INT => CValuePayload::Int32(0),
        super::ffi::TYPE_UINT32 | super::ffi::TYPE_UINT => CValuePayload::UInt32(0),
        super::ffi::TYPE_INT64 => CValuePayload::Int64(0),
        super::ffi::TYPE_UINT64 => CValuePayload::UInt64(0),
        super::ffi::TYPE_LONG => CValuePayload::Long(0),
        super::ffi::TYPE_ULONG => CValuePayload::ULong(0),
        super::ffi::TYPE_SIZE_T => CValuePayload::SizeT(0),
        super::ffi::TYPE_FLOAT => CValuePayload::Float(0.0),
        super::ffi::TYPE_DOUBLE => CValuePayload::Double(0.0),
        super::ffi::TYPE_BOOL => CValuePayload::Bool(false),
        super::ffi::TYPE_STRING => CValuePayload::String(String::new()),
        super::ffi::TYPE_VOID => CValuePayload::Void,
        super::ffi::TYPE_HANDLE => CValuePayload::Handle(std::ptr::null_mut()),
        super::ffi::TYPE_HANDLE_RESULT => CValuePayload::HandleResult(None),
        TYPE_ARGS => CValuePayload::Args(Vec::new()),
        _ => CValuePayload::Void,
    };
    allocate_record(type_code, payload)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_release(handle: u64) {
    release_record(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_int8(handle: u64, val: i8) {
    update_record_payload(handle, CValuePayload::Int8(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_uint8(handle: u64, val: u8) {
    update_record_payload(handle, CValuePayload::UInt8(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_int16(handle: u64, val: i16) {
    update_record_payload(handle, CValuePayload::Int16(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_uint16(handle: u64, val: u16) {
    update_record_payload(handle, CValuePayload::UInt16(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_int32(handle: u64, val: i32) {
    update_record_payload(handle, CValuePayload::Int32(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_uint32(handle: u64, val: u32) {
    update_record_payload(handle, CValuePayload::UInt32(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_int64(handle: u64, val: i64) {
    update_record_payload(handle, CValuePayload::Int64(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_uint64(handle: u64, val: u64) {
    update_record_payload(handle, CValuePayload::UInt64(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_float(handle: u64, val: f32) {
    update_record_payload(handle, CValuePayload::Float(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_double(handle: u64, val: f64) {
    update_record_payload(handle, CValuePayload::Double(val));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_bool(handle: u64, val: i32) {
    update_record_payload(handle, CValuePayload::Bool(val != 0));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_string(handle: u64, text: *const c_char) {
    let text_str = crate::string::string_text(text).unwrap_or_default();
    update_record_payload(handle, CValuePayload::String(text_str));
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_set_array(
    handle: u64,
    data_ptr: *mut u8,
    element_size: usize,
    count: usize,
) {
    update_record_payload(
        handle,
        CValuePayload::Array {
            data: data_ptr,
            element_size,
            count,
        },
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_int8(handle: u64) -> i8 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Int8(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_uint8(handle: u64) -> u8 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::UInt8(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_int16(handle: u64) -> i16 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Int16(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_uint16(handle: u64) -> u16 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::UInt16(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_int32(handle: u64) -> i32 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Int32(v) | CValuePayload::Int(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_uint32(handle: u64) -> u32 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::UInt32(v) | CValuePayload::UInt(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_int64(handle: u64) -> i64 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Int64(v) | CValuePayload::Long(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_uint64(handle: u64) -> u64 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::UInt64(v) | CValuePayload::ULong(v) | CValuePayload::SizeT(v)) => v,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_float(handle: u64) -> f32 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Float(v)) => v,
        _ => 0.0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_double(handle: u64) -> f64 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Double(v)) => v,
        _ => 0.0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_get_bool(handle: u64) -> i32 {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::Bool(v)) => i32::from(v),
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_args_new() -> u64 {
    allocate_record(TYPE_ARGS, CValuePayload::Args(Vec::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_args_add(args_handle: u64, item_handle: u64) {
    args_add(args_handle, item_handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_args_count(args_handle: u64) -> i32 {
    i32::try_from(args_count(args_handle)).unwrap_or(i32::MAX)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_call(
    library: *const c_char,
    symbol: *const c_char,
    args_handle: u64,
    result_handle: u64,
) -> i32 {
    let lib_str = match crate::string::string_text(library) {
        Some(s) => s,
        None => {
            crate::set_error_report(
                ERROR_INVALID_ARGUMENT,
                "Call",
                "invalid null or non-UTF8 library name",
                "",
            );
            return ERROR_INVALID_ARGUMENT;
        }
    };
    let sym_str = match crate::string::string_text(symbol) {
        Some(s) => s,
        None => {
            crate::set_error_report(
                ERROR_INVALID_ARGUMENT,
                "Call",
                "invalid null or non-UTF8 symbol name",
                "",
            );
            return ERROR_INVALID_ARGUMENT;
        }
    };

    match call_c_function_with_args(&lib_str, &sym_str, &[], args_handle, result_handle) {
        Ok(()) => 0,
        Err(err) => {
            crate::set_error_report(
                err.code,
                "Call",
                &err.message,
                err.cause.as_deref().unwrap_or(""),
            );
            err.code
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_handle_result_value(handle: u64) -> *mut c_void {
    match get_record(handle).map(|r| r.payload) {
        Some(CValuePayload::HandleResult(Some(ptr))) => {
            let h = allocate_record(super::ffi::TYPE_HANDLE, CValuePayload::Handle(ptr));
            h as usize as *mut c_void
        }
        _ => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_handle_read_string(handle: u64, out_ptr: *mut *mut c_char) -> i32 {
    if out_ptr.is_null() {
        return ERROR_INVALID_ARGUMENT;
    }
    // SAFETY: out_ptr verified non-null.
    unsafe { *out_ptr = std::ptr::null_mut() };

    let ptr = match get_handle_ptr(handle) {
        Ok(p) => p,
        Err(e) => {
            crate::set_error_report(
                e.code,
                "ReadString",
                &e.message,
                e.cause.as_deref().unwrap_or(""),
            );
            return e.code;
        }
    };

    match read_c_string(ptr) {
        Ok(s) => {
            let str_ptr = super::string::c_string_from_bytes(s.as_bytes());
            if str_ptr.is_null() {
                crate::set_error_report(1, "ReadString", "out of memory", "");
                return 1;
            }
            // SAFETY: out_ptr verified non-null.
            unsafe { *out_ptr = str_ptr };
            0
        }
        Err(e) => {
            crate::set_error_report(
                e.code,
                "ReadString",
                &e.message,
                e.cause.as_deref().unwrap_or(""),
            );
            e.code
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_array_read_from(array_handle: u64, handle_h: u64) -> i32 {
    let ptr = match get_handle_ptr(handle_h) {
        Ok(p) => p as *mut c_void,
        Err(e) => {
            crate::set_error_report(
                e.code,
                "ReadFrom",
                &e.message,
                e.cause.as_deref().unwrap_or(""),
            );
            return e.code;
        }
    };

    match array_read_from(array_handle, ptr) {
        Ok(()) => 0,
        Err(e) => {
            crate::set_error_report(
                e.code,
                "ReadFrom",
                &e.message,
                e.cause.as_deref().unwrap_or(""),
            );
            e.code
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_ffi_array_write_to(array_handle: u64, handle_h: u64) -> i32 {
    let ptr = match get_handle_ptr(handle_h) {
        Ok(p) => p as *mut c_void,
        Err(e) => {
            crate::set_error_report(
                e.code,
                "WriteTo",
                &e.message,
                e.cause.as_deref().unwrap_or(""),
            );
            return e.code;
        }
    };

    match array_write_to(array_handle, ptr) {
        Ok(()) => 0,
        Err(e) => {
            crate::set_error_report(
                e.code,
                "WriteTo",
                &e.message,
                e.cause.as_deref().unwrap_or(""),
            );
            e.code
        }
    }
}
