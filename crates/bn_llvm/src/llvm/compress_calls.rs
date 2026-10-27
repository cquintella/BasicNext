// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Calls to `BNCompress` members. `Bytes` and `ZipArchive` travel as `bn_rt` table
//! handles (`i64`), as BNCrypto resources do.
#![allow(clippy::wildcard_imports)]
use super::host_results::{emit_handle_result, emit_status_result, emit_void_result};
use super::json_calls::emit_handle_index;
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(super) fn lower_bncompress_call(
    text: &mut crate::ir::LlvmModule,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) {
    let dest = destination.0;
    let v = |id: ValueId| O::reg(format!("v{}", id.0));
    let cmph = || O::reg(format!("cmph{dest}"));
    let r = |name: &str| O::reg(format!("{name}{dest}"));

    match member {
        "FromText" => {
            let args = vec![(T::Ptr, v(arguments[0]))];
            let call = I::call(T::I64, "bn_rt_compress_bytes_from_text", args);
            text.assign(format!("cmph{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::IntToPtr, T::I64, cmph(), T::Ptr),
            );
        }
        "FromHex" => {
            text.assign(format!("cmpout{dest}"), I::alloca(T::I64));
            let args = vec![(T::Ptr, v(arguments[0])), (T::Ptr, r("cmpout"))];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_compress_bytes_from_hex", args),
            );
            text.assign(format!("cmphandle{dest}"), I::load(T::I64, r("cmpout")));
            emit_handle_result(text, destination, &r("cmprc"), &r("cmphandle"));
        }
        "Length" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let args = vec![(T::I64, cmph())];
            let call = I::call(T::I64, "bn_rt_compress_bytes_length", args);
            text.assign(format!("cmpl{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::Trunc, T::I64, r("cmpl"), T::I32),
            );
        }
        "ToHex" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let args = vec![(T::I64, cmph())];
            let call = I::call(T::Ptr, "bn_rt_compress_bytes_to_hex", args);
            text.assign(format!("v{dest}"), call);
        }
        "ToText" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            text.assign(format!("cmpout{dest}"), I::alloca(T::Ptr));
            let args = vec![(T::I64, cmph()), (T::Ptr, r("cmpout"))];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_compress_bytes_to_text", args),
            );
            text.assign(format!("cmpstr{dest}"), I::load(T::Ptr, r("cmpout")));
            emit_status_result(text, destination, &r("cmprc"), None, &r("cmpstr"), &O::int(0));
        }
        "Deflate" | "Inflate" | "Gzip" | "Gunzip" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            text.assign(format!("cmpout{dest}"), I::alloca(T::I64));
            let symbol = format!("bn_rt_compress_{}", member.to_ascii_lowercase());
            let args = vec![(T::I64, cmph()), (T::Ptr, r("cmpout"))];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, &symbol, args));
            text.assign(format!("cmphandle{dest}"), I::load(T::I64, r("cmpout")));
            emit_handle_result(text, destination, &r("cmprc"), &r("cmphandle"));
        }
        "DeflateText" | "GzipText" => {
            text.assign(format!("cmpout{dest}"), I::alloca(T::I64));
            let symbol = if member == "DeflateText" {
                "bn_rt_compress_deflate_text"
            } else {
                "bn_rt_compress_gzip_text"
            };
            let args = vec![(T::Ptr, v(arguments[0])), (T::Ptr, r("cmpout"))];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, symbol, args));
            text.assign(format!("cmphandle{dest}"), I::load(T::I64, r("cmpout")));
            emit_handle_result(text, destination, &r("cmprc"), &r("cmphandle"));
        }
        "InflateText" | "GunzipText" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            text.assign(format!("cmpout{dest}"), I::alloca(T::Ptr));
            let symbol = if member == "InflateText" {
                "bn_rt_compress_inflate_text"
            } else {
                "bn_rt_compress_gunzip_text"
            };
            let args = vec![(T::I64, cmph()), (T::Ptr, r("cmpout"))];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, symbol, args));
            text.assign(format!("cmpstr{dest}"), I::load(T::Ptr, r("cmpout")));
            emit_status_result(text, destination, &r("cmprc"), None, &r("cmpstr"), &O::int(0));
        }
        "Create" | "Open" => {
            text.assign(format!("cmpout{dest}"), I::alloca(T::I64));
            let symbol = if member == "Create" {
                "bn_rt_zip_create"
            } else {
                "bn_rt_zip_open"
            };
            let args = vec![(T::Ptr, v(arguments[0])), (T::Ptr, r("cmpout"))];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, symbol, args));
            text.assign(format!("cmphandle{dest}"), I::load(T::I64, r("cmpout")));
            emit_handle_result(text, destination, &r("cmprc"), &r("cmphandle"));
        }
        "AddFile" | "ExtractFile" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let symbol = if member == "AddFile" {
                "bn_rt_zip_add_file"
            } else {
                "bn_rt_zip_extract_file"
            };
            let args = vec![
                (T::I64, cmph()),
                (T::Ptr, v(arguments[1])),
                (T::Ptr, v(arguments[2])),
            ];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, symbol, args));
            emit_void_result(text, destination, &r("cmprc"));
        }
        "AddText" | "AddStoredText" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let symbol = if member == "AddText" {
                "bn_rt_zip_add_text"
            } else {
                "bn_rt_zip_add_stored_text"
            };
            let args = vec![
                (T::I64, cmph()),
                (T::Ptr, v(arguments[1])),
                (T::Ptr, v(arguments[2])),
            ];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, symbol, args));
            emit_void_result(text, destination, &r("cmprc"));
        }
        "AddBytes" | "AddStoredBytes" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            emit_handle_index(text, analysis, &format!("cmpbytes{dest}"), arguments[2]);
            let symbol = if member == "AddBytes" {
                "bn_rt_zip_add_bytes"
            } else {
                "bn_rt_zip_add_stored_bytes"
            };
            let args = vec![
                (T::I64, cmph()),
                (T::Ptr, v(arguments[1])),
                (T::I64, r("cmpbytes")),
            ];
            text.assign(format!("cmprc{dest}"), I::call(T::I32, symbol, args));
            emit_void_result(text, destination, &r("cmprc"));
        }
        "ExtractAll" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let args = vec![(T::I64, cmph()), (T::Ptr, v(arguments[1]))];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_zip_extract_all", args),
            );
            emit_void_result(text, destination, &r("cmprc"));
        }
        "ExtractText" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            text.assign(format!("cmpout{dest}"), I::alloca(T::Ptr));
            let args = vec![
                (T::I64, cmph()),
                (T::Ptr, v(arguments[1])),
                (T::Ptr, r("cmpout")),
            ];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_zip_extract_text", args),
            );
            text.assign(format!("cmpstr{dest}"), I::load(T::Ptr, r("cmpout")));
            emit_status_result(text, destination, &r("cmprc"), None, &r("cmpstr"), &O::int(0));
        }
        "ExtractBytes" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            text.assign(format!("cmpout{dest}"), I::alloca(T::I64));
            let args = vec![
                (T::I64, cmph()),
                (T::Ptr, v(arguments[1])),
                (T::Ptr, r("cmpout")),
            ];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_zip_extract_bytes", args),
            );
            text.assign(format!("cmphandle{dest}"), I::load(T::I64, r("cmpout")));
            emit_handle_result(text, destination, &r("cmprc"), &r("cmphandle"));
        }
        "EntryCount" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let args = vec![(T::I64, cmph())];
            let call = I::call(T::I64, "bn_rt_zip_entry_count", args);
            text.assign(format!("cmpcount{dest}"), call);
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::Trunc, T::I64, r("cmpcount"), T::I32),
            );
        }
        "EntryName" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let arg_ty = analysis.values.get(&arguments[1]).expect("entry index type");
            let index = helpers::extend_to_i64(text, arguments[1], arg_ty);
            text.assign(format!("cmpout{dest}"), I::alloca(T::Ptr));
            let args = vec![
                (T::I64, cmph()),
                (T::I64, index),
                (T::Ptr, r("cmpout")),
            ];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_zip_entry_name", args),
            );
            text.assign(format!("cmpstr{dest}"), I::load(T::Ptr, r("cmpout")));
            emit_status_result(text, destination, &r("cmprc"), None, &r("cmpstr"), &O::int(0));
        }
        "Close" => {
            emit_handle_index(text, analysis, &format!("cmph{dest}"), arguments[0]);
            let args = vec![(T::I64, cmph())];
            text.assign(
                format!("cmprc{dest}"),
                I::call(T::I32, "bn_rt_zip_close", args),
            );
            emit_void_result(text, destination, &r("cmprc"));
        }
        other => panic!("unsupported BNCompress member reached emission: {other}"),
    }
}
