// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native LLVM lowering for `BNWeb` standard library operations.

#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{handle_result_ty, llvm_type};

/// Checks if `ty` represents a `BNWeb` resource type backed by a handle.
pub(crate) fn is_bnweb_resource_type(module: &Module, ty: &Type) -> bool {
    matches!(ty, Type::ImportedNamed { module: module_id, name }
        if module.bnweb_providers.contains(&bn_ir::ModuleId(module_id.0))
            && matches!(name.as_str(), "Response" | "Request" | "Server" | "CookieJar" | "SessionStore" | "Scraper" | "ACL" | "Client"))
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
        "Server.StartTLS" => Some("server_start_tls"),
        "Server.Dispatch" => Some("server_dispatch"),
        "Server.UseSessions" => Some("server_use_sessions"),
        "Server.Stop" => Some("server_stop"),
        "Server.Close" => Some("server_close"),

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
        | "cookie_jar_constructor"
        | "acl_constructor"
        | "client_constructor" => !arguments.is_empty(),
        "scraper_constructor" => arguments.len() >= 2,
        "response_status"
        | "response_commit"
        | "response_is_committed"
        | "response_close"
        | "request_method"
        | "request_target"
        | "cookie_jar_count"
        | "client_close"
        | "client_new" => arguments.len() == 1,
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
        | "tls_from_pem" => arguments.len() == 2,
        "response_set_header"
        | "server_static"
        | "server_start_tls"
        | "server_dispatch"
        | "server_use_sessions" => arguments.len() == 3,
        "server_route" | "response_clear_cookie" | "cookie_jar_get" | "client_request" => {
            arguments.len() == 4
        }
        "client_request_with_policy" | "egress_policy_new" => arguments.len() == 5,
        "response_set_cookie" | "cookie_jar_set" => arguments.len() == 6,
        "response_set_cookie_with_policy" | "cookie_jar_set_with_policy" => arguments.len() == 9,
        _ => false,
    }
}

/// Extracts an `i64` handle from an operand.
fn web_handle(
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
fn web_int64(
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
fn ptr_arg(arguments: &[ValueId], idx: usize) -> (T, O) {
    (T::Ptr, O::reg(format!("v{}", arguments[idx].0)))
}

#[inline]
fn bool_arg(arguments: &[ValueId], idx: usize) -> (T, O) {
    (T::I1, O::reg(format!("v{}", arguments[idx].0)))
}

/// Lowers a call to a `BNWeb` method.
pub(crate) fn lower_bnweb_call(
    text: &mut crate::ir::LlvmModule,
    _module: &Module,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;

    match method {
        // Constructors: already allocated in emit_allocate
        "response_constructor"
        | "request_constructor"
        | "server_constructor"
        | "cookie_jar_constructor"
        | "scraper_constructor"
        | "acl_constructor"
        | "client_constructor" => {}

        "client_new" => {
            let logger = super::log_emission::logger_handle(text, dest, arguments[0], analysis);
            let handle = format!("webh{dest}");
            text.assign(
                &handle,
                I::call(T::I64, "bn_web_client_create", vec![logger]),
            );
            emit_handle_result(text, destination, &O::int(0), &O::reg(&handle));
        }

        "response_set_status" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_response_set_status", vec![h0, i1]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_status" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_response_status", vec![h0]);
            text.assign(format!("webstat{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::Trunc,
                    T::I64,
                    O::reg(format!("webstat{dest}")),
                    T::I32,
                ),
            );
        }

        "response_set_header" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_response_set_header",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_header" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_response_header",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "response_set_cookie" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i5 = web_int64(text, arguments[5], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_response_set_cookie",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    ptr_arg(arguments, 4),
                    i5,
                ],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_set_cookie_with_policy" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i5 = web_int64(text, arguments[5], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_response_set_cookie_with_policy",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    ptr_arg(arguments, 4),
                    i5,
                    bool_arg(arguments, 6),
                    bool_arg(arguments, 7),
                    ptr_arg(arguments, 8),
                ],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_clear_cookie" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_response_clear_cookie",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                ],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_write" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_response_write",
                vec![h0, ptr_arg(arguments, 1)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_commit" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_response_commit", vec![h0]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "response_is_committed" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I32, "bn_web_response_is_committed", vec![h0]);
            text.assign(format!("webret{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::icmp(
                    ICmpCond::Ne,
                    T::I32,
                    O::reg(format!("webret{dest}")),
                    O::int(0),
                ),
            );
        }

        "response_close" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_response_close", vec![h0]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "request_method" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::Ptr, "bn_web_request_method", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "request_target" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::Ptr, "bn_web_request_target", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "request_header" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_request_header",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "request_param" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_request_param",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "request_cookie" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_request_cookie",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "request_body" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_request_body",
                vec![h0, i1, (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "server_add_filter" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_add_filter",
                vec![h0, ptr_arg(arguments, 1)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_route" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_route",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                ],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_static" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_static",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_start" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (addr_op, port_op) = endpoint_parts(text, arguments[1], analysis);
            let port_reg = format!("webport{dest}");
            text.assign(&port_reg, I::cast(CastOp::Trunc, T::I32, port_op, T::I16));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_start",
                vec![h0, (T::Ptr, addr_op), (T::I16, O::reg(port_reg))],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_start_tls" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (addr_op, port_op) = endpoint_parts(text, arguments[1], analysis);
            let port_reg = format!("webport{dest}");
            text.assign(&port_reg, I::cast(CastOp::Trunc, T::I32, port_op, T::I16));
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_start_tls",
                vec![h0, (T::Ptr, addr_op), (T::I16, O::reg(port_reg)), h2],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_dispatch" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h1 = web_handle(text, dest, 1, arguments[1], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_server_dispatch", vec![h0, h1, h2]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_use_sessions" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let i2 = web_int64(text, arguments[2], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_server_use_sessions", vec![h0, i1, i2]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_stop" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_server_stop", vec![h0, i1]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_close" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_server_close", vec![h0, i1]);
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "tls_from_pem" => {
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_tls_from_pem",
                vec![
                    ptr_arg(arguments, 0),
                    ptr_arg(arguments, 1),
                    (T::Ptr, O::reg(&out_reg)),
                ],
            );
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "cookie_jar_set" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i5 = web_int64(text, arguments[5], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_cookie_jar_set",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    ptr_arg(arguments, 4),
                    i5,
                ],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "cookie_jar_set_with_policy" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i5 = web_int64(text, arguments[5], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_cookie_jar_set_with_policy",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    ptr_arg(arguments, 4),
                    i5,
                    bool_arg(arguments, 6),
                    bool_arg(arguments, 7),
                    ptr_arg(arguments, 8),
                ],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "cookie_jar_get" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_cookie_jar_get",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    (T::Ptr, O::reg(&st_reg)),
                ],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "cookie_jar_count" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_cookie_jar_count", vec![h0]);
            text.assign(format!("webcnt{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(
                    CastOp::Trunc,
                    T::I64,
                    O::reg(format!("webcnt{dest}")),
                    T::I32,
                ),
            );
        }

        "session_store_new" => {
            let i0 = web_int64(text, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_session_store_new",
                vec![i0, i1, (T::Ptr, O::reg(&out_reg))],
            );
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "session_store_create" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_session_store_create",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "session_store_get" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_session_store_get",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "scraper_text" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_scraper_text",
                vec![h0, ptr_arg(arguments, 1), (T::Ptr, O::reg(&st_reg))],
            );
            text.assign(format!("webstr{dest}"), call);
            text.assign(format!("webrc{dest}"), I::load(T::I32, O::reg(&st_reg)));
            emit_status_result(
                text,
                destination,
                &O::reg(format!("webrc{dest}")),
                None,
                &O::reg(format!("webstr{dest}")),
                &O::int(0),
            );
        }

        "acl_allow" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (cidr_net, cidr_pfx) = cidr_parts(text, arguments[1]);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_acl_allow",
                vec![h0, (T::Ptr, cidr_net), (T::I32, cidr_pfx)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "acl_deny" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (cidr_net, cidr_pfx) = cidr_parts(text, arguments[1]);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_acl_deny",
                vec![h0, (T::Ptr, cidr_net), (T::I32, cidr_pfx)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "acl_check" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let addr_op = net_payload_ptr(text, arguments[1]);
            let call = I::call(T::I32, "bn_web_acl_check", vec![h0, (T::Ptr, addr_op)]);
            text.assign(format!("webret{dest}"), call);
            text.assign(
                format!("webpay{dest}"),
                I::cast(
                    CastOp::ZExt,
                    T::I32,
                    O::reg(format!("webret{dest}")),
                    T::I64,
                ),
            );
            emit_handle_result(
                text,
                destination,
                &O::int(0),
                &O::reg(format!("webpay{dest}")),
            );
        }

        "egress_policy_new" => {
            let i3 = web_int64(text, arguments[3], analysis);
            let i4 = web_int64(text, arguments[4], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_egress_policy_new",
                vec![
                    ptr_arg(arguments, 0),
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    i3,
                    i4,
                    (T::Ptr, O::reg(&out_reg)),
                ],
            );
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "client_request" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_request",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    (T::Ptr, O::reg(&out_reg)),
                ],
            );
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "client_request_with_policy" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h4 = web_handle(text, dest, 4, arguments[4], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_request_with_policy",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    h4,
                    (T::Ptr, O::reg(&out_reg)),
                ],
            );
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "client_close" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            text.emit(I::call(T::Void, "bn_web_client_close", vec![h0]));
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::BitCast, T::Ptr, O::null(), T::Ptr),
            );
        }

        other => unreachable!(
            "{}",
            bn_catalog::fill("llvm", "msg.unsupported_bnweb_method", &[("other", &other)])
        ),
    }
}

/// Helper extracting address and port operands from an Endpoint argument.
fn endpoint_parts(
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
fn net_payload_ptr(text: &mut crate::ir::LlvmModule, value: ValueId) -> O {
    let temp = format!("webpay{}", value.0);
    text.assign(
        temp.clone(),
        I::extract(handle_result_ty(), O::reg(format!("v{}", value.0)), 1),
    );
    O::reg(temp)
}

/// Helper extracting network address string pointer and prefix length from a Net.CIDR argument.
fn cidr_parts(text: &mut crate::ir::LlvmModule, value: ValueId) -> (O, O) {
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

#[path = "web_abi.rs"]
mod web_abi;
pub(crate) use web_abi::declare_web_abi;
