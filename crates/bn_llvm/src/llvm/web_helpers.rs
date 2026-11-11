// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![allow(clippy::wildcard_imports)]
use std::collections::HashMap;

use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{handle_result_ty, llvm_type};

/// Checks whether an imported type is a `BNWeb` resource class.
pub(crate) fn is_bnweb_resource_type(module: &Module, ty: &Type) -> bool {
    matches!(ty, Type::ImportedNamed { module: module_id, name }
        if module.bnweb_providers.contains(&bn_ir::ModuleId(module_id.0))
            && matches!(name.as_str(), "Response" | "Request" | "Server" | "ServerOptions" | "CookieJar" | "SessionStore" | "Scraper" | "ACL" | "Client" | "ClientReply" | "Headers" | "Query" | "Fields" | "Form"))
}

/// Identifies supported `BNWeb` methods by module ID and method name.
pub(crate) fn bnweb_method(module: &Module, name: &str) -> Option<&'static str> {
    let (module_id, rest) = name.strip_prefix('#')?.split_once('.')?;
    let module_id = bn_ir::ModuleId(module_id.parse().ok()?);
    if !module.bnweb_providers.contains(&module_id) {
        return None;
    }
    match rest {
        "Response.CONSTRUCTOR" => Some("response_constructor"),
        "Response.SetStatus" => Some("response_set_status"),
        "Response.Status" => Some("response_status"),
        "Response.SetHeader" => Some("response_set_header"),
        "Response.Header" => Some("response_header"),
        "Response.SetCookie" => Some("response_set_cookie"),
        "Response.SetCookieWithPolicy" => Some("response_set_cookie_with_policy"),
        "Response.ClearCookie" => Some("response_clear_cookie"),
        "Response.Write" => Some("response_write"),
        "Response.Commit" => Some("response_commit"),
        "Response.IsCommitted" => Some("response_is_committed"),
        "Response.Close" => Some("response_close"),

        "Request.CONSTRUCTOR" => Some("request_constructor"),
        "Request.Method" => Some("request_method"),
        "Request.Target" => Some("request_target"),
        "Request.Header" => Some("request_header"),
        "Request.Param" => Some("request_param"),
        "Request.Cookie" => Some("request_cookie"),
        "Request.Body" => Some("request_body"),

        "Server.CONSTRUCTOR" => Some("server_constructor"),
        "Server.AddFilter" => Some("server_add_filter"),
        "Server.Route" => Some("server_route"),
        "Server.Static" => Some("server_static"),
        "Server.Start" => Some("server_start"),
        "Server.StartWithOptions" => Some("server_start_with_options"),
        "Server.StartTLS" => Some("server_start_tls"),
        "Server.StartTLSWithOptions" => Some("server_start_tls_with_options"),
        "Server.Dispatch" => Some("server_dispatch"),
        "Server.UseSessions" => Some("server_use_sessions"),
        "Server.Stop" => Some("server_stop"),
        "Server.Close" => Some("server_close"),

        "ServerOptions.CONSTRUCTOR" => Some("server_options_constructor"),
        "ServerOptions.New" => Some("server_options_new"),

        "TLSConfig.FromPEM" => Some("tls_from_pem"),

        "CookieJar.CONSTRUCTOR" => Some("cookie_jar_constructor"),
        "CookieJar.Set" => Some("cookie_jar_set"),
        "CookieJar.SetWithPolicy" => Some("cookie_jar_set_with_policy"),
        "CookieJar.Get" => Some("cookie_jar_get"),
        "CookieJar.Count" => Some("cookie_jar_count"),

        "SessionStore.New" => Some("session_store_new"),
        "SessionStore.Create" => Some("session_store_create"),
        "SessionStore.Get" => Some("session_store_get"),

        "Scraper.CONSTRUCTOR" => Some("scraper_constructor"),
        "Scraper.Text" => Some("scraper_text"),

        "ACL.CONSTRUCTOR" => Some("acl_constructor"),
        "ACL.Allow" => Some("acl_allow"),
        "ACL.Deny" => Some("acl_deny"),
        "ACL.Check" => Some("acl_check"),

        "EgressPolicy.New" => Some("egress_policy_new"),

        "Client.CONSTRUCTOR" => Some("client_constructor"),
        "Client.New" => Some("client_new"),
        "Client.Request" => Some("client_request"),
        "Client.RequestWithPolicy" => Some("client_request_with_policy"),
        "Client.Close" => Some("client_close"),

        "ClientReply.CONSTRUCTOR" => Some("client_reply_constructor"),
        "ClientReply.Status" => Some("client_reply_status"),
        "ClientReply.HttpVersion" => Some("client_reply_http_version"),
        "ClientReply.BodySize" => Some("client_reply_body_size"),
        "ClientReply.TotalMs" => Some("client_reply_total_ms"),
        "ClientReply.ConnectMs" => Some("client_reply_connect_ms"),
        "ClientReply.Header" => Some("client_reply_header"),
        "ClientReply.HeaderCount" => Some("client_reply_header_count"),
        "ClientReply.HeaderName" => Some("client_reply_header_name"),
        "ClientReply.HeaderValue" => Some("client_reply_header_value"),
        "ClientReply.BodyText" => Some("client_reply_body_text"),
        "ClientReply.BodyBytes" => Some("client_reply_body_bytes"),
        "ClientReply.BodyDecodedText" => Some("client_reply_body_decoded_text"),
        "ClientReply.BodyDecodedBytes" => Some("client_reply_body_decoded_bytes"),
        "ClientReply.PeerAddress" => Some("client_reply_peer_address"),

        "Headers.CONSTRUCTOR" => Some("headers_constructor"),
        "Headers.New" => Some("headers_new"),
        "Headers.Add" => Some("headers_add"),
        "Headers.Set" => Some("headers_set"),
        "Headers.Count" => Some("headers_count"),
        "Headers.Get" => Some("headers_get"),

        "Query.CONSTRUCTOR" => Some("query_constructor"),
        "Query.New" => Some("query_new"),
        "Query.Add" => Some("query_add"),
        "Query.Encode" => Some("query_encode"),

        "Fields.CONSTRUCTOR" => Some("fields_constructor"),
        "Fields.New" => Some("fields_new"),
        "Fields.Add" => Some("fields_add"),
        "Fields.Encode" => Some("fields_encode"),

        "Form.CONSTRUCTOR" => Some("form_constructor"),
        "Form.New" => Some("form_new"),
        "Form.AddField" => Some("form_add_field"),
        "Form.AddFile" => Some("form_add_file"),

        "Client.Get" => Some("client_get"),
        "Client.GetWithHeaders" => Some("client_get_with_headers"),
        "Client.GetDecoded" => Some("client_get_decoded"),
        "Client.GetDecodedWithHeaders" => Some("client_get_decoded_with_headers"),
        "Client.GetQuery" => Some("client_get_query"),
        "Client.GetQueryWithHeaders" => Some("client_get_query_with_headers"),
        "Client.Head" => Some("client_head"),
        "Client.HeadWithHeaders" => Some("client_head_with_headers"),
        "Client.Delete" => Some("client_delete"),
        "Client.DeleteWithHeaders" => Some("client_delete_with_headers"),
        "Client.Options" => Some("client_options"),
        "Client.OptionsWithHeaders" => Some("client_options_with_headers"),
        "Client.Post" => Some("client_post"),
        "Client.PostWithHeaders" => Some("client_post_with_headers"),
        "Client.PostBytes" => Some("client_post_bytes"),
        "Client.PostBytesWithHeaders" => Some("client_post_bytes_with_headers"),
        "Client.PostFields" => Some("client_post_fields"),
        "Client.PostFieldsWithHeaders" => Some("client_post_fields_with_headers"),
        "Client.PostForm" => Some("client_post_form"),
        "Client.PostFormWithHeaders" => Some("client_post_form_with_headers"),
        "Client.PostJson" => Some("client_post_json"),
        "Client.PostJsonWithHeaders" => Some("client_post_json_with_headers"),
        "Client.Put" => Some("client_put"),
        "Client.PutWithHeaders" => Some("client_put_with_headers"),
        "Client.PutBytes" => Some("client_put_bytes"),
        "Client.PutBytesWithHeaders" => Some("client_put_bytes_with_headers"),
        "Client.Patch" => Some("client_patch"),
        "Client.PatchWithHeaders" => Some("client_patch_with_headers"),
        "Client.PatchBytes" => Some("client_patch_bytes"),
        "Client.PatchBytesWithHeaders" => Some("client_patch_bytes_with_headers"),
        "Client.RequestMethod" => Some("client_request_method"),
        "Client.RequestMethodWithHeaders" => Some("client_request_method_with_headers"),

        _ => None,
    }
}

/// Verifies whether the argument list for a `BNWeb` call is supported.
pub(crate) fn bnweb_arguments_supported(
    _module: &Module,
    method: Option<&str>,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> bool {
    let Some(m) = method else { return false };
    if arguments.iter().any(|arg| values.get(arg).is_none()) {
        return false;
    }
    match m {
        "response_constructor"
        | "request_constructor"
        | "server_constructor"
        | "server_options_constructor"
        | "cookie_jar_constructor"
        | "acl_constructor"
        | "client_constructor"
        | "client_reply_constructor"
        | "headers_constructor"
        | "query_constructor"
        | "fields_constructor"
        | "form_constructor" => !arguments.is_empty(),
        "headers_new" | "query_new" | "fields_new" | "form_new" => arguments.is_empty(),
        "scraper_constructor" => arguments.len() >= 2,
        "response_status"
        | "response_commit"
        | "response_is_committed"
        | "response_close"
        | "request_method"
        | "request_target"
        | "cookie_jar_count"
        | "client_close"
        | "client_new"
        | "client_reply_status"
        | "client_reply_http_version"
        | "client_reply_body_size"
        | "client_reply_total_ms"
        | "client_reply_connect_ms"
        | "client_reply_header_count"
        | "client_reply_peer_address"
        | "headers_count"
        | "query_encode"
        | "fields_encode" => arguments.len() == 1,
        "response_set_status"
        | "response_write"
        | "response_header"
        | "request_header"
        | "request_param"
        | "request_cookie"
        | "request_body"
        | "server_add_filter"
        | "server_start"
        | "server_stop"
        | "server_close"
        | "session_store_new"
        | "session_store_create"
        | "session_store_get"
        | "scraper_text"
        | "acl_allow"
        | "acl_deny"
        | "acl_check"
        | "tls_from_pem"
        | "client_reply_header"
        | "client_reply_header_name"
        | "client_reply_header_value"
        | "client_reply_body_text"
        | "client_reply_body_bytes"
        | "client_reply_body_decoded_text"
        | "client_reply_body_decoded_bytes"
        | "headers_get" => arguments.len() == 2,
        "response_set_header"
        | "server_static"
        | "server_start_with_options"
        | "server_start_tls"
        | "server_dispatch"
        | "server_use_sessions"
        | "headers_add"
        | "headers_set"
        | "query_add"
        | "fields_add"
        | "form_add_field"
        | "client_get"
        | "client_get_decoded"
        | "client_head"
        | "client_delete"
        | "client_options" => arguments.len() == 3,
        "server_start_tls_with_options"
        | "server_route"
        | "response_clear_cookie"
        | "cookie_jar_get"
        | "client_request"
        | "client_get_with_headers"
        | "client_get_decoded_with_headers"
        | "client_get_query"
        | "client_head_with_headers"
        | "client_delete_with_headers"
        | "client_options_with_headers"
        | "client_post_fields"
        | "client_post_form"
        | "client_post_json"
        | "form_add_file" => arguments.len() == 4,
        "client_request_with_policy"
        | "egress_policy_new"
        | "client_get_query_with_headers"
        | "client_post"
        | "client_post_bytes"
        | "client_post_fields_with_headers"
        | "client_post_form_with_headers"
        | "client_post_json_with_headers"
        | "client_put"
        | "client_put_bytes"
        | "client_patch"
        | "client_patch_bytes" => arguments.len() == 5,
        "response_set_cookie"
        | "cookie_jar_set"
        | "client_post_with_headers"
        | "client_post_bytes_with_headers"
        | "client_put_with_headers"
        | "client_put_bytes_with_headers"
        | "client_patch_with_headers"
        | "client_patch_bytes_with_headers"
        | "client_request_method" => arguments.len() == 6,
        "client_request_method_with_headers" => arguments.len() == 7,
        "response_set_cookie_with_policy" | "cookie_jar_set_with_policy" => arguments.len() == 9,
        "server_options_new" => arguments.len() == 18,
        _ => false,
    }
}

/// Extracts an `i64` handle from an operand.
pub(crate) fn web_handle(
    text: &mut crate::ir::LlvmModule,
    dest: u32,
    arg_idx: usize,
    value: ValueId,
    analysis: &LoweringAnalysis<'_>,
) -> (T, O) {
    let reg_name = format!("webh{dest}_{arg_idx}");
    let own = O::reg(format!("v{}", value.0));
    let inst = if matches!(analysis.values.get(&value), Some(Type::Alternative(_))) {
        I::extract(handle_result_ty(), own, 2)
    } else {
        I::cast(CastOp::PtrToInt, T::Ptr, own, T::I64)
    };
    text.assign(&reg_name, inst);
    (T::I64, O::reg(reg_name))
}

/// Extracts an integer operand widened to `i64`.
pub(crate) fn web_int64(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    analysis: &LoweringAnalysis<'_>,
) -> (T, O) {
    let ty = analysis
        .values
        .get(&value)
        .unwrap_or_else(|| panic!("{}", bn_catalog::text("llvm", "msg.int_type")));
    (T::I64, extend_to_i64(text, value, ty))
}

#[inline]
pub(crate) fn ptr_arg(arguments: &[ValueId], idx: usize) -> (T, O) {
    (T::Ptr, O::reg(format!("v{}", arguments[idx].0)))
}

#[inline]
pub(crate) fn bool_arg(arguments: &[ValueId], idx: usize) -> (T, O) {
    (T::I1, O::reg(format!("v{}", arguments[idx].0)))
}

/// Helper extracting address and port operands from an Endpoint argument.
pub(crate) fn endpoint_parts(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    analysis: &LoweringAnalysis<'_>,
) -> (O, O) {
    let ty = analysis
        .values
        .get(&value)
        .and_then(llvm_type)
        .unwrap_or(crate::layout::vector_ty());
    let first = usize::from(ty != crate::layout::vector_ty());
    let address = format!("webepaddr{}", value.0);
    let port = format!("webepport{}", value.0);
    text.assign(
        address.clone(),
        I::extract(ty.clone(), O::reg(format!("v{}", value.0)), first),
    );
    text.assign(
        port.clone(),
        I::extract(ty, O::reg(format!("v{}", value.0)), first + 1),
    );
    (O::reg(address), O::reg(port))
}

/// Helper extracting the string pointer payload from a Net.Address or Net.CIDR argument.
pub(crate) fn net_payload_ptr(text: &mut crate::ir::LlvmModule, value: ValueId) -> O {
    let temp = format!("webpay{}", value.0);
    text.assign(
        temp.clone(),
        I::extract(handle_result_ty(), O::reg(format!("v{}", value.0)), 1),
    );
    O::reg(temp)
}

/// Helper extracting network address string pointer and prefix length from a Net.CIDR argument.
pub(crate) fn cidr_parts(text: &mut crate::ir::LlvmModule, value: ValueId) -> (O, O) {
    let temp_ptr = format!("webcidrp{}", value.0);
    text.assign(
        temp_ptr.clone(),
        I::extract(handle_result_ty(), O::reg(format!("v{}", value.0)), 1),
    );
    let temp_pfx64 = format!("webcidrpfx64{}", value.0);
    text.assign(
        temp_pfx64.clone(),
        I::extract(handle_result_ty(), O::reg(format!("v{}", value.0)), 2),
    );
    let temp_pfx32 = format!("webcidrpfx32{}", value.0);
    text.assign(
        temp_pfx32.clone(),
        I::cast(CastOp::Trunc, T::I64, O::reg(temp_pfx64), T::I32),
    );
    (O::reg(temp_ptr), O::reg(temp_pfx32))
}
