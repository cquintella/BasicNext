// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native `HOST.FileSystem`: which calls `bnc` supports, and their lowering
// onto the `bn_rt_file_*` / `bn_rt_fs_*` C ABI (semantics in `bn_rt::file`).
#![allow(clippy::wildcard_imports)]
use super::*;

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
    match name {
        "HOST.FileSystem.Open" => {
            let dest = destination.0;
            let mode = extend_to_i32(
                text,
                arguments[1],
                analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated file mode"),
            );
            let _ = writeln!(text, "  %fileout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %filerc{dest} = call i32 @bn_rt_file_open(ptr %v{}, i32 {mode}, ptr %fileout{dest})",
                arguments[0].0
            );
            let _ = writeln!(text, "  %filehandle{dest} = load i64, ptr %fileout{dest}");
            emit_handle_result(
                text,
                destination,
                format!("%filerc{dest}"),
                format!("%filehandle{dest}"),
            );
        }
        "FS.File.Close" => {
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %fileclosehandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            emit_void_result(
                text,
                destination,
                format!("call i32 @bn_rt_file_close(i64 %fileclosehandle{dest})"),
            );
        }
        "HOST.FileSystem.Exists" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %fsexout{dest} = alloca i32");
            let _ = writeln!(
                text,
                "  %fsexrc{dest} = call i32 @bn_rt_fs_exists(ptr %v{}, ptr %fsexout{dest})",
                arguments[0].0
            );
            let _ = writeln!(text, "  %fsexval{dest} = load i32, ptr %fsexout{dest}");
            let _ = writeln!(text, "  %fsexpay{dest} = zext i32 %fsexval{dest} to i64");
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
            emit_void_result(
                text,
                destination,
                format!("call i32 @bn_rt_fs_delete_file(ptr %v{})", arguments[0].0),
            );
        }
        "FS.File.Write" | "FS.File.WriteLine" => {
            let dest = destination.0;
            let symbol = if name == "FS.File.Write" {
                "bn_rt_file_write"
            } else {
                "bn_rt_file_write_line"
            };
            let _ = writeln!(
                text,
                "  %filewhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            emit_void_result(
                text,
                destination,
                format!(
                    "call i32 @{symbol}(i64 %filewhandle{dest}, ptr %v{})",
                    arguments[1].0
                ),
            );
        }
        "FS.File.ReadAll" | "FS.File.ReadLine" => {
            let dest = destination.0;
            let (symbol, eof) = if name == "FS.File.ReadAll" {
                ("bn_rt_file_read_all", None)
            } else {
                ("bn_rt_file_read_line", Some(4))
            };
            let _ = writeln!(
                text,
                "  %filerhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            let _ = writeln!(text, "  %filerout{dest} = alloca ptr");
            let _ = writeln!(
                text,
                "  %filerrc{dest} = call i32 @{symbol}(i64 %filerhandle{dest}, ptr %filerout{dest})"
            );
            let _ = writeln!(text, "  %filerdata{dest} = load ptr, ptr %filerout{dest}");
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
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %filebhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            let _ = writeln!(
                text,
                "  %filebptr{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
                arguments[1].0
            );
            let _ = writeln!(
                text,
                "  %filebcap{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
                arguments[1].0
            );
            let _ = writeln!(
                text,
                "  %filebcap64{dest} = sext i32 %filebcap{dest} to i64"
            );
            let _ = writeln!(text, "  %filebout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %filebrc{dest} = call i32 @bn_rt_file_read_bytes(i64 %filebhandle{dest}, ptr %filebptr{dest}, i64 %filebcap64{dest}, ptr %filebout{dest})"
            );
            let _ = writeln!(text, "  %filebcount{dest} = load i64, ptr %filebout{dest}");
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
            let dest = destination.0;
            let count_ty = analysis
                .values
                .get(&arguments[2])
                .expect("validated byte count");
            let count = coerce_to_type(
                text,
                arguments[2],
                count_ty,
                &Type::Integer(IntegerType::Int64),
            );
            let _ = writeln!(
                text,
                "  %filewbhandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            let _ = writeln!(
                text,
                "  %filewbptr{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
                arguments[1].0
            );
            let _ = writeln!(
                text,
                "  %filewbcap{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
                arguments[1].0
            );
            let _ = writeln!(
                text,
                "  %filewbcap64{dest} = sext i32 %filewbcap{dest} to i64"
            );
            // host.md: `count` outside 0..=LEN(buffer) is INDEX_OUT_OF_BOUNDS,
            // the same trap as vector indexing.
            let _ = writeln!(text, "  %filewbneg{dest} = icmp slt i64 {count}, 0");
            let _ = writeln!(
                text,
                "  %filewbover{dest} = icmp sgt i64 {count}, %filewbcap64{dest}"
            );
            let _ = writeln!(
                text,
                "  %filewbbad{dest} = or i1 %filewbneg{dest}, %filewbover{dest}"
            );
            let ok = take_continuation(block_id, state);
            let _ = writeln!(
                text,
                "  br i1 %filewbbad{dest}, label %trap_numeric_overflow, label %{ok}"
            );
            state.control_flow.label(text, ok);
            state.needs_numeric_overflow_trap = true;
            emit_void_result(
                text,
                destination,
                format!(
                    "call i32 @bn_rt_file_write_bytes(i64 %filewbhandle{dest}, ptr %filewbptr{dest}, i64 {count})"
                ),
            );
        }
        _ => return false,
    }
    true
}
