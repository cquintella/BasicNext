// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! External declarations for `BNWeb` C ABI functions in native LLVM modules.

use crate::ir::{LlvmType as T, abi::Signature};

/// Declares external `BNWeb` C ABI functions in the LLVM module.
pub(crate) fn declare_web_abi(text: &mut crate::ir::LlvmModule) {
    let decls = [
        Signature {
            name: "bn_web_response_create",
            ret: T::I64,
            params: vec![],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_set_status",
            ret: T::I32,
            params: vec![T::I64, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_status",
            ret: T::I64,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_set_header",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_header",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_set_cookie",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr, T::Ptr, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_set_cookie_with_policy",
            ret: T::I32,
            params: vec![
                T::I64,
                T::Ptr,
                T::Ptr,
                T::Ptr,
                T::Ptr,
                T::I64,
                T::I1,
                T::I1,
                T::Ptr,
            ],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_clear_cookie",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_write",
            ret: T::I32,
            params: vec![T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_commit",
            ret: T::I32,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_is_committed",
            ret: T::I32,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_response_close",
            ret: T::Void,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_create",
            ret: T::I64,
            params: vec![],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_method",
            ret: T::Ptr,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_target",
            ret: T::Ptr,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_header",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_param",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_cookie",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_request_body",
            ret: T::Ptr,
            params: vec![T::I64, T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_create",
            ret: T::I64,
            params: vec![],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_add_filter",
            ret: T::I32,
            params: vec![T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_route",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_static",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_start",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::I16],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_start_tls",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::I16, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_dispatch",
            ret: T::I32,
            params: vec![T::I64, T::I64, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_use_sessions",
            ret: T::I32,
            params: vec![T::I64, T::I64, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_stop",
            ret: T::I32,
            params: vec![T::I64, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_server_close",
            ret: T::I32,
            params: vec![T::I64, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_tls_from_pem",
            ret: T::I32,
            params: vec![T::Ptr, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_cookie_jar_create",
            ret: T::I64,
            params: vec![],
            variadic: false,
        },
        Signature {
            name: "bn_web_cookie_jar_set",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr, T::Ptr, T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_cookie_jar_set_with_policy",
            ret: T::I32,
            params: vec![
                T::I64,
                T::Ptr,
                T::Ptr,
                T::Ptr,
                T::Ptr,
                T::I64,
                T::I1,
                T::I1,
                T::Ptr,
            ],
            variadic: false,
        },
        Signature {
            name: "bn_web_cookie_jar_get",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_cookie_jar_count",
            ret: T::I64,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_session_store_create",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_session_store_new",
            ret: T::I32,
            params: vec![T::I64, T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_session_store_get",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_scraper_create",
            ret: T::I64,
            params: vec![T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_scraper_text",
            ret: T::Ptr,
            params: vec![T::I64, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_acl_create",
            ret: T::I64,
            params: vec![],
            variadic: false,
        },
        Signature {
            name: "bn_web_acl_allow",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::I32],
            variadic: false,
        },
        Signature {
            name: "bn_web_acl_deny",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::I32],
            variadic: false,
        },
        Signature {
            name: "bn_web_acl_check",
            ret: T::I32,
            params: vec![T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_egress_policy_new",
            ret: T::I32,
            params: vec![T::Ptr, T::Ptr, T::Ptr, T::I64, T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_client_create",
            ret: T::I64,
            params: vec![T::I64],
            variadic: false,
        },
        Signature {
            name: "bn_web_client_request",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_client_request_with_policy",
            ret: T::I32,
            params: vec![T::I64, T::Ptr, T::Ptr, T::Ptr, T::I64, T::Ptr],
            variadic: false,
        },
        Signature {
            name: "bn_web_client_close",
            ret: T::Void,
            params: vec![T::I64],
            variadic: false,
        },
    ];
    for sig in decls {
        text.declare(sig);
    }
}
