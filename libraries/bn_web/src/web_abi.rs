// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(unsafe_code)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::too_many_arguments)]

//! C ABI for `BNWeb` compiled code (`bnc`).
//!
//! Provides `bn_web_*` symbols linked when native executables use `BNWeb`.
//! Handles are thread/task isolated through `bn_core_copy`. Error reports
//! are recorded for `bn_rt_error_take` via `bn_rt_error_set_report`.

use std::collections::HashMap;
use std::ffi::c_char;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use crate::web_error::WebFailure;

// SAFETY: the foreign signature matches the C or system function this module calls.
unsafe extern "C" {
    fn bn_rt_error_set_report(
        code: i32,
        operation: *const c_char,
        message: *const c_char,
        cause: *const c_char,
    );
    fn bn_rt_log_client_access(
        handle: u64,
        method: *const c_char,
        url: *const c_char,
        status: i64,
        bytes: i64,
        duration_ms: i64,
        headers: *const c_char,
    ) -> i32;
}

// ---- Native String Marshalling -------------------------------------------

// One STRING layout for the whole runtime: `bn_rt::string` owns the header,
// the reader's checks and the allocator; this module does not copy them.

pub(crate) use bn_rt::string::c_string_from_bytes;

/// A STRING payload as owned text. Null, any other pointer and invalid UTF-8
/// are `None`: the reader never guesses a C string.
pub(crate) fn string_text(ptr: *const c_char) -> Option<String> {
    bn_rt::string::string_text(ptr)
}

pub(crate) fn c_string(text: &str) -> *mut c_char {
    c_string_from_bytes(text.as_bytes())
}

// ---- Error Recording -----------------------------------------------------

pub(crate) const BN_WEB_OK: i32 = 0;
pub(crate) const BN_WEB_FAILED: i32 = 1;

pub(crate) fn record_failure(operation: &str, failure: &WebFailure) -> i32 {
    let mut parts = operation.rsplit('.');
    let method = parts.next().unwrap_or_default();
    let class = parts.next().unwrap_or_default();
    let op_name = if class.is_empty() || class == "BNWeb" {
        format!("BNWeb.{method}\0")
    } else {
        format!("BNWeb.{class}.{method}\0")
    };
    let msg = format!("{}\0", failure.message());
    let cause = format!("{}\0", failure.cause());
    // SAFETY: the three strings were just built with a trailing NUL and live for this call.
    unsafe {
        bn_rt_error_set_report(
            failure.code(),
            op_name.as_ptr().cast(),
            msg.as_ptr().cast(),
            cause.as_ptr().cast(),
        );
    }
    BN_WEB_FAILED
}

// ---- State Table ---------------------------------------------------------

pub(crate) struct ServerEntry {
    pub(crate) state: Arc<Mutex<crate::web::ServerState>>,
    pub(crate) filters: Vec<extern "C" fn(u64, u64)>,
    pub(crate) routes: Vec<(String, String, extern "C" fn(u64, u64))>,
}

#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct WebTable {
    pub(crate) servers: crate::counted::Counted<ServerEntry>,
    pub(crate) server_options: crate::counted::Counted<crate::web::ServerOptions>,
    pub(crate) tls_configs: crate::counted::Counted<Arc<rustls::ServerConfig>>,
    pub(crate) cookie_jars: crate::counted::Counted<crate::web_state::CookieJar>,
    pub(crate) session_stores: crate::counted::Counted<crate::web_state::SessionStore>,
    pub(crate) scrapers: crate::counted::Counted<crate::web_state::Scraper>,
    pub(crate) acls: crate::counted::Counted<crate::web_state::Acl>,
    pub(crate) egress_policies: crate::counted::Counted<crate::web::EgressPolicy>,
    pub(crate) responses: crate::counted::Counted<crate::web::Response>,
    pub(crate) requests: crate::counted::Counted<crate::web::Request>,
    pub(crate) values: HashMap<u64, Vec<String>>,
    pub(crate) request_sessions: HashMap<u64, crate::web::sessions::RequestSession>,
    /// Logger handle for each client; `0` means the client does not log.
    pub(crate) clients: crate::counted::Counted<u64>,
    pub(crate) client_replies: crate::counted::Counted<crate::web::ClientReply>,
    pub(crate) headers: crate::counted::Counted<crate::web::Headers>,
    pub(crate) queries: crate::counted::Counted<crate::web::Query>,
    pub(crate) fields: crate::counted::Counted<crate::web::Fields>,
    pub(crate) forms: crate::counted::Counted<crate::web::Form>,
    pub(crate) next_id: u64,
}

macro_rules! counted_abi {
    ($table:ident, $retain_fn:ident, $release_fn:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $retain_fn(handle: u64) -> i32 {
            if handle == 0 {
                return BN_WEB_OK;
            }
            let wrapper = table();
            let mut guard = wrapper.lock_table();
            if guard.$table.retain(handle) {
                BN_WEB_OK
            } else {
                BN_WEB_FAILED
            }
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn $release_fn(handle: u64) -> i32 {
            if handle == 0 {
                return BN_WEB_OK;
            }
            let wrapper = table();
            let mut guard = wrapper.lock_table();
            match guard.$table.release(handle) {
                crate::counted::Release::Unknown => BN_WEB_FAILED,
                _ => BN_WEB_OK,
            }
        }
    };
}

counted_abi!(servers, bn_rt_web_server_retain, bn_rt_web_server_release);
counted_abi!(
    server_options,
    bn_rt_web_server_options_retain,
    bn_rt_web_server_options_release
);
counted_abi!(
    tls_configs,
    bn_rt_web_tls_config_retain,
    bn_rt_web_tls_config_release
);
counted_abi!(
    cookie_jars,
    bn_rt_web_cookie_jar_retain,
    bn_rt_web_cookie_jar_release
);
counted_abi!(
    session_stores,
    bn_rt_web_session_store_retain,
    bn_rt_web_session_store_release
);
counted_abi!(
    scrapers,
    bn_rt_web_scraper_retain,
    bn_rt_web_scraper_release
);
counted_abi!(acls, bn_rt_web_acl_retain, bn_rt_web_acl_release);
counted_abi!(
    egress_policies,
    bn_rt_web_egress_policy_retain,
    bn_rt_web_egress_policy_release
);
counted_abi!(
    responses,
    bn_rt_web_response_retain,
    bn_rt_web_response_release
);
counted_abi!(
    requests,
    bn_rt_web_request_retain,
    bn_rt_web_request_release
);
counted_abi!(clients, bn_rt_web_client_retain, bn_rt_web_client_release);
counted_abi!(
    client_replies,
    bn_rt_web_client_reply_retain,
    bn_rt_web_client_reply_release
);
counted_abi!(headers, bn_rt_web_headers_retain, bn_rt_web_headers_release);
counted_abi!(queries, bn_rt_web_query_retain, bn_rt_web_query_release);
counted_abi!(fields, bn_rt_web_fields_retain, bn_rt_web_fields_release);
counted_abi!(forms, bn_rt_web_form_retain, bn_rt_web_form_release);

pub(crate) struct WebTableWrapper(pub(crate) Mutex<WebTable>);

impl Default for WebTableWrapper {
    fn default() -> Self {
        Self(Mutex::new(WebTable {
            next_id: 1,
            ..Default::default()
        }))
    }
}

impl WebTableWrapper {
    pub(crate) fn lock_table(&self) -> std::sync::MutexGuard<'_, WebTable> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub(crate) fn table() -> Arc<WebTableWrapper> {
    bn_core_copy::table::<WebTableWrapper>()
}

pub(crate) fn alloc_handle() -> u64 {
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard.next_id = guard.next_id.wrapping_add(1).max(1);
    guard.next_id
}

// ---- Response C ABI ------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard
        .responses
        .insert(handle, crate::web::Response::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_set_status(handle: u64, status: i64) -> i32 {
    let Ok(u16_status) = u16::try_from(status) else {
        return record_failure(
            "Response.SetStatus",
            &WebFailure::InvalidArgument("status code out of range".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(response) = guard.responses.get_mut(&handle) else {
        return record_failure(
            "Response.SetStatus",
            &WebFailure::Closed("response handle closed".into()),
        );
    };
    if response.is_committed() {
        return record_failure("Response.SetStatus", &WebFailure::ResponseCommittedOrClosed);
    }
    match response.set_status(u16_status) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Response.SetStatus", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_status(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .responses
        .get(&handle)
        .map_or(200, |r| r.status.into())
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_set_header(
    handle: u64,
    name: *const c_char,
    value: *const c_char,
) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(value)) else {
        return record_failure(
            "Response.SetHeader",
            &WebFailure::InvalidArgument("invalid string".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(response) = guard.responses.get_mut(&handle) else {
        return record_failure(
            "Response.SetHeader",
            &WebFailure::Closed("response handle closed".into()),
        );
    };
    if response.is_committed() {
        return record_failure("Response.SetHeader", &WebFailure::ResponseCommittedOrClosed);
    }
    match response.set_header(&name_str, &val_str) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Response.SetHeader", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_header(
    handle: u64,
    name: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(name_str) = string_text(name) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Response.Header",
                    &WebFailure::InvalidArgument("invalid header name".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(response) = guard.responses.get(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Response.Header",
                    &WebFailure::Closed("response closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    if let Some(val) = response.header(&name_str) {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { *out_status = BN_WEB_OK };
        }
        c_string(val)
    } else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Response.Header",
                    &WebFailure::from_message("response header is not present"),
                );
            };
        }
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_set_cookie(
    handle: u64,
    name: *const c_char,
    val: *const c_char,
    domain: *const c_char,
    path: *const c_char,
    max_age_ms: i64,
) -> i32 {
    bn_web_response_set_cookie_with_policy(
        handle,
        name,
        val,
        domain,
        path,
        max_age_ms,
        false,
        false,
        std::ptr::null(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_set_cookie_with_policy(
    handle: u64,
    name: *const c_char,
    val: *const c_char,
    domain: *const c_char,
    path: *const c_char,
    max_age_ms: i64,
    secure: bool,
    http_only: bool,
    same_site: *const c_char,
) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "Response.SetCookie",
            &WebFailure::InvalidArgument("invalid cookie name or value".into()),
        );
    };
    let domain_str = string_text(domain).unwrap_or_default();
    let path_str = string_text(path).unwrap_or_default();
    let same_site_str = string_text(same_site).unwrap_or_else(|| "Lax".to_string());

    let options =
        match crate::web_state::CookieOptions::from_policy(secure, http_only, &same_site_str) {
            Ok(opt) => opt,
            Err(err) => {
                return record_failure("Response.SetCookie", &WebFailure::from_message(err));
            }
        };
    let max_age = match crate::web_state::cookie_max_age(max_age_ms.into()) {
        Ok(age) => (!age.is_zero()).then_some(age),
        Err(err) => return record_failure("Response.SetCookie", &WebFailure::from_message(err)),
    };
    let domain_opt = (!domain_str.is_empty()).then_some(domain_str.as_str());

    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(response) = guard.responses.get_mut(&handle) else {
        return record_failure(
            "Response.SetCookie",
            &WebFailure::Closed("response closed".into()),
        );
    };
    if response.is_committed() {
        return record_failure("Response.SetCookie", &WebFailure::ResponseCommittedOrClosed);
    }
    match response.set_cookie(&name_str, &val_str, domain_opt, &path_str, max_age, options) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Response.SetCookie", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_clear_cookie(
    handle: u64,
    name: *const c_char,
    domain: *const c_char,
    path: *const c_char,
) -> i32 {
    let Some(name_str) = string_text(name) else {
        return record_failure(
            "Response.ClearCookie",
            &WebFailure::InvalidArgument("invalid cookie name".into()),
        );
    };
    let domain_str = string_text(domain).unwrap_or_default();
    let path_str = string_text(path).unwrap_or_default();
    let domain_opt = (!domain_str.is_empty()).then_some(domain_str.as_str());

    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(response) = guard.responses.get_mut(&handle) else {
        return record_failure(
            "Response.ClearCookie",
            &WebFailure::Closed("response closed".into()),
        );
    };
    if response.is_committed() {
        return record_failure(
            "Response.ClearCookie",
            &WebFailure::ResponseCommittedOrClosed,
        );
    }
    match response.clear_cookie(&name_str, domain_opt, &path_str) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Response.ClearCookie", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_write(handle: u64, body: *const c_char) -> i32 {
    let body_str = string_text(body).unwrap_or_default();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(response) = guard.responses.get_mut(&handle) else {
        return record_failure(
            "Response.Write",
            &WebFailure::Closed("response closed".into()),
        );
    };
    if response.is_committed() {
        return record_failure("Response.Write", &WebFailure::ResponseCommittedOrClosed);
    }
    match response.write(&body_str) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Response.Write", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_commit(handle: u64) -> i32 {
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(response) = guard.responses.get_mut(&handle) else {
        return record_failure(
            "Response.Commit",
            &WebFailure::Closed("response closed".into()),
        );
    };
    if response.is_committed() {
        return record_failure("Response.Commit", &WebFailure::ResponseCommittedOrClosed);
    }
    match response.commit() {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Response.Commit", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_is_committed(handle: u64) -> i32 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    // `i32`, not `bool`: the caller reads the full register, and a C `bool`
    // leaves the upper bits undefined on x86-64.
    i32::from(
        guard
            .responses
            .get(&handle)
            .is_some_and(super::web::Response::is_committed),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_close(handle: u64) -> i32 {
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    if let Some(mut response) = guard.responses.remove(&handle) {
        let () = response.close();
    }
    BN_WEB_OK
}

// ---- Request C ABI -------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard
        .requests
        .insert(handle, crate::web::Request::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_method(handle: u64) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .requests
        .get(&handle)
        .map_or_else(|| c_string("GET"), |r| c_string(&r.method))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_target(handle: u64) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .requests
        .get(&handle)
        .map_or_else(|| c_string("/"), |r| c_string(&r.target.path))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_header(
    handle: u64,
    name: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(name_str) = string_text(name) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Request.Header",
                    &WebFailure::InvalidArgument("invalid header name".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(request) = guard.requests.get(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Request.Header",
                    &WebFailure::Closed("request closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    match request.header(&name_str) {
        Ok(vals) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_status = BN_WEB_OK };
            }
            c_string(vals.get(0).unwrap_or(""))
        }
        Err(err) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe {
                    *out_status = record_failure("Request.Header", &WebFailure::from_message(err));
                };
            }
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_param(
    handle: u64,
    name: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(name_str) = string_text(name) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Request.Param",
                    &WebFailure::InvalidArgument("invalid param name".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(request) = guard.requests.get(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Request.Param",
                    &WebFailure::Closed("request closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    if let Some(val) = request.param(&name_str) {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { *out_status = BN_WEB_OK };
        }
        c_string(val)
    } else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure("Request.Param", &WebFailure::RouteParameterNotFound);
            };
        }
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_cookie(
    handle: u64,
    name: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(name_str) = string_text(name) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Request.Cookie",
                    &WebFailure::InvalidArgument("invalid cookie name".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(request) = guard.requests.get(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Request.Cookie",
                    &WebFailure::Closed("request closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    if let Some(val) = request.cookie(&name_str) {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { *out_status = BN_WEB_OK };
        }
        c_string(val)
    } else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure("Request.Cookie", &WebFailure::RequestCookieNotFound);
            };
        }
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_body(
    handle: u64,
    max_bytes: i64,
    out_status: *mut i32,
) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(request) = guard.requests.get(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status =
                    record_failure("Request.Body", &WebFailure::Closed("request closed".into()));
            };
        }
        return std::ptr::null_mut();
    };
    match request.body(i128::from(max_bytes)) {
        Ok(text) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_status = BN_WEB_OK };
            }
            c_string(text)
        }
        Err(err) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe {
                    *out_status = record_failure("Request.Body", &WebFailure::from_message(err));
                };
            }
            std::ptr::null_mut()
        }
    }
}

// ---- Server C ABI --------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard.servers.insert(
        handle,
        ServerEntry {
            state: Arc::new(Mutex::new(crate::web::ServerState::new())),
            filters: Vec::new(),
            routes: Vec::new(),
        },
    );
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_add_filter(handle: u64, filter_fn: extern "C" fn(u64, u64)) -> i32 {
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(entry) = guard.servers.get_mut(&handle) else {
        return record_failure(
            "Server.AddFilter",
            &WebFailure::Closed("server closed".into()),
        );
    };
    if entry.filters.len() >= 64 {
        return record_failure("Server.AddFilter", &WebFailure::FilterLimitExceeded);
    }
    entry.filters.push(filter_fn);
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_route(
    handle: u64,
    method: *const c_char,
    pattern: *const c_char,
    handler_fn: extern "C" fn(u64, u64),
) -> i32 {
    let (Some(method_str), Some(pattern_str)) = (string_text(method), string_text(pattern)) else {
        return record_failure(
            "Server.Route",
            &WebFailure::InvalidArgument("invalid method or pattern".into()),
        );
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(entry) = guard.servers.get_mut(&handle) else {
        return record_failure("Server.Route", &WebFailure::Closed("server closed".into()));
    };
    let mut state = entry
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match state.add_route(method_str.clone(), pattern_str.clone()) {
        Ok(()) => {
            entry.routes.push((method_str, pattern_str, handler_fn));
            BN_WEB_OK
        }
        Err(err) => record_failure("Server.Route", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_static(
    handle: u64,
    prefix: *const c_char,
    directory: *const c_char,
) -> i32 {
    let (Some(prefix_str), Some(dir_str)) = (string_text(prefix), string_text(directory)) else {
        return record_failure(
            "Server.Static",
            &WebFailure::InvalidArgument("invalid prefix or directory".into()),
        );
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(entry) = guard.servers.get(&handle) else {
        return record_failure("Server.Static", &WebFailure::Closed("server closed".into()));
    };
    let mut state = entry
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match state.add_static(&prefix_str, &dir_str) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.Static", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_use_sessions(handle: u64, capacity: i64, idle_ms: i64) -> i32 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(entry) = guard.servers.get(&handle) else {
        return record_failure(
            "Server.UseSessions",
            &WebFailure::Closed("server closed".into()),
        );
    };
    let mut state = entry
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match state.use_sessions(i128::from(capacity), i128::from(idle_ms)) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.UseSessions", &WebFailure::from_message(err)),
    }
}

#[allow(clippy::too_many_lines)]
fn start_server_internal(
    handle: u64,
    addr_ptr: *const c_char,
    port: i32,
    tls_handle: Option<u64>,
    options_handle: Option<u64>,
    operation: &str,
) -> i32 {
    let (filters, routes, state_arc, tls_config, options) = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure(operation, &WebFailure::Closed("server closed".into()));
        };
        let options = if let Some(opt_h) = options_handle {
            match guard.server_options.get(&opt_h) {
                Some(opts) => opts.clone(),
                None => {
                    return record_failure(
                        operation,
                        &WebFailure::InvalidArgument(
                            bn_catalog::text("web", "server.options_not_live").into(),
                        ),
                    );
                }
            }
        } else {
            crate::web::ServerOptions::default()
        };
        let tls_config = if let Some(tls_h) = tls_handle {
            match guard.tls_configs.get(&tls_h) {
                Some(cfg) => Some(Arc::clone(cfg)),
                None => {
                    return record_failure(
                        operation,
                        &WebFailure::InvalidArgument(
                            bn_catalog::text("web", "server.tls_config_not_live").into(),
                        ),
                    );
                }
            }
        } else {
            None
        };
        (
            entry.filters.clone(),
            entry.routes.clone(),
            Arc::clone(&entry.state),
            tls_config,
            options,
        )
    };

    if let Err(message) = options.validate() {
        return record_failure(operation, &WebFailure::from_message(message));
    }

    let Some(address_text) = string_text(addr_ptr) else {
        return record_failure(
            operation,
            &WebFailure::InvalidArgument(bn_catalog::text("web", "server.invalid_address").into()),
        );
    };
    let Ok(address) = bn_core_net::Address::parse(&address_text) else {
        return record_failure(
            operation,
            &WebFailure::InvalidArgument(bn_catalog::text("web", "server.invalid_address").into()),
        );
    };
    let Ok(port_u16) = u16::try_from(port) else {
        return record_failure(
            operation,
            &WebFailure::InvalidArgument(
                bn_catalog::text("web", "server.port_out_of_range").into(),
            ),
        );
    };
    let endpoint = bn_core_net::Endpoint::new(address, port_u16);

    let listener = match bn_core_net::TcpListener::bind_with_backlog(endpoint, options.backlog) {
        Ok(listener) => listener,
        Err(error) => {
            return record_failure(operation, &WebFailure::from_message(&error.to_string()));
        }
    };

    {
        let mut state = state_arc
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Err(message) = state.start_with_options(options) {
            return record_failure(operation, &WebFailure::from_message(message));
        }
        if let Err(message) = state.install_worker_pool() {
            return record_failure(operation, &WebFailure::from_message(message));
        }
    }

    let handler_state = Arc::clone(&state_arc);
    let request_handler: crate::http::Handler = Arc::new(move |req, resp| {
        let req_handle = alloc_handle();
        let resp_handle = alloc_handle();
        {
            let wrapper = table();
            let mut guard = wrapper.lock_table();
            guard.requests.insert(req_handle, req.clone());
            guard.responses.insert(resp_handle, resp.clone());
        }

        for filter in &filters {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                filter(req_handle, resp_handle);
            }));
            let committed = {
                let wrapper = table();
                let guard = wrapper.lock_table();
                guard
                    .responses
                    .get(&resp_handle)
                    .is_some_and(crate::web::Response::is_committed)
            };
            if committed {
                break;
            }
        }

        let is_committed = {
            let wrapper = table();
            let guard = wrapper.lock_table();
            guard
                .responses
                .get(&resp_handle)
                .is_some_and(crate::web::Response::is_committed)
        };

        if !is_committed {
            let matched_pattern = {
                let state = handler_state.lock().ok();
                state.and_then(|s| s.matched_route_pattern(&req.method, &req.target.path))
            };
            if let Some(pattern) = matched_pattern {
                let handler_fn = routes
                    .iter()
                    .find(|(m, p, _)| m == &req.method && p == &pattern)
                    .map(|(_, _, f)| *f);
                if let Some(handler_fn) = handler_fn {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handler_fn(req_handle, resp_handle);
                    }));
                    if outcome.is_err() {
                        let wrapper = table();
                        let mut guard = wrapper.lock_table();
                        if let Some(r) = guard.responses.get_mut(&resp_handle) {
                            let _ = r.set_status(500);
                        }
                    }
                } else {
                    let wrapper = table();
                    let mut guard = wrapper.lock_table();
                    if let Some(r) = guard.responses.get_mut(&resp_handle) {
                        let _ = r.set_status(404);
                    }
                }
            }
        }

        let wrapper = table();
        let mut guard = wrapper.lock_table();
        if let Some(modified_resp) = guard.responses.remove(&resp_handle) {
            *resp = modified_resp;
        }
        guard.requests.remove(&req_handle);
        guard.values.remove(&req_handle);
        guard.values.remove(&resp_handle);

        Ok(())
    });

    if let Err(message) =
        crate::web::spawn_listener_thread(&state_arc, listener, request_handler, tls_config)
    {
        return record_failure(operation, &WebFailure::from_message(message));
    }

    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_start(handle: u64, addr_ptr: *const c_char, port: i32) -> i32 {
    start_server_internal(handle, addr_ptr, port, None, None, "Server.Start")
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_start_with_options(
    handle: u64,
    addr_ptr: *const c_char,
    port: i32,
    options_handle: u64,
) -> i32 {
    start_server_internal(
        handle,
        addr_ptr,
        port,
        None,
        Some(options_handle),
        "Server.StartWithOptions",
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_start_tls(
    handle: u64,
    addr_ptr: *const c_char,
    port: i32,
    tls_handle: u64,
) -> i32 {
    start_server_internal(
        handle,
        addr_ptr,
        port,
        Some(tls_handle),
        None,
        "Server.StartTLS",
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_start_tls_with_options(
    handle: u64,
    addr_ptr: *const c_char,
    port: i32,
    tls_handle: u64,
    options_handle: u64,
) -> i32 {
    start_server_internal(
        handle,
        addr_ptr,
        port,
        Some(tls_handle),
        Some(options_handle),
        "Server.StartTLSWithOptions",
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_dispatch(handle: u64, req_handle: u64, resp_handle: u64) -> i32 {
    let wrapper = table();
    let (filters, routes, state_arc) = {
        let guard = wrapper.lock_table();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure(
                "Server.Dispatch",
                &WebFailure::Closed("server closed".into()),
            );
        };
        (
            entry.filters.clone(),
            entry.routes.clone(),
            std::sync::Arc::clone(&entry.state),
        )
    };

    let (method, target) = {
        let guard = wrapper.lock_table();
        let Some(req) = guard.requests.get(&req_handle) else {
            return record_failure(
                "Server.Dispatch",
                &WebFailure::Closed("request closed".into()),
            );
        };
        (req.method.clone(), req.target.path.clone())
    };

    // Dispatch via state to check acceptance and match route
    let matched = {
        let mut state = state_arc
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut res = None;
        if state
            .dispatch(&method, &target, |outcome| {
                res = Some(match outcome {
                    crate::web::RouteOutcome::Matched(route, found) => {
                        Ok((route.pattern().to_owned(), found))
                    }
                    crate::web::RouteOutcome::MethodNotAllowed(_) => Err(405),
                    crate::web::RouteOutcome::NotFound => Err(404),
                });
            })
            .is_err()
        {
            return record_failure("Server.Dispatch", &WebFailure::ServerNotAccepting);
        }
        res.unwrap_or(Err(500))
    };

    let (pattern, params) = match matched {
        Ok((pat, p)) => (pat, p),
        Err(code) => {
            let status = if matches!(code, 404 | 405) { code } else { 500 };
            let mut guard = wrapper.lock_table();
            if let Some(resp) = guard.responses.get_mut(&resp_handle) {
                let _ = resp.set_status(status);
            }
            return BN_WEB_OK;
        }
    };

    // Store params on request
    {
        let mut guard = wrapper.lock_table();
        if let Some(req) = guard.requests.get_mut(&req_handle) {
            req.params = crate::web::sorted_params(&params);
        }
    }

    // Execute filters under bn_core_copy::run and catch_unwind
    // Execute filters under catch_unwind
    for filter in filters {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            filter(req_handle, resp_handle);
        }));
    }

    // Find and execute route handler
    let handler = routes
        .iter()
        .find(|(m, p, _)| m == &method && p == &pattern)
        .map(|(_, _, f)| *f);
    if let Some(handler_fn) = handler {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handler_fn(req_handle, resp_handle);
        }));
        if outcome.is_err() {
            let mut guard = wrapper.lock_table();
            if let Some(resp) = guard.responses.get_mut(&resp_handle) {
                let _ = resp.set_status(500);
            }
        }
    } else {
        let mut guard = wrapper.lock_table();
        if let Some(resp) = guard.responses.get_mut(&resp_handle) {
            let _ = resp.set_status(404);
        }
    }

    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_stop(handle: u64, timeout_ms: i64) -> i32 {
    let wrapper = table();
    let state_arc = {
        let guard = wrapper.lock_table();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure("Server.Stop", &WebFailure::Closed("server closed".into()));
        };
        std::sync::Arc::clone(&entry.state)
    };
    match crate::web::drain_server(&state_arc, i128::from(timeout_ms), false) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.Stop", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_close(handle: u64, timeout_ms: i64) -> i32 {
    let wrapper = table();
    let state_arc = {
        let guard = wrapper.lock_table();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure("Server.Close", &WebFailure::Closed("server closed".into()));
        };
        std::sync::Arc::clone(&entry.state)
    };
    match crate::web::drain_server(&state_arc, i128::from(timeout_ms), true) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.Close", &WebFailure::from_message(err)),
    }
}

// ---- ServerOptions C ABI -------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_options_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard
        .server_options
        .insert(handle, crate::web::ServerOptions::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_options_new(
    active_connections: i64,
    backlog: i64,
    pending_work: i64,
    worker_count: i64,
    max_header_bytes: i64,
    max_header_fields: i64,
    max_target_bytes: i64,
    max_body_bytes: i64,
    tls_handshake_ms: i64,
    header_read_ms: i64,
    body_read_ms: i64,
    idle_keep_alive_ms: i64,
    connection_total_ms: i64,
    stop_drain_ms: i64,
    rate_limit_burst: i64,
    rate_limit_refill_per_second: i64,
    rate_limit_key_capacity: i64,
    trusted_proxy: bool,
    out_handle: *mut u64,
) -> i32 {
    let non_neg = |val: i64| usize::try_from(val).ok();
    let non_neg_u64 = |val: i64| u64::try_from(val).ok();
    let Some(options) = (|| {
        Some(crate::web::ServerOptions {
            active_connections: non_neg(active_connections)?,
            backlog: non_neg(backlog)?,
            pending_work: non_neg(pending_work)?,
            worker_count: non_neg(worker_count)?,
            max_header_bytes: non_neg(max_header_bytes)?,
            max_header_fields: non_neg(max_header_fields)?,
            max_target_bytes: non_neg(max_target_bytes)?,
            max_body_bytes: non_neg(max_body_bytes)?,
            trusted_proxy,
            tls_handshake_ms: non_neg_u64(tls_handshake_ms)?,
            header_read_ms: non_neg_u64(header_read_ms)?,
            body_read_ms: non_neg_u64(body_read_ms)?,
            idle_keep_alive_ms: non_neg_u64(idle_keep_alive_ms)?,
            connection_total_ms: non_neg_u64(connection_total_ms)?,
            stop_drain_ms: non_neg_u64(stop_drain_ms)?,
            rate_limit_burst: non_neg(rate_limit_burst)?,
            rate_limit_refill_per_second: non_neg(rate_limit_refill_per_second)?,
            rate_limit_key_capacity: non_neg(rate_limit_key_capacity)?,
        })
    })() else {
        return record_failure(
            "ServerOptions.New",
            &WebFailure::InvalidArgument(
                bn_catalog::text("web", "server.options_non_negative").into(),
            ),
        );
    };

    if let Err(err) = options.validate() {
        return record_failure("ServerOptions.New", &WebFailure::from_message(err));
    }
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard.server_options.insert(handle, options);
    if !out_handle.is_null() {
        // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
        unsafe { *out_handle = handle };
    }
    BN_WEB_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_options_init(
    handle: u64,
    active_connections: i64,
    backlog: i64,
    pending_work: i64,
    worker_count: i64,
    max_header_bytes: i64,
    max_header_fields: i64,
    max_target_bytes: i64,
    max_body_bytes: i64,
    tls_handshake_ms: i64,
    header_read_ms: i64,
    body_read_ms: i64,
    idle_keep_alive_ms: i64,
    connection_total_ms: i64,
    stop_drain_ms: i64,
    rate_limit_burst: i64,
    rate_limit_refill_per_second: i64,
    rate_limit_key_capacity: i64,
    trusted_proxy: bool,
) -> i32 {
    let mut out = 0u64;
    let res = bn_web_server_options_new(
        active_connections,
        backlog,
        pending_work,
        worker_count,
        max_header_bytes,
        max_header_fields,
        max_target_bytes,
        max_body_bytes,
        tls_handshake_ms,
        header_read_ms,
        body_read_ms,
        idle_keep_alive_ms,
        connection_total_ms,
        stop_drain_ms,
        rate_limit_burst,
        rate_limit_refill_per_second,
        rate_limit_key_capacity,
        trusted_proxy,
        &raw mut out,
    );
    if res == BN_WEB_OK {
        let wrapper = table();
        let mut guard = wrapper.lock_table();
        if let Some(opts) = guard.server_options.remove(&out) {
            guard.server_options.insert(handle, opts);
        }
    }
    res
}

// ---- TLSConfig C ABI -----------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_tls_from_pem(
    cert_pem: *const c_char,
    key_pem: *const c_char,
    out_handle: *mut u64,
) -> i32 {
    let (Some(cert), Some(key)) = (string_text(cert_pem), string_text(key_pem)) else {
        return record_failure(
            "TLSConfig.FromPEM",
            &WebFailure::InvalidArgument("invalid cert or key".into()),
        );
    };
    match crate::tls::server_config_from_pem(&cert, &key) {
        Ok(config) => {
            let handle = alloc_handle();
            let wrapper = table();
            let mut guard = wrapper.lock_table();
            guard.tls_configs.insert(handle, Arc::new(config));
            if !out_handle.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_handle = handle };
            }
            BN_WEB_OK
        }
        Err(err) => record_failure("TLSConfig.FromPEM", &WebFailure::from_message(&err)),
    }
}

// ---- CookieJar C ABI -----------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_cookie_jar_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard
        .cookie_jars
        .insert(handle, crate::web_state::CookieJar::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_cookie_jar_set(
    handle: u64,
    name: *const c_char,
    val: *const c_char,
    domain: *const c_char,
    path: *const c_char,
    max_age_ms: i64,
) -> i32 {
    bn_web_cookie_jar_set_with_policy(
        handle,
        name,
        val,
        domain,
        path,
        max_age_ms,
        false,
        false,
        std::ptr::null(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_cookie_jar_set_with_policy(
    handle: u64,
    name: *const c_char,
    val: *const c_char,
    domain: *const c_char,
    path: *const c_char,
    max_age_ms: i64,
    secure: bool,
    http_only: bool,
    same_site: *const c_char,
) -> i32 {
    let (Some(name_str), Some(val_str)) = (string_text(name), string_text(val)) else {
        return record_failure(
            "CookieJar.Set",
            &WebFailure::InvalidArgument("invalid cookie name or val".into()),
        );
    };
    let domain_str = string_text(domain).unwrap_or_default();
    let path_str = string_text(path).unwrap_or_default();
    let same_site_str = string_text(same_site).unwrap_or_else(|| "Lax".into());

    let options =
        match crate::web_state::CookieOptions::from_policy(secure, http_only, &same_site_str) {
            Ok(opt) => opt,
            Err(err) => return record_failure("CookieJar.Set", &WebFailure::from_message(err)),
        };
    let duration = std::time::Duration::from_millis(u64::try_from(max_age_ms.max(0)).unwrap_or(0));

    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(jar) = guard.cookie_jars.get_mut(&handle) else {
        return record_failure(
            "CookieJar.Set",
            &WebFailure::Closed("cookie jar closed".into()),
        );
    };
    match jar.set_with_options(
        &name_str,
        &val_str,
        &domain_str,
        &path_str,
        duration,
        options,
    ) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("CookieJar.Set", &WebFailure::from_message(&err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_cookie_jar_get(
    handle: u64,
    name: *const c_char,
    domain: *const c_char,
    path: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let (Some(name_str), Some(domain_str), Some(path_str)) =
        (string_text(name), string_text(domain), string_text(path))
    else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "CookieJar.Get",
                    &WebFailure::InvalidArgument("invalid argument".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(jar) = guard.cookie_jars.get_mut(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "CookieJar.Get",
                    &WebFailure::Closed("cookie jar closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    if let Some(val) = jar.get(&name_str, &domain_str, &path_str) {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { *out_status = BN_WEB_OK };
        }
        c_string(&val)
    } else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure("CookieJar.Get", &WebFailure::CookieNotFound);
            };
        }
        std::ptr::null_mut()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_cookie_jar_count(handle: u64) -> i64 {
    let wrapper = table();
    let guard = wrapper.lock_table();
    guard
        .cookie_jars
        .get(&handle)
        .map_or(0, |jar| i64::try_from(jar.len()).unwrap_or(i64::MAX))
}

// ---- SessionStore C ABI --------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_session_store_new(
    capacity: i64,
    idle_ms: i64,
    out_handle: *mut u64,
) -> i32 {
    let idle_duration =
        std::time::Duration::from_millis(u64::try_from(idle_ms.max(1)).unwrap_or(1));
    match crate::web_state::SessionStore::new(i128::from(capacity), idle_duration) {
        Ok(store) => {
            let handle = alloc_handle();
            let wrapper = table();
            let mut guard = wrapper.lock_table();
            guard.session_stores.insert(handle, store);
            if !out_handle.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_handle = handle };
            }
            BN_WEB_OK
        }
        Err(err) => record_failure("SessionStore.New", &WebFailure::from_message(&err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_session_store_create(
    handle: u64,
    val: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(val_str) = string_text(val) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "SessionStore.Create",
                    &WebFailure::InvalidArgument("invalid val".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(store) = guard.session_stores.get_mut(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "SessionStore.Create",
                    &WebFailure::Closed("session store closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    match store.create(&val_str) {
        Ok(sid) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_status = BN_WEB_OK };
            }
            c_string(&sid)
        }
        Err(err) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe {
                    *out_status =
                        record_failure("SessionStore.Create", &WebFailure::from_message(&err));
                };
            }
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_session_store_get(
    handle: u64,
    id: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(id_str) = string_text(id) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "SessionStore.Get",
                    &WebFailure::InvalidArgument("invalid id".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(store) = guard.session_stores.get_mut(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "SessionStore.Get",
                    &WebFailure::Closed("session store closed".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    if let Some(val) = store.get(&id_str) {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { *out_status = BN_WEB_OK };
        }
        c_string(&val)
    } else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure("SessionStore.Get", &WebFailure::SessionNotFound);
            };
        }
        std::ptr::null_mut()
    }
}

// ---- Scraper C ABI -------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_scraper_create(html: *const c_char) -> u64 {
    let html_str = string_text(html).unwrap_or_default();
    let scraper = crate::web_state::Scraper::parse(&html_str)
        .unwrap_or_else(|_| crate::web_state::Scraper::empty());
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard.scrapers.insert(handle, scraper);
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_scraper_text(
    handle: u64,
    selector: *const c_char,
    out_status: *mut i32,
) -> *mut c_char {
    let Some(sel_str) = string_text(selector) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status = record_failure(
                    "Scraper.Text",
                    &WebFailure::InvalidArgument("invalid selector".into()),
                );
            };
        }
        return std::ptr::null_mut();
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(scraper) = guard.scrapers.get(&handle) else {
        if !out_status.is_null() {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe {
                *out_status =
                    record_failure("Scraper.Text", &WebFailure::Closed("scraper closed".into()));
            };
        }
        return std::ptr::null_mut();
    };
    match scraper.text(&sel_str) {
        Ok(text) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_status = BN_WEB_OK };
            }
            c_string(&text)
        }
        Err(err) => {
            if !out_status.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe {
                    *out_status = record_failure("Scraper.Text", &WebFailure::from_message(&err));
                };
            }
            std::ptr::null_mut()
        }
    }
}

// ---- ACL C ABI -----------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_acl_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    guard.acls.insert(handle, crate::web_state::Acl::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_acl_allow(handle: u64, net_str_ptr: *const c_char, prefix: i32) -> i32 {
    let Some(net_text) = string_text(net_str_ptr) else {
        return record_failure(
            "ACL.Allow",
            &WebFailure::InvalidArgument("invalid CIDR network".into()),
        );
    };
    let cidr_text = format!("{net_text}/{prefix}");
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(acl) = guard.acls.get_mut(&handle) else {
        return record_failure("ACL.Allow", &WebFailure::Closed("ACL closed".into()));
    };
    match acl.allow(&cidr_text) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("ACL.Allow", &WebFailure::from_message(&err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_acl_deny(handle: u64, net_str_ptr: *const c_char, prefix: i32) -> i32 {
    let Some(net_text) = string_text(net_str_ptr) else {
        return record_failure(
            "ACL.Deny",
            &WebFailure::InvalidArgument("invalid CIDR network".into()),
        );
    };
    let cidr_text = format!("{net_text}/{prefix}");
    let wrapper = table();
    let mut guard = wrapper.lock_table();
    let Some(acl) = guard.acls.get_mut(&handle) else {
        return record_failure("ACL.Deny", &WebFailure::Closed("ACL closed".into()));
    };
    match acl.deny(&cidr_text) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("ACL.Deny", &WebFailure::from_message(&err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_acl_check(handle: u64, addr_ptr: *const c_char) -> i32 {
    let Some(host_address) = string_text(addr_ptr) else {
        return 0;
    };
    let Ok(parsed) = host_address.parse::<IpAddr>() else {
        return 0;
    };
    let wrapper = table();
    let guard = wrapper.lock_table();
    let Some(acl) = guard.acls.get(&handle) else {
        return 0;
    };
    i32::from(acl.check(parsed))
}

// ---- EgressPolicy C ABI --------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_egress_policy_new(
    schemes: *const c_char,
    cidrs: *const c_char,
    ports: *const c_char,
    max_redirects: i64,
    deadline_ms: i64,
    out_handle: *mut u64,
) -> i32 {
    let schemes_str = string_text(schemes).unwrap_or_default();
    let cidrs_str = string_text(cidrs).unwrap_or_default();
    let ports_str = string_text(ports).unwrap_or_default();
    // A negative value is refused, not clamped (`EgressPolicy OR Error`).
    let (Ok(redirects), Ok(deadline)) =
        (usize::try_from(max_redirects), u64::try_from(deadline_ms))
    else {
        return record_failure(
            "EgressPolicy.New",
            &WebFailure::InvalidArgument(crate::web::EGRESS_POLICY_BOUNDS.into()),
        );
    };

    match crate::web::EgressPolicy::from_csv(
        &schemes_str,
        &cidrs_str,
        &ports_str,
        redirects,
        deadline,
    ) {
        Ok(policy) => {
            let handle = alloc_handle();
            let wrapper = table();
            let mut guard = wrapper.lock_table();
            guard.egress_policies.insert(handle, policy);
            if !out_handle.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_handle = handle };
            }
            BN_WEB_OK
        }
        Err(err) => record_failure("EgressPolicy.New", &WebFailure::InvalidArgument(err.into())),
    }
}

// ---- Client C ABI --------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_create(logger: u64) -> u64 {
    let handle = alloc_handle();
    if logger != 0 {
        table().lock_table().clients.insert(handle, logger);
    }
    handle
}

fn header_blob(headers: &[(String, String)]) -> String {
    let mut blob = String::new();
    for (name, value) in headers {
        if name.contains(['\n', '\r', '\0']) || value.contains(['\n', '\r', '\0']) {
            continue;
        }
        blob.push_str(name);
        blob.push('\n');
        blob.push_str(value);
        blob.push('\n');
    }
    blob
}

fn log_client_access(
    logger: u64,
    method: &str,
    url: &str,
    status: i64,
    bytes: i64,
    started: std::time::Instant,
    headers: &[(String, String)],
) {
    if logger == 0 {
        return;
    }
    let Ok(method) = std::ffi::CString::new(method) else {
        return;
    };
    let Ok(url) = std::ffi::CString::new(url) else {
        return;
    };
    let Ok(headers) = std::ffi::CString::new(header_blob(headers)) else {
        return;
    };
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    // SAFETY: the three pointers address the CString buffers above, which outlive this call.
    // `logger` is the handle `Client.New` stored.
    unsafe {
        bn_rt_log_client_access(
            logger,
            method.as_ptr(),
            url.as_ptr(),
            status,
            bytes,
            duration_ms,
            headers.as_ptr(),
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_request(
    handle: u64,
    method: *const c_char,
    url: *const c_char,
    body: *const c_char,
    out_resp_handle: *mut u64,
) -> i32 {
    bn_web_client_request_with_policy(handle, method, url, body, 0, out_resp_handle)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_request_with_policy(
    handle: u64,
    method: *const c_char,
    url: *const c_char,
    body: *const c_char,
    policy_handle: u64,
    out_resp_handle: *mut u64,
) -> i32 {
    let (Some(method_str), Some(url_str)) = (string_text(method), string_text(url)) else {
        return record_failure(
            "Client.Request",
            &WebFailure::InvalidArgument("invalid method or url".into()),
        );
    };
    let body_str = string_text(body).unwrap_or_default();

    let wrapper = table();
    let (policy, logger) = {
        let guard = wrapper.lock_table();
        let policy = if policy_handle != 0 {
            guard
                .egress_policies
                .get(&policy_handle)
                .cloned()
                .unwrap_or_else(crate::web::EgressPolicy::default)
        } else {
            crate::web::EgressPolicy::default()
        };
        let logger = guard.clients.get(&handle).copied().unwrap_or(0);
        (policy, logger)
    };

    let started = std::time::Instant::now();
    match crate::http::client_request_with_policy(&method_str, &url_str, &body_str, &policy) {
        Ok(resp) => {
            let bytes = i64::try_from(resp.body.len()).unwrap_or(i64::MAX);
            log_client_access(
                logger,
                &method_str,
                &url_str,
                i64::from(resp.status),
                bytes,
                started,
                &resp.headers,
            );
            let resp_handle = alloc_handle();
            let mut guard = wrapper.lock_table();
            guard.responses.insert(resp_handle, resp);
            if !out_resp_handle.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_resp_handle = resp_handle };
            }
            BN_WEB_OK
        }
        Err(err) => {
            log_client_access(logger, &method_str, &url_str, 0, 0, started, &[]);
            record_failure("Client.Request", &WebFailure::from_message(&err))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_close(handle: u64) -> i32 {
    table().lock_table().clients.remove(&handle);
    BN_WEB_OK
}

// ---- ClientReply C ABI ---------------------------------------------------

fn write_status(out_status: *mut i32, status: i32) {
    if !out_status.is_null() {
        // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
        unsafe { *out_status = status };
    }
}

fn write_u64(out: *mut u64, value: u64) {
    if !out.is_null() {
        // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
        unsafe { *out = value };
    }
}

#[expect(clippy::verbose_file_reads)]
fn read_form_file(path: &str) -> Vec<u8> {
    use std::io::Read as _;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    bytes
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

// ---- Client Transfer C ABI -----------------------------------------------

pub(crate) fn do_client_transfer(
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
    let (policy, logger, headers_entries) = {
        let guard = wrapper.lock_table();
        let logger = guard.clients.get(&client_handle).copied().unwrap_or(0);
        let policy = crate::web::EgressPolicy::default();
        let headers_entries = if headers_handle != 0 {
            guard
                .headers
                .get(&headers_handle)
                .map(|h| h.entries.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        (policy, logger, headers_entries)
    };

    let mut extra = headers_entries;
    if let Some(ct) = content_type
        && !extra
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
    {
        extra.push(("content-type".into(), ct.into()));
    }

    let started = std::time::Instant::now();
    match crate::http::client_transfer_with_policy(method, url, body, &extra, &policy) {
        Ok(reply) => {
            let reply_handle = alloc_handle();
            let bytes = i64::try_from(reply.body.len()).unwrap_or(i64::MAX);
            log_client_access(
                logger,
                method,
                url,
                i64::from(reply.status),
                bytes,
                started,
                &reply.headers,
            );
            let max_usize = usize::try_from(maximum.max(0)).unwrap_or(usize::MAX);
            if maximum > 0 && reply.body.len() > max_usize {
                return record_failure(
                    "Client.Transfer",
                    &WebFailure::BodyLimitExceeded(format!("{}/{}", reply.body.len(), maximum)),
                );
            }
            let mut guard = wrapper.lock_table();
            guard.client_replies.insert(reply_handle, reply);
            write_u64(out_reply, reply_handle);
            BN_WEB_OK
        }
        Err(err) => {
            log_client_access(logger, method, url, 0, 0, started, &[]);
            record_failure("Client.Transfer", &WebFailure::from_message(&err))
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.Get",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
        client_handle,
        "GET",
        &url_str,
        &[],
        None,
        0,
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
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.GetWithHeaders",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    do_client_transfer(
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
pub extern "C" fn bn_web_client_get_decoded(
    client_handle: u64,
    url: *const c_char,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    bn_web_client_get(client_handle, url, maximum, out_reply)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_get_decoded_with_headers(
    client_handle: u64,
    url: *const c_char,
    headers: u64,
    maximum: i64,
    out_reply: *mut u64,
) -> i32 {
    bn_web_client_get_with_headers(client_handle, url, headers, maximum, out_reply)
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
            .map(super::web::Query::encode)
            .unwrap_or_default()
    };
    let full_url = append_query(&url_str, &q_enc);
    do_client_transfer(
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
            .map(super::web::Query::encode)
            .unwrap_or_default()
    };
    let full_url = append_query(&url_str, &q_enc);
    do_client_transfer(
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
            .map(super::web::Fields::encode)
            .unwrap_or_default()
    };
    do_client_transfer(
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
            .map(super::web::Fields::encode)
            .unwrap_or_default()
    };
    do_client_transfer(
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
    bn_web_client_post_form_with_headers(client_handle, url, form, 0, maximum, out_reply)
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
    let Some(url_str) = string_text(url) else {
        return record_failure(
            "Client.PostForm",
            &WebFailure::InvalidArgument("invalid_url".into()),
        );
    };
    let form_data = {
        let wrapper = table();
        let guard = wrapper.lock_table();
        guard.forms.get(&form).cloned().unwrap_or_default()
    };
    let boundary = "----BNWebFormBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    for (k, v) in form_data.fields {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    for (name, path, ct) in form_data.files {
        let content = read_form_file(&path);
        let filename = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {ct}\r\n\r\n").as_bytes());
        body.extend_from_slice(&content);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let ct = format!("multipart/form-data;boundary={boundary}");
    do_client_transfer(
        client_handle,
        "POST",
        &url_str,
        &body,
        Some(&ct),
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

#[cfg(test)]
mod string_contract_tests {
    /// A NUL-terminated text after 16 bytes that are no STRING header.
    #[repr(C, align(8))]
    struct ForeignText([u8; 32]);

    /// A web argument is a STRING payload; any other pointer is refused,
    /// never guessed to be a C string.
    #[test]
    fn a_web_argument_that_is_not_a_string_is_refused_not_guessed() {
        let mut bytes = [0_u8; 32];
        bytes[..16].fill(0xFE);
        bytes[16..22].copy_from_slice(b"plain\0");
        let foreign = ForeignText(bytes);
        let pointer = foreign.0[16..].as_ptr().cast::<std::ffi::c_char>();
        assert_eq!(super::string_text(pointer), None);
        let real = super::c_string("plain");
        assert_eq!(super::string_text(real).as_deref(), Some("plain"));
    }
}

#[cfg(test)]
mod server_abi_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    extern "C" fn test_route_handler(_req: u64, resp: u64) {
        let msg = c_string("native ok");
        assert_eq!(bn_web_response_write(resp, msg), BN_WEB_OK);
    }

    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
        listener.local_addr().expect("port").port()
    }

    #[test]
    fn compiled_server_binds_listener_serves_request_and_stops() {
        let handle = bn_web_server_create();
        let method = c_string("GET");
        let pattern = c_string("/test");
        assert_eq!(
            bn_web_server_route(handle, method, pattern, test_route_handler),
            BN_WEB_OK
        );
        let port = free_port();
        let addr = c_string("127.0.0.1");
        assert_eq!(
            bn_web_server_start(handle, addr, i32::from(port)),
            BN_WEB_OK
        );

        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .write_all(b"GET /test HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("write request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("read response");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(response.contains("native ok"), "{response}");

        assert_eq!(bn_web_server_stop(handle, 1000), BN_WEB_OK);
    }

    #[test]
    fn compiled_server_start_with_options_and_start_tls() {
        let handle = bn_web_server_create();
        let mut opt_handle = 0u64;
        assert_eq!(
            bn_web_server_options_new(
                10,
                64,
                10,
                2,
                8192,
                64,
                4096,
                65536,
                5000,
                5000,
                5000,
                10000,
                30000,
                1000,
                10,
                10,
                100,
                false,
                &raw mut opt_handle,
            ),
            BN_WEB_OK
        );
        let port = free_port();
        let addr = c_string("127.0.0.1");
        assert_eq!(
            bn_web_server_start_with_options(handle, addr, i32::from(port), opt_handle),
            BN_WEB_OK
        );
        assert_eq!(bn_web_server_stop(handle, 1000), BN_WEB_OK);

        let cert_str = include_str!("../../../tests/fixtures/tls/server.pem");
        let key_str = include_str!("../../../tests/fixtures/tls/server-key.pem");
        let cert_c = c_string(cert_str);
        let key_c = c_string(key_str);
        let mut tls_handle = 0u64;
        assert_eq!(
            bn_web_tls_from_pem(cert_c, key_c, &raw mut tls_handle),
            BN_WEB_OK
        );

        let tls_server = bn_web_server_create();
        let tls_port = free_port();
        let tls_addr = c_string("127.0.0.1");
        assert_eq!(
            bn_web_server_start_tls_with_options(
                tls_server,
                tls_addr,
                i32::from(tls_port),
                tls_handle,
                opt_handle
            ),
            BN_WEB_OK
        );
        assert_eq!(bn_web_server_stop(tls_server, 1000), BN_WEB_OK);
    }
}
