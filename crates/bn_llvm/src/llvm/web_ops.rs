// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native LLVM lowering for `BNWeb` standard library operations.

#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

use super::web_helpers::*;

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

        "server_options_constructor" => {
            if arguments.len() >= 19 {
                let h0 = web_handle(text, dest, 0, arguments[0], analysis);
                let mut args = vec![h0];
                for arg in arguments.iter().take(18).skip(1) {
                    args.push(web_int64(text, *arg, analysis));
                }
                let b18 = (T::I1, O::reg(format!("v{}", arguments[18].0)));
                args.push(b18);
                let rc_reg = format!("webrc{dest}");
                let call = I::call(T::I32, "bn_web_server_options_init", args);
                text.assign(&rc_reg, call);
            }
        }

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
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_start",
                vec![h0, (T::Ptr, addr_op), (T::I32, port_op)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_start_with_options" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (addr_op, port_op) = endpoint_parts(text, arguments[1], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_start_with_options",
                vec![h0, (T::Ptr, addr_op), (T::I32, port_op), h2],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_start_tls" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (addr_op, port_op) = endpoint_parts(text, arguments[1], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_start_tls",
                vec![h0, (T::Ptr, addr_op), (T::I32, port_op), h2],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "server_start_tls_with_options" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let (addr_op, port_op) = endpoint_parts(text, arguments[1], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let h3 = web_handle(text, dest, 3, arguments[3], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_server_start_tls_with_options",
                vec![h0, (T::Ptr, addr_op), (T::I32, port_op), h2, h3],
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

        "server_options_new" => {
            let mut args = Vec::with_capacity(19);
            for arg in arguments.iter().take(17) {
                args.push(web_int64(text, *arg, analysis));
            }
            let b17 = (T::I1, O::reg(format!("v{}", arguments[17].0)));
            args.push(b17);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            args.push((T::Ptr, O::reg(&out_reg)));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_server_options_new", args);
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
            let rc_reg = format!("webrc{dest}");
            text.assign(&rc_reg, I::call(T::I32, "bn_web_client_close", vec![h0]));
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "client_reply_constructor" => {
            let call = I::call(T::I64, "bn_web_client_reply_create", vec![]);
            text.assign(format!("webh{dest}"), call);
            let pointer = I::cast(
                CastOp::IntToPtr,
                T::I64,
                O::reg(format!("webh{dest}")),
                T::Ptr,
            );
            text.assign(format!("v{dest}"), pointer);
        }

        other => {
            if !web_client_ops::lower_bnweb_client_call(
                text,
                destination,
                method,
                arguments,
                analysis,
            ) {
                unreachable!(
                    "{}",
                    bn_catalog::fill("llvm", "msg.unsupported_bnweb_method", &[("other", &other)])
                );
            }
        }
    }
}

#[path = "web_client_ops.rs"]
mod web_client_ops;

#[path = "web_abi.rs"]
mod web_abi;
pub(crate) use web_abi::declare_web_abi;
