// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native `HOST.FileSystem`: which calls `bnc` supports, and their lowering
// onto the `bn_rt_file_*` / `bn_rt_fs_*` C ABI (semantics in `bn_rt::file`).
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::{handle_result_ty, vector_ty};

/// Every `HOST.FileSystem` operation native code implements.
pub(crate) const FS_CALLS: [&str; 10] = [
    "HOST.FileSystem.Open",
    "HOST.FileSystem.Exists",
    "HOST.FileSystem.DeleteFile",
    "FS.File.Close",
    "FS.File.ReadAll",
    "FS.File.ReadLine",
    "FS.File.Write",
    "FS.File.WriteLine",
    "FS.File.ReadBytes",
    "FS.File.WriteBytes",
];

/// Whether the argument shapes of FS call `name` are supported; `None` for a
/// call outside `HOST.FileSystem`.
pub(crate) fn fs_call_supported(
    name: &str,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> Option<bool> {
    Some(match name {
        "HOST.FileSystem.Open" => {
            arguments.len() == 2
                && values.get(&arguments[0]) == Some(&Type::String)
                && values
                    .get(&arguments[1])
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "FS.File.Close" | "FS.File.ReadAll" | "FS.File.ReadLine" => arguments.len() == 1,
        "HOST.FileSystem.Exists" | "HOST.FileSystem.DeleteFile" => {
            arguments.len() == 1 && values.get(&arguments[0]) == Some(&Type::String)
        }
        "FS.File.Write" | "FS.File.WriteLine" => {
            arguments.len() == 2 && values.get(&arguments[1]) == Some(&Type::String)
        }
        "FS.File.ReadBytes" => {
            arguments.len() == 2
                && values.get(&arguments[1]).and_then(llvm_type) == Some("{ ptr, i32 }")
        }
        "FS.File.WriteBytes" => {
            arguments.len() == 3
                && values.get(&arguments[1]).and_then(llvm_type) == Some("{ ptr, i32 }")
                && values
                    .get(&arguments[2])
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        _ => return None,
    })
}

/// Lowers FS call `name`; false when `name` is not a `HOST.FileSystem` call.
#[allow(clippy::too_many_lines)]
pub(crate) fn lower_fs_call(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) -> bool {
    let dest = destination.0;
    let v = |index: usize| O::reg(format!("v{}", arguments[index].0));
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    // `%{prefix}handle{dest}`: the file handle in slot 2 of `FS.File OR Error`.
    let file_handle = |text: &mut String, prefix: &str| {
        let handle = I::extract(handle_result_ty(), v(0), 2);
        text.assign(format!("{prefix}handle{dest}"), handle);
        (T::I64, r(&format!("{prefix}handle")))
    };
    // `%{prefix}ptr` and `%{prefix}cap64`: the data and length of a BYTE buffer.
    let buffer = |text: &mut String, prefix: &str| {
        text.assign(
            format!("{prefix}ptr{dest}"),
            I::extract(vector_ty(), v(1), 0),
        );
        text.assign(
            format!("{prefix}cap{dest}"),
            I::extract(vector_ty(), v(1), 1),
        );
        let wide = I::cast(CastOp::SExt, T::I32, r(&format!("{prefix}cap")), T::I64);
        text.assign(format!("{prefix}cap64{dest}"), wide);
    };
    let call = |symbol: &str, args| I::call(T::I32, symbol, args).to_string();
    match name {
        "HOST.FileSystem.Open" => {
            let mode = extend_to_i32(
                text,
                arguments[1],
                analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated file mode"),
            );
            text.assign(format!("fileout{dest}"), I::alloca(T::I64));
            let args = vec![
                (T::Ptr, v(0)),
                (T::I32, O::raw(mode)),
                (T::Ptr, r("fileout")),
            ];
            text.assign(
                format!("filerc{dest}"),
                I::call(T::I32, "bn_rt_file_open", args),
            );
            text.assign(format!("filehandle{dest}"), I::load(T::I64, r("fileout")));
            emit_handle_result(
                text,
                destination,
                format!("%filerc{dest}"),
                format!("%filehandle{dest}"),
            );
        }
        "FS.File.Close" => {
            let handle = file_handle(text, "fileclose");
            emit_void_result(text, destination, call("bn_rt_file_close", vec![handle]));
        }
        "HOST.FileSystem.Exists" => {
            text.assign(format!("fsexout{dest}"), I::alloca(T::I32));
            let args = vec![(T::Ptr, v(0)), (T::Ptr, r("fsexout"))];
            text.assign(
                format!("fsexrc{dest}"),
                I::call(T::I32, "bn_rt_fs_exists", args),
            );
            text.assign(format!("fsexval{dest}"), I::load(T::I32, r("fsexout")));
            let wide = I::cast(CastOp::ZExt, T::I32, r("fsexval"), T::I64);
            text.assign(format!("fsexpay{dest}"), wide);
            emit_status_result(
                text,
                destination,
                &format!("%fsexrc{dest}"),
                None,
                "null",
                &format!("%fsexpay{dest}"),
            );
        }
        "HOST.FileSystem.DeleteFile" => {
            let delete = call("bn_rt_fs_delete_file", vec![(T::Ptr, v(0))]);
            emit_void_result(text, destination, delete);
        }
        "FS.File.Write" | "FS.File.WriteLine" => {
            let symbol = if name == "FS.File.Write" {
                "bn_rt_file_write"
            } else {
                "bn_rt_file_write_line"
            };
            let handle = file_handle(text, "filew");
            emit_void_result(
                text,
                destination,
                call(symbol, vec![handle, (T::Ptr, v(1))]),
            );
        }
        "FS.File.ReadAll" | "FS.File.ReadLine" => {
            let (symbol, eof) = if name == "FS.File.ReadAll" {
                ("bn_rt_file_read_all", None)
            } else {
                ("bn_rt_file_read_line", Some(4))
            };
            let handle = file_handle(text, "filer");
            text.assign(format!("filerout{dest}"), I::alloca(T::Ptr));
            let args = vec![handle, (T::Ptr, r("filerout"))];
            text.assign(format!("filerrc{dest}"), I::call(T::I32, symbol, args));
            text.assign(format!("filerdata{dest}"), I::load(T::Ptr, r("filerout")));
            emit_status_result(
                text,
                destination,
                &format!("%filerrc{dest}"),
                eof,
                &format!("%filerdata{dest}"),
                "0",
            );
        }
        "FS.File.ReadBytes" => {
            let handle = file_handle(text, "fileb");
            buffer(text, "fileb");
            text.assign(format!("filebout{dest}"), I::alloca(T::I64));
            let args = vec![
                handle,
                (T::Ptr, r("filebptr")),
                (T::I64, r("filebcap64")),
                (T::Ptr, r("filebout")),
            ];
            let read = I::call(T::I32, "bn_rt_file_read_bytes", args);
            text.assign(format!("filebrc{dest}"), read);
            text.assign(format!("filebcount{dest}"), I::load(T::I64, r("filebout")));
            emit_status_result(
                text,
                destination,
                &format!("%filebrc{dest}"),
                Some(4),
                "null",
                &format!("%filebcount{dest}"),
            );
        }
        "FS.File.WriteBytes" => {
            let count_ty = analysis
                .values
                .get(&arguments[2])
                .expect("validated byte count");
            let count = O::raw(coerce_to_type(
                text,
                arguments[2],
                count_ty,
                &Type::Integer(IntegerType::Int64),
            ));
            let handle = file_handle(text, "filewb");
            buffer(text, "filewb");
            // host.md: `count` outside 0..=LEN(buffer) is INDEX_OUT_OF_BOUNDS,
            // the same trap as vector indexing.
            let negative = I::icmp(ICmpCond::Slt, T::I64, count.clone(), O::int(0));
            text.assign(format!("filewbneg{dest}"), negative);
            let over = I::icmp(ICmpCond::Sgt, T::I64, count.clone(), r("filewbcap64"));
            text.assign(format!("filewbover{dest}"), over);
            let bad = I::binary(BinaryOp::Or, T::I1, r("filewbneg"), r("filewbover"));
            text.assign(format!("filewbbad{dest}"), bad);
            let ok = take_continuation(block_id, state);
            let index = I::cast(CastOp::SExt, T::I64, count.clone(), T::I128);
            text.assign(format!("filewbidx{dest}"), index);
            let length = I::cast(CastOp::SExt, T::I32, r("filewbcap"), T::I128);
            text.assign(format!("filewblen{dest}"), length);
            emit_trap(
                text,
                block_id,
                state,
                &format!("%filewbbad{dest}"),
                ok,
                bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
                vec![
                    ("index", Fact::Runtime("{}", format!("%filewbidx{dest}"))),
                    ("bound", Fact::Runtime("{}", format!("%filewblen{dest}"))),
                    ("context", Fact::Text("BYTE buffer".into())),
                ],
            );
            let args = vec![handle, (T::Ptr, r("filewbptr")), (T::I64, count)];
            emit_void_result(text, destination, call("bn_rt_file_write_bytes", args));
        }
        _ => return false,
    }
    true
}
