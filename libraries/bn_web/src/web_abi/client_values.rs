// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The C ABI of the values the `BNWeb` client exchanges: `ClientReply`, `Headers`, `Query`, `Fields` and `Form`.

use super::{
    BN_WEB_FAILED, BN_WEB_OK, WebFailure, alloc_handle, c_char, c_string, record_failure,
    string_text, table,
};

// ---- ClientReply C ABI ---------------------------------------------------

fn write_status(out_status: *mut i32, status: i32) {
    if !out_status.is_null() {
        // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
        unsafe { *out_status = status };
    }
}

pub(super) fn write_u64(out: *mut u64, value: u64) {
    if !out.is_null() {
        // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
        unsafe { *out = value };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_create() -> u64 {
    let handle = alloc_handle();
    table()
        .lock_table()
        .client_replies
        .insert(handle, crate::web::ClientReply::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_status(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .client_replies
        .get(&handle)
        .map_or(0, |r| i64::from(r.status))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_http_version(handle: u64) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let version = guard
        .client_replies
        .get(&handle)
        .map_or("HTTP/1.1", |r| r.http_version());
    c_string(version)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_body_size(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .client_replies
        .get(&handle)
        .map_or(0, |r| i64::try_from(r.body_size).unwrap_or(i64::MAX))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_total_ms(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard.client_replies.get(&handle).map_or(0, |r| r.total_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_connect_ms(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .client_replies
        .get(&handle)
        .map_or(0, |r| r.connect_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_header(
    handle: u64,
    name: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(name_str) = string_text(name) else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure(
            "ClientReply.Header",
            &WebFailure::InvalidArgument("invalid_name".into()),
        );
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(reply) = guard.client_replies.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure(
            "ClientReply.Header",
            &WebFailure::InvalidArgument("invalid_reply".into()),
        );
        return std::ptr::null_mut();
    };
    if let Some(val) = reply.header(&name_str) {
        write_status(out_status, BN_WEB_OK);
        c_string(val)
    } else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure("ClientReply.Header", &WebFailure::HeaderNotFound(name_str));
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_header_count(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .client_replies
        .get(&handle)
        .map_or(0, |r| i64::try_from(r.header_count()).unwrap_or(0))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_header_name(
    handle: u64,
    index: i64,
    out_status: *mut i32,
) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(reply) = guard.client_replies.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    let idx = usize::try_from(index).unwrap_or(usize::MAX);
    if let Some(name) = reply.header_name(idx) {
        write_status(out_status, BN_WEB_OK);
        c_string(name)
    } else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure(
            "ClientReply.HeaderName",
            &WebFailure::IndexOutOfRange {
                index: i128::from(index),
                count: reply.header_count(),
            },
        );
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_header_value(
    handle: u64,
    index: i64,
    out_status: *mut i32,
) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(reply) = guard.client_replies.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    let idx = usize::try_from(index).unwrap_or(usize::MAX);
    if let Some(val) = reply.header_value(idx) {
        write_status(out_status, BN_WEB_OK);
        c_string(val)
    } else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure(
            "ClientReply.HeaderValue",
            &WebFailure::IndexOutOfRange {
                index: i128::from(index),
                count: reply.header_count(),
            },
        );
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_body_text(
    handle: u64,
    maximum: i64,
    out_status: *mut i32,
) -> *mut c_char {
    let wrapper = table();
    let reply = {
        let guard = wrapper.lock_table();
        let Some(r) = guard.client_replies.get(&handle) else {
            write_status(out_status, BN_WEB_FAILED);
            return std::ptr::null_mut();
        };
        r.clone()
    };
    let max = usize::try_from(maximum).unwrap_or(usize::MAX);
    match reply.body_text(max) {
        Ok(text) => {
            write_status(out_status, BN_WEB_OK);
            c_string(&text)
        }
        Err(err) => {
            write_status(out_status, BN_WEB_FAILED);
            record_failure("ClientReply.BodyText", &err);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_body_bytes(
    handle: u64,
    maximum: i64,
    out_handle: *mut u64,
) -> i32 {
    let wrapper = table();
    let reply = {
        let guard = wrapper.lock_table();
        let Some(r) = guard.client_replies.get(&handle) else {
            return record_failure(
                "ClientReply.BodyBytes",
                &WebFailure::InvalidArgument("invalid_reply".into()),
            );
        };
        r.clone()
    };
    let max = usize::try_from(maximum).unwrap_or(usize::MAX);
    match reply.body_bytes(max) {
        Ok(bytes) => {
            let byte_handle = bn_stdlib::crypto_abi::store(bytes.to_vec());
            write_u64(out_handle, byte_handle);
            BN_WEB_OK
        }
        Err(err) => record_failure("ClientReply.BodyBytes", &err),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_body_decoded_text(
    handle: u64,
    maximum: i64,
    out_status: *mut i32,
) -> *mut c_char {
    let wrapper = table();
    let reply = {
        let guard = wrapper.lock_table();
        let Some(r) = guard.client_replies.get(&handle) else {
            write_status(out_status, BN_WEB_FAILED);
            return std::ptr::null_mut();
        };
        r.clone()
    };
    let max = usize::try_from(maximum).unwrap_or(usize::MAX);
    match reply.body_decoded_text(max) {
        Ok(text) => {
            write_status(out_status, BN_WEB_OK);
            c_string(&text)
        }
        Err(err) => {
            write_status(out_status, BN_WEB_FAILED);
            record_failure("ClientReply.BodyDecodedText", &err);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_body_decoded_bytes(
    handle: u64,
    maximum: i64,
    out_handle: *mut u64,
) -> i32 {
    let wrapper = table();
    let reply = {
        let guard = wrapper.lock_table();
        let Some(r) = guard.client_replies.get(&handle) else {
            return record_failure(
                "ClientReply.BodyDecodedBytes",
                &WebFailure::InvalidArgument("invalid_reply".into()),
            );
        };
        r.clone()
    };
    let max = usize::try_from(maximum).unwrap_or(usize::MAX);
    match reply.body_decoded_bytes(max) {
        Ok(bytes) => {
            let byte_handle = bn_stdlib::crypto_abi::store(bytes);
            write_u64(out_handle, byte_handle);
            BN_WEB_OK
        }
        Err(err) => record_failure("ClientReply.BodyDecodedBytes", &err),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_reply_peer_address(
    handle: u64,
    out_status: *mut i32,
) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(reply) = guard.client_replies.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    if let Some(addr) = reply.peer_address() {
        write_status(out_status, BN_WEB_OK);
        c_string(&addr.to_string())
    } else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure(
            "ClientReply.PeerAddress",
            &WebFailure::NotFound("peer_address_not_found".into()),
        );
        std::ptr::null_mut()
    }
}

// ---- Headers C ABI -------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_headers_create() -> u64 {
    let handle = alloc_handle();
    table()
        .lock_table()
        .headers
        .insert(handle, crate::web::Headers::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_headers_new(out_handle: *mut u64) -> i32 {
    let handle = bn_web_headers_create();
    write_u64(out_handle, handle);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_headers_add(handle: u64, name: *const c_char, val: *const c_char) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "Headers.Add",
            &WebFailure::InvalidArgument("invalid_header".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(headers) = guard.headers.get_mut(&handle) else {
        return record_failure(
            "Headers.Add",
            &WebFailure::InvalidArgument("invalid_headers_handle".into()),
        );
    };
    headers.add(name_str, val_str);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_headers_set(handle: u64, name: *const c_char, val: *const c_char) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "Headers.Set",
            &WebFailure::InvalidArgument("invalid_header".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(headers) = guard.headers.get_mut(&handle) else {
        return record_failure(
            "Headers.Set",
            &WebFailure::InvalidArgument("invalid_headers_handle".into()),
        );
    };
    headers.set(name_str, val_str);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_headers_count(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .headers
        .get(&handle)
        .map_or(0, |h| i64::try_from(h.count()).unwrap_or(0))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_headers_get(
    handle: u64,
    name: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(name_str) = string_text(name) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(headers) = guard.headers.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    if let Some(val) = headers.get(&name_str) {
        write_status(out_status, BN_WEB_OK);
        c_string(val)
    } else {
        write_status(out_status, BN_WEB_FAILED);
        record_failure("Headers.Get", &WebFailure::HeaderNotFound(name_str));
        std::ptr::null_mut()
    }
}

// ---- Query C ABI ---------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_query_create() -> u64 {
    let handle = alloc_handle();
    table()
        .lock_table()
        .queries
        .insert(handle, crate::web::Query::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_query_new(out_handle: *mut u64) -> i32 {
    let handle = bn_web_query_create();
    write_u64(out_handle, handle);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_query_add(handle: u64, name: *const c_char, val: *const c_char) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "Query.Add",
            &WebFailure::InvalidArgument("invalid_query".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(query) = guard.queries.get_mut(&handle) else {
        return record_failure(
            "Query.Add",
            &WebFailure::InvalidArgument("invalid_query_handle".into()),
        );
    };
    query.add(name_str, val_str);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_query_encode(handle: u64, out_status: *mut i32) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(query) = guard.queries.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    let enc = query.encode();
    write_status(out_status, BN_WEB_OK);
    c_string(&enc)
}

// ---- Fields C ABI --------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_fields_create() -> u64 {
    let handle = alloc_handle();
    table()
        .lock_table()
        .fields
        .insert(handle, crate::web::Fields::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_fields_new(out_handle: *mut u64) -> i32 {
    let handle = bn_web_fields_create();
    write_u64(out_handle, handle);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_fields_add(handle: u64, name: *const c_char, val: *const c_char) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "Fields.Add",
            &WebFailure::InvalidArgument("invalid_field".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(fields) = guard.fields.get_mut(&handle) else {
        return record_failure(
            "Fields.Add",
            &WebFailure::InvalidArgument("invalid_fields_handle".into()),
        );
    };
    fields.add(name_str, val_str);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_fields_encode(handle: u64, out_status: *mut i32) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(fields) = guard.fields.get(&handle) else {
        write_status(out_status, BN_WEB_FAILED);
        return std::ptr::null_mut();
    };
    let enc = fields.encode();
    write_status(out_status, BN_WEB_OK);
    c_string(&enc)
}

// ---- Form C ABI ----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_form_create() -> u64 {
    let handle = alloc_handle();
    table()
        .lock_table()
        .forms
        .insert(handle, crate::web::Form::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_form_new(out_handle: *mut u64) -> i32 {
    let handle = bn_web_form_create();
    write_u64(out_handle, handle);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_form_add_field(
    handle: u64,
    name: *const c_char,
    val: *const c_char,
) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "Form.AddField",
            &WebFailure::InvalidArgument("invalid_field".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(form) = guard.forms.get_mut(&handle) else {
        return record_failure(
            "Form.AddField",
            &WebFailure::InvalidArgument("invalid_form_handle".into()),
        );
    };
    form.add_field(name_str, val_str);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_form_add_file(
    handle: u64,
    name: *const c_char,
    path: *const c_char,
    content_type: *const c_char,
) -> i32 {
    let (Some(name_str), Some(path_str)) = (string_text(name), string_text(path)) else {
        return record_failure(
            "Form.AddFile",
            &WebFailure::InvalidArgument("invalid_file_params".into()),
        );
    };
    let ct_str = string_text(content_type).unwrap_or_else(|| "application/octet-stream".into());
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(form) = guard.forms.get_mut(&handle) else {
        return record_failure(
            "Form.AddFile",
            &WebFailure::InvalidArgument("invalid_form_handle".into()),
        );
    };
    form.add_file(name_str, path_str, ct_str);
    BN_WEB_OK
}
