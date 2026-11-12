// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The C ABI of the `BNWeb.Client` verbs that send a body: `Post*`, `Put*` and `Patch*`.

use super::{WebFailure, c_char, record_failure, string_text, table};

use super::client_transfer::do_client_transfer;

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post(
    client_handle: u64,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(body_str)) = (string_text(url), string_text(body)) else {
        return record_failure(
            "Client.Post",
            &WebFailure::InvalidArgument("invalid_post".into()),
        );
    };
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.Post",
        client_handle,
        "POST",
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_with_headers(
    client_handle: u64,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(body_str)) = (string_text(url), string_text(body)) else {
        return record_failure(
            "Client.PostWithHeaders",
            &WebFailure::InvalidArgument("invalid_post".into()),
        );
    };
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PostWithHeaders",
        client_handle,
        "POST",
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_bytes(
    client_handle: u64,
    url: *const c_char,
    body_handle: u64,
    content_type: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PostBytes",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let bytes =
        bn_stdlib::crypto_abi::with_buffers(|b| b.get(&body_handle).map(|buf| buf.to_vec()))
            .unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PostBytes",
        client_handle,
        "POST",
        &url_str,
        &bytes,
        ct.as_deref(),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_bytes_with_headers(
    client_handle: u64,
    url: *const c_char,
    body_handle: u64,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PostBytesWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let bytes =
        bn_stdlib::crypto_abi::with_buffers(|b| b.get(&body_handle).map(|buf| buf.to_vec()))
            .unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PostBytesWithHeaders",
        client_handle,
        "POST",
        &url_str,
        &bytes,
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_fields(
    client_handle: u64,
    url: *const c_char,
    fields: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PostFields",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let enc = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        guard
            .fields
            .get(&fields)
            .map(crate::web::Fields::encode)
            .unwrap_or_default()
    };
    do_client_transfer(
        "Client.PostFields",
        client_handle,
        "POST",
        &url_str,
        enc.as_bytes(),
        Some("application/x-www-form-urlencoded"),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_fields_with_headers(
    client_handle: u64,
    url: *const c_char,
    fields: u64,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PostFieldsWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let enc = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        guard
            .fields
            .get(&fields)
            .map(crate::web::Fields::encode)
            .unwrap_or_default()
    };
    do_client_transfer(
        "Client.PostFieldsWithHeaders",
        client_handle,
        "POST",
        &url_str,
        enc.as_bytes(),
        Some("application/x-www-form-urlencoded"),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_form(
    client_handle: u64,
    url: *const c_char,
    form: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    client_post_form(
        "Client.PostForm",
        client_handle,
        url,
        form,
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_form_with_headers(
    client_handle: u64,
    url: *const c_char,
    form: u64,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    client_post_form(
        "Client.PostFormWithHeaders",
        client_handle,
        url,
        form,
        headers,
        maximum,
        out_reply,
    )
}

/// `PostForm` and `PostFormWithHeaders`; `operation` names the member called.
fn client_post_form(
    operation: &str,
    client_handle: u64,
    url: *const c_char,
    form: u64,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            operation,
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let (body, content_type) = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        guard
            .forms
            .get(&form)
            .cloned()
            .unwrap_or_default()
            .multipart()
    };
    do_client_transfer(
        operation,
        client_handle,
        "POST",
        &url_str,
        &body,
        Some(&content_type),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_json(
    client_handle: u64,
    url: *const c_char,
    json: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(json_str)) = (string_text(url), string_text(json)) else {
        return record_failure(
            "Client.PostJson",
            &WebFailure::InvalidArgument("invalid_post".into()),
        );
    };
    do_client_transfer(
        "Client.PostJson",
        client_handle,
        "POST",
        &url_str,
        json_str.as_bytes(),
        Some("application/json"),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_post_json_with_headers(
    client_handle: u64,
    url: *const c_char,
    json: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(json_str)) = (string_text(url), string_text(json)) else {
        return record_failure(
            "Client.PostJsonWithHeaders",
            &WebFailure::InvalidArgument("invalid_post".into()),
        );
    };
    do_client_transfer(
        "Client.PostJsonWithHeaders",
        client_handle,
        "POST",
        &url_str,
        json_str.as_bytes(),
        Some("application/json"),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_put(
    client_handle: u64,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(body_str)) = (string_text(url), string_text(body)) else {
        return record_failure(
            "Client.Put",
            &WebFailure::InvalidArgument("invalid_put".into()),
        );
    };
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.Put",
        client_handle,
        "PUT",
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_put_with_headers(
    client_handle: u64,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(body_str)) = (string_text(url), string_text(body)) else {
        return record_failure(
            "Client.PutWithHeaders",
            &WebFailure::InvalidArgument("invalid_put".into()),
        );
    };
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PutWithHeaders",
        client_handle,
        "PUT",
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_put_bytes(
    client_handle: u64,
    url: *const c_char,
    body_handle: u64,
    content_type: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PutBytes",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let bytes =
        bn_stdlib::crypto_abi::with_buffers(|b| b.get(&body_handle).map(|buf| buf.to_vec()))
            .unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PutBytes",
        client_handle,
        "PUT",
        &url_str,
        &bytes,
        ct.as_deref(),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_put_bytes_with_headers(
    client_handle: u64,
    url: *const c_char,
    body_handle: u64,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PutBytesWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let bytes =
        bn_stdlib::crypto_abi::with_buffers(|b| b.get(&body_handle).map(|buf| buf.to_vec()))
            .unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PutBytesWithHeaders",
        client_handle,
        "PUT",
        &url_str,
        &bytes,
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_patch(
    client_handle: u64,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(body_str)) = (string_text(url), string_text(body)) else {
        return record_failure(
            "Client.Patch",
            &WebFailure::InvalidArgument("invalid_patch".into()),
        );
    };
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.Patch",
        client_handle,
        "PATCH",
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_patch_with_headers(
    client_handle: u64,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(url_str), Some(body_str)) = (string_text(url), string_text(body)) else {
        return record_failure(
            "Client.PatchWithHeaders",
            &WebFailure::InvalidArgument("invalid_patch".into()),
        );
    };
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PatchWithHeaders",
        client_handle,
        "PATCH",
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_patch_bytes(
    client_handle: u64,
    url: *const c_char,
    body_handle: u64,
    content_type: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PatchBytes",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let bytes =
        bn_stdlib::crypto_abi::with_buffers(|b| b.get(&body_handle).map(|buf| buf.to_vec()))
            .unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PatchBytes",
        client_handle,
        "PATCH",
        &url_str,
        &bytes,
        ct.as_deref(),
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_patch_bytes_with_headers(
    client_handle: u64,
    url: *const c_char,
    body_handle: u64,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PatchBytesWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let bytes =
        bn_stdlib::crypto_abi::with_buffers(|b| b.get(&body_handle).map(|buf| buf.to_vec()))
            .unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.PatchBytesWithHeaders",
        client_handle,
        "PATCH",
        &url_str,
        &bytes,
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}
