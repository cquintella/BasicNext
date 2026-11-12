// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native LLVM lowering for `BNWeb` client transfer operations.

#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(crate) fn lower_bnweb_client_call(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) -> bool {
    let dest = destination.0;

    match method {
        "client_reply_status" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_client_reply_status", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "client_reply_http_version" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::Ptr, "bn_web_client_reply_http_version", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "client_reply_body_size" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_client_reply_body_size", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "client_reply_total_ms" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_client_reply_total_ms", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "client_reply_connect_ms" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_client_reply_connect_ms", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "client_reply_header_count" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_client_reply_header_count", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "client_reply_header" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_client_reply_header",
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

        "client_reply_header_name" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_client_reply_header_name",
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

        "client_reply_header_value" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_client_reply_header_value",
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

        "client_reply_body_text" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_client_reply_body_text",
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

        "client_reply_body_bytes" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_reply_body_bytes",
                vec![h0, i1, (T::Ptr, O::reg(&out_reg))],
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

        "client_reply_body_decoded_text" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_client_reply_body_decoded_text",
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

        "client_reply_body_decoded_bytes" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i1 = web_int64(text, arguments[1], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_reply_body_decoded_bytes",
                vec![h0, i1, (T::Ptr, O::reg(&out_reg))],
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

        "client_reply_peer_address" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_client_reply_peer_address",
                vec![h0, (T::Ptr, O::reg(&st_reg))],
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

        "headers_constructor" => {
            let call = I::call(T::I64, "bn_web_headers_create", vec![]);
            text.assign(format!("webh{dest}"), call);
            let pointer = I::cast(
                CastOp::IntToPtr,
                T::I64,
                O::reg(format!("webh{dest}")),
                T::Ptr,
            );
            text.assign(format!("v{dest}"), pointer);
        }

        "headers_new" => {
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_headers_new",
                vec![(T::Ptr, O::reg(&out_reg))],
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

        "headers_add" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_headers_add",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "headers_set" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_headers_set",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "headers_count" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let call = I::call(T::I64, "bn_web_headers_count", vec![h0]);
            text.assign(format!("v{dest}"), call);
        }

        "headers_get" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_headers_get",
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

        "query_constructor" => {
            let call = I::call(T::I64, "bn_web_query_create", vec![]);
            text.assign(format!("webh{dest}"), call);
            let pointer = I::cast(
                CastOp::IntToPtr,
                T::I64,
                O::reg(format!("webh{dest}")),
                T::Ptr,
            );
            text.assign(format!("v{dest}"), pointer);
        }

        "query_new" => {
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_query_new", vec![(T::Ptr, O::reg(&out_reg))]);
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "query_add" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_query_add",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "query_encode" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_query_encode",
                vec![h0, (T::Ptr, O::reg(&st_reg))],
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

        "fields_constructor" => {
            let call = I::call(T::I64, "bn_web_fields_create", vec![]);
            text.assign(format!("webh{dest}"), call);
            let pointer = I::cast(
                CastOp::IntToPtr,
                T::I64,
                O::reg(format!("webh{dest}")),
                T::Ptr,
            );
            text.assign(format!("v{dest}"), pointer);
        }

        "fields_new" => {
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_fields_new",
                vec![(T::Ptr, O::reg(&out_reg))],
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

        "fields_add" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_fields_add",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "fields_encode" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let st_reg = format!("weboutst{dest}");
            text.assign(&st_reg, I::alloca(T::I32));
            let call = I::call(
                T::Ptr,
                "bn_web_fields_encode",
                vec![h0, (T::Ptr, O::reg(&st_reg))],
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

        "form_constructor" => {
            let call = I::call(T::I64, "bn_web_form_create", vec![]);
            text.assign(format!("webh{dest}"), call);
            let pointer = I::cast(
                CastOp::IntToPtr,
                T::I64,
                O::reg(format!("webh{dest}")),
                T::Ptr,
            );
            text.assign(format!("v{dest}"), pointer);
        }

        "form_new" => {
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(T::I32, "bn_web_form_new", vec![(T::Ptr, O::reg(&out_reg))]);
            text.assign(&rc_reg, call);
            text.assign(format!("webhval{dest}"), I::load(T::I64, O::reg(&out_reg)));
            emit_handle_result(
                text,
                destination,
                &O::reg(rc_reg),
                &O::reg(format!("webhval{dest}")),
            );
        }

        "form_add_field" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_form_add_field",
                vec![h0, ptr_arg(arguments, 1), ptr_arg(arguments, 2)],
            );
            text.assign(&rc_reg, call);
            emit_void_result(text, destination, &O::reg(rc_reg));
        }

        "form_add_file" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_form_add_file",
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

        // ---- Client Verbs Lowering ----
        "client_get" | "client_get_decoded" | "client_head" | "client_delete"
        | "client_options" => {
            let sym = match method {
                "client_get" => "bn_web_client_get",
                "client_get_decoded" => "bn_web_client_get_decoded",
                "client_head" => "bn_web_client_head",
                "client_delete" => "bn_web_client_delete",
                _ => "bn_web_client_options",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i2 = web_int64(text, arguments[2], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![h0, ptr_arg(arguments, 1), i2, (T::Ptr, O::reg(&out_reg))],
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

        "client_get_with_headers"
        | "client_get_decoded_with_headers"
        | "client_head_with_headers"
        | "client_delete_with_headers"
        | "client_options_with_headers" => {
            let sym = match method {
                "client_get_with_headers" => "bn_web_client_get_with_headers",
                "client_get_decoded_with_headers" => "bn_web_client_get_decoded_with_headers",
                "client_head_with_headers" => "bn_web_client_head_with_headers",
                "client_delete_with_headers" => "bn_web_client_delete_with_headers",
                _ => "bn_web_client_options_with_headers",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let i3 = web_int64(text, arguments[3], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    h2,
                    i3,
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

        "client_get_query" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let i3 = web_int64(text, arguments[3], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_get_query",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    h2,
                    i3,
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

        "client_get_query_with_headers" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let h3 = web_handle(text, dest, 3, arguments[3], analysis);
            let i4 = web_int64(text, arguments[4], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_get_query_with_headers",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    h2,
                    h3,
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

        "client_post" | "client_put" | "client_patch" => {
            let sym = match method {
                "client_post" => "bn_web_client_post",
                "client_put" => "bn_web_client_put",
                _ => "bn_web_client_patch",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let i4 = web_int64(text, arguments[4], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
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

        "client_post_with_headers" | "client_put_with_headers" | "client_patch_with_headers" => {
            let sym = match method {
                "client_post_with_headers" => "bn_web_client_post_with_headers",
                "client_put_with_headers" => "bn_web_client_put_with_headers",
                _ => "bn_web_client_patch_with_headers",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h4 = web_handle(text, dest, 4, arguments[4], analysis);
            let i5 = web_int64(text, arguments[5], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    h4,
                    i5,
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

        "client_post_bytes" | "client_put_bytes" | "client_patch_bytes" => {
            let sym = match method {
                "client_post_bytes" => "bn_web_client_post_bytes",
                "client_put_bytes" => "bn_web_client_put_bytes",
                _ => "bn_web_client_patch_bytes",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let i4 = web_int64(text, arguments[4], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    h2,
                    ptr_arg(arguments, 3),
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

        "client_post_bytes_with_headers"
        | "client_put_bytes_with_headers"
        | "client_patch_bytes_with_headers" => {
            let sym = match method {
                "client_post_bytes_with_headers" => "bn_web_client_post_bytes_with_headers",
                "client_put_bytes_with_headers" => "bn_web_client_put_bytes_with_headers",
                _ => "bn_web_client_patch_bytes_with_headers",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h2 = web_handle(text, dest, 2, arguments[2], analysis);
            let h4 = web_handle(text, dest, 4, arguments[4], analysis);
            let i5 = web_int64(text, arguments[5], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    h2,
                    ptr_arg(arguments, 3),
                    h4,
                    i5,
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

        "client_post_fields" | "client_post_form" | "client_post_json" => {
            let sym = match method {
                "client_post_fields" => "bn_web_client_post_fields",
                "client_post_form" => "bn_web_client_post_form",
                _ => "bn_web_client_post_json",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let arg2 = if method == "client_post_json" {
                ptr_arg(arguments, 2)
            } else {
                web_handle(text, dest, 2, arguments[2], analysis)
            };
            let i3 = web_int64(text, arguments[3], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    arg2,
                    i3,
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

        "client_post_fields_with_headers"
        | "client_post_form_with_headers"
        | "client_post_json_with_headers" => {
            let sym = match method {
                "client_post_fields_with_headers" => "bn_web_client_post_fields_with_headers",
                "client_post_form_with_headers" => "bn_web_client_post_form_with_headers",
                _ => "bn_web_client_post_json_with_headers",
            };
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let arg2 = if method == "client_post_json_with_headers" {
                ptr_arg(arguments, 2)
            } else {
                web_handle(text, dest, 2, arguments[2], analysis)
            };
            let h3 = web_handle(text, dest, 3, arguments[3], analysis);
            let i4 = web_int64(text, arguments[4], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                sym,
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    arg2,
                    h3,
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

        "client_request_method" => {
            let h0 = web_handle(text, dest, 0, arguments[0], analysis);
            let h5 = web_handle(text, dest, 5, arguments[5], analysis);
            let i6 = web_int64(text, arguments[6], analysis);
            let out_reg = format!("webout{dest}");
            text.assign(&out_reg, I::alloca(T::I64));
            let rc_reg = format!("webrc{dest}");
            let call = I::call(
                T::I32,
                "bn_web_client_request_method",
                vec![
                    h0,
                    ptr_arg(arguments, 1),
                    ptr_arg(arguments, 2),
                    ptr_arg(arguments, 3),
                    ptr_arg(arguments, 4),
                    h5,
                    i6,
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

        _ => return false,
    }
    true
}
