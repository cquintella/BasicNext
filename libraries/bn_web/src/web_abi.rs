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
use std::ffi::{CStr, c_char};
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

#[repr(C, align(8))]
pub(crate) struct StringHeader {
    pub ref_count: std::sync::atomic::AtomicU64,
    pub len: u64,
}
pub(crate) const HEADER_SIZE: usize = std::mem::size_of::<StringHeader>();

#[allow(clippy::cast_ptr_alignment)] // the alignment check above the cast accepts only a StringHeader
pub(crate) fn slice_from_raw<'a>(ptr: *const c_char) -> Option<&'a [u8]> {
    if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<StringHeader>()) {
        return None;
    }
    // SAFETY: G7-D1. A non-null, aligned STRING payload is preceded by its header.
    let header_ptr = unsafe { ptr.cast::<u8>().sub(HEADER_SIZE).cast::<StringHeader>() };
    // SAFETY: G7-D1. `header_ptr` is that header, and it stays live for this read.
    let header = unsafe { &*header_ptr };
    let count = usize::try_from(header.len).ok()?;
    // SAFETY: G7-D1. `header.len` is the number of payload bytes at `ptr`.
    Some(unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), count) })
}

pub(crate) fn string_text(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    if let Some(bytes) = slice_from_raw(ptr) {
        String::from_utf8(bytes.to_vec()).ok()
    } else {
        // SAFETY: the pointer is non-null and NUL-terminated for this read.
        unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .ok()
            .map(String::from)
    }
}

#[allow(clippy::cast_ptr_alignment)] // malloc is aligned for StringHeader
pub(crate) fn c_string_from_bytes(bytes: &[u8]) -> *mut c_char {
    let len = bytes.len();
    let total_alloc = HEADER_SIZE + len + 1;
    // SAFETY: G7-D1: a non-null STRING is preceded by its header; the allocation is the header, the payload and a trailing NUL, and a null allocation is not read.
    let base = unsafe { libc::malloc(total_alloc) }.cast::<u8>();
    if base.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: G7-D1: a non-null STRING is preceded by its header; the allocation is the header, the payload and a trailing NUL, and a null allocation is not read.
    unsafe {
        base.cast::<StringHeader>().write(StringHeader {
            ref_count: std::sync::atomic::AtomicU64::new(1),
            len: len as u64,
        });
        let data_ptr = base.add(HEADER_SIZE);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data_ptr, bytes.len());
        data_ptr.add(bytes.len()).write(0);
        data_ptr.cast::<c_char>()
    }
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
    pub(crate) servers: HashMap<u64, ServerEntry>,
    pub(crate) server_options: HashMap<u64, crate::web::ServerOptions>,
    pub(crate) tls_configs: HashMap<u64, Arc<rustls::ServerConfig>>,
    pub(crate) cookie_jars: HashMap<u64, crate::web_state::CookieJar>,
    pub(crate) session_stores: HashMap<u64, crate::web_state::SessionStore>,
    pub(crate) scrapers: HashMap<u64, crate::web_state::Scraper>,
    pub(crate) acls: HashMap<u64, crate::web_state::Acl>,
    pub(crate) egress_policies: HashMap<u64, crate::web::EgressPolicy>,
    pub(crate) responses: HashMap<u64, crate::web::Response>,
    pub(crate) requests: HashMap<u64, crate::web::Request>,
    pub(crate) values: HashMap<u64, Vec<String>>,
    pub(crate) request_sessions: HashMap<u64, crate::web::sessions::RequestSession>,
    /// Logger handle for each client; `0` means the client does not log.
    pub(crate) clients: HashMap<u64, u64>,
    pub(crate) next_id: u64,
}

pub(crate) struct WebTableWrapper(pub(crate) Mutex<WebTable>);

impl Default for WebTableWrapper {
    fn default() -> Self {
        Self(Mutex::new(WebTable {
            next_id: 1,
            ..Default::default()
        }))
    }
}

pub(crate) fn table() -> Arc<WebTableWrapper> {
    bn_core_copy::table::<WebTableWrapper>()
}

pub(crate) fn alloc_handle() -> u64 {
    let wrapper = table();
    let mut guard = wrapper.0.lock().unwrap();
    guard.next_id = guard.next_id.wrapping_add(1).max(1);
    guard.next_id
}

// ---- Response C ABI ------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_create() -> u64 {
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
pub extern "C" fn bn_web_response_is_committed(handle: u64) -> bool {
    let wrapper = table();
    let guard = wrapper.0.lock().unwrap();
    guard
        .responses
        .get(&handle)
        .is_some_and(super::web::Response::is_committed)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_response_close(handle: u64) -> i32 {
    let wrapper = table();
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
    guard
        .requests
        .insert(handle, crate::web::Request::default());
    handle
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_method(handle: u64) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.0.lock().unwrap();
    guard
        .requests
        .get(&handle)
        .map_or_else(|| c_string("GET"), |r| c_string(&r.method))
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_request_target(handle: u64) -> *mut c_char {
    let wrapper = table();
    let guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
    let Some(entry) = guard.servers.get_mut(&handle) else {
        return record_failure("Server.Route", &WebFailure::Closed("server closed".into()));
    };
    let mut state = entry.state.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
    let Some(entry) = guard.servers.get(&handle) else {
        return record_failure("Server.Static", &WebFailure::Closed("server closed".into()));
    };
    let mut state = entry.state.lock().unwrap();
    match state.add_static(&prefix_str, &dir_str) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.Static", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_use_sessions(handle: u64, capacity: i64, idle_ms: i64) -> i32 {
    let wrapper = table();
    let guard = wrapper.0.lock().unwrap();
    let Some(entry) = guard.servers.get(&handle) else {
        return record_failure(
            "Server.UseSessions",
            &WebFailure::Closed("server closed".into()),
        );
    };
    let mut state = entry.state.lock().unwrap();
    match state.use_sessions(i128::from(capacity), i128::from(idle_ms)) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.UseSessions", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_start(handle: u64, _addr_ptr: *const c_char, _port: i32) -> i32 {
    let wrapper = table();
    let guard = wrapper.0.lock().unwrap();
    let Some(entry) = guard.servers.get(&handle) else {
        return record_failure("Server.Start", &WebFailure::Closed("server closed".into()));
    };
    let mut state = entry.state.lock().unwrap();
    match state.start_with_options(crate::web::ServerOptions::default()) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.Start", &WebFailure::from_message(err)),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_server_dispatch(handle: u64, req_handle: u64, resp_handle: u64) -> i32 {
    let wrapper = table();
    let (filters, routes, state_arc) = {
        let guard = wrapper.0.lock().unwrap();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure(
                "Server.Dispatch",
                &WebFailure::Closed("server closed".into()),
            );
        };
        (
            entry.filters.clone(),
            entry.routes.clone(),
            entry.state.clone(),
        )
    };

    let (method, target) = {
        let guard = wrapper.0.lock().unwrap();
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
        let mut state = state_arc.lock().unwrap();
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
        Err(404) => {
            let mut guard = wrapper.0.lock().unwrap();
            if let Some(resp) = guard.responses.get_mut(&resp_handle) {
                let _ = resp.set_status(404);
            }
            return BN_WEB_OK;
        }
        Err(405) => {
            let mut guard = wrapper.0.lock().unwrap();
            if let Some(resp) = guard.responses.get_mut(&resp_handle) {
                let _ = resp.set_status(405);
            }
            return BN_WEB_OK;
        }
        Err(_) => {
            let mut guard = wrapper.0.lock().unwrap();
            if let Some(resp) = guard.responses.get_mut(&resp_handle) {
                let _ = resp.set_status(500);
            }
            return BN_WEB_OK;
        }
    };

    // Store params on request
    {
        let mut guard = wrapper.0.lock().unwrap();
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
            let mut guard = wrapper.0.lock().unwrap();
            if let Some(resp) = guard.responses.get_mut(&resp_handle) {
                let _ = resp.set_status(500);
            }
        }
    } else {
        let mut guard = wrapper.0.lock().unwrap();
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
        let guard = wrapper.0.lock().unwrap();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure("Server.Stop", &WebFailure::Closed("server closed".into()));
        };
        entry.state.clone()
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
        let guard = wrapper.0.lock().unwrap();
        let Some(entry) = guard.servers.get(&handle) else {
            return record_failure("Server.Close", &WebFailure::Closed("server closed".into()));
        };
        entry.state.clone()
    };
    match crate::web::drain_server(&state_arc, i128::from(timeout_ms), true) {
        Ok(()) => BN_WEB_OK,
        Err(err) => record_failure("Server.Close", &WebFailure::from_message(err)),
    }
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
            let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
            let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
        .unwrap_or_else(|_| crate::web_state::Scraper::parse("").unwrap());
    let handle = alloc_handle();
    let wrapper = table();
    let mut guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let mut guard = wrapper.0.lock().unwrap();
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
    let guard = wrapper.0.lock().unwrap();
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
    let redirects = usize::try_from(max_redirects.max(0)).unwrap_or(0);
    let deadline = u64::try_from(deadline_ms.max(0)).unwrap_or(0);

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
            let mut guard = wrapper.0.lock().unwrap();
            guard.egress_policies.insert(handle, policy);
            if !out_handle.is_null() {
                // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
                unsafe { *out_handle = handle };
            }
            BN_WEB_OK
        }
        Err(err) => record_failure("EgressPolicy.New", &WebFailure::from_message(err)),
    }
}

// ---- Client C ABI --------------------------------------------------------

#[unsafe(no_mangle)]
pub extern "C" fn bn_web_client_create(logger: u64) -> u64 {
    let handle = alloc_handle();
    if logger != 0 {
        table().0.lock().unwrap().clients.insert(handle, logger);
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
        let guard = wrapper.0.lock().unwrap();
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
            let mut guard = wrapper.0.lock().unwrap();
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
    table().0.lock().unwrap().clients.remove(&handle);
    BN_WEB_OK
}
