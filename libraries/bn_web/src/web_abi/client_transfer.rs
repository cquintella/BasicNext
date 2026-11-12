// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The C ABI of the `BNWeb.Client` transfer: the one transfer every verb uses, the verbs without a body and `RequestMethod`.

use super::client_values::write_u64;
use super::{
    BN_WEB_OK, WebFailure, alloc_handle, c_char, log_client_access, record_failure, string_text,
    table,
};

// ---- Client Transfer C ABI -----------------------------------------------

pub(crate) fn do_client_transfer(
    operation: &str,
    client_handle: u64,
    method: &str,
    url: &str,
    body: &[u8],
    content_type: Option<&str>,
    headers_handle: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let wrapper = table();
    let (logger, headers) = {
        let guard = wrapper.lock_table();
        let logger = guard.clients.get(&client_handle).copied().unwrap_or(0);
        let headers = guard
            .headers
            .get(&headers_handle)
            .map(|headers| headers.entries.clone())
            .unwrap_or_default();
        (logger, headers)
    };
    let outcome = crate::web::Transfer {
        method,
        url,
        body,
        content_type,
        headers,
        maximum,
        policy: None,
    }
    .run();
    if let Some(access) = &outcome.access {
        log_client_access(logger, method, url, access);
    }
    match outcome.result {
        Ok(reply) => {
            let reply_handle = alloc_handle();
            wrapper
                .lock_table()
                .client_replies
                .insert(reply_handle, reply);
            write_u64(out_reply, reply_handle);
            BN_WEB_OK
        }
        Err(failure) => record_failure(operation, &failure),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    client_get("Client.Get", client_handle, url, 0, maximum, out_reply)
}

/// The four `GET` members; `operation` names the one called.
fn client_get(
    operation: &str,
    client_handle: u64,
    url: *const c_char,
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
    do_client_transfer(
        operation,
        client_handle,
        "GET",
        &url_str,
        &[],
        None,
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get_with_headers(
    client_handle: u64,
    url: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    client_get(
        "Client.GetWithHeaders",
        client_handle,
        url,
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get_decoded(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    client_get(
        "Client.GetDecoded",
        client_handle,
        url,
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get_decoded_with_headers(
    client_handle: u64,
    url: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    client_get(
        "Client.GetDecodedWithHeaders",
        client_handle,
        url,
        headers,
        maximum,
        out_reply,
    )
}

fn append_query(url: &str, query_str: &str) -> String {
    if query_str.is_empty() {
        return url.to_owned();
    }
    if url.contains('?') {
        format!("{url}&{query_str}")
    } else {
        format!("{url}?{query_str}")
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get_query(
    client_handle: u64,
    url: *const c_char,
    query: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.GetQuery",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let q_enc = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        guard
            .queries
            .get(&query)
            .map(crate::web::Query::encode)
            .unwrap_or_default()
    };
    let full_url = append_query(&url_str, &q_enc);
    do_client_transfer(
        "Client.GetQuery",
        client_handle,
        "GET",
        &full_url,
        &[],
        None,
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get_query_with_headers(
    client_handle: u64,
    url: *const c_char,
    query: u64,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.GetQueryWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let q_enc = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        guard
            .queries
            .get(&query)
            .map(crate::web::Query::encode)
            .unwrap_or_default()
    };
    let full_url = append_query(&url_str, &q_enc);
    do_client_transfer(
        "Client.GetQueryWithHeaders",
        client_handle,
        "GET",
        &full_url,
        &[],
        None,
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_head(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.Head",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        "Client.Head",
        client_handle,
        "HEAD",
        &url_str,
        &[],
        None,
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_head_with_headers(
    client_handle: u64,
    url: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.HeadWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        "Client.HeadWithHeaders",
        client_handle,
        "HEAD",
        &url_str,
        &[],
        None,
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_delete(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.Delete",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        "Client.Delete",
        client_handle,
        "DELETE",
        &url_str,
        &[],
        None,
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_delete_with_headers(
    client_handle: u64,
    url: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.DeleteWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        "Client.DeleteWithHeaders",
        client_handle,
        "DELETE",
        &url_str,
        &[],
        None,
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_options(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.Options",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        "Client.Options",
        client_handle,
        "OPTIONS",
        &url_str,
        &[],
        None,
        0,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_options_with_headers(
    client_handle: u64,
    url: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.OptionsWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        "Client.OptionsWithHeaders",
        client_handle,
        "OPTIONS",
        &url_str,
        &[],
        None,
        headers,
        maximum,
        out_reply,
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_request_method(
    client_handle: u64,
    method: *const c_char,
    url: *const c_char,
    body: *const c_char,
    content_type: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let (Some(method_str), Some(url_str)) = (string_text(method), string_text(url)) else {
        return record_failure(
            "Client.RequestMethod",
            &WebFailure::InvalidArgument("invalid_params".into()),
        );
    };
    let body_str = string_text(body).unwrap_or_default();
    let ct = string_text(content_type);
    do_client_transfer(
        "Client.RequestMethod",
        client_handle,
        &method_str,
        &url_str,
        body_str.as_bytes(),
        ct.as_deref(),
        headers,
        maximum,
        out_reply,
    )
}
