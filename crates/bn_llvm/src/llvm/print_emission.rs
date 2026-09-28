// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `PRINT` statements: stream locking, the text of floats by BN type, and
// printing `T OR Error` and sentinel alternatives.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

pub(crate) fn lower_print_emission(
    text: &mut String,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::Print {
            values: printed, ..
        } => {
            let stdout = format!("%stdout{}", state.print_count);
            let (lock, unlock) = crate::platform_stdio::stdout_lock_functions();
            if state.synchronize_prints {
                let _ = writeln!(text, "{}", crate::platform_stdio::stdout_stream_ir(&stdout));
                let _ = writeln!(text, "  call void @{lock}(ptr {stdout})");
            }
            for (index, value) in printed.iter().enumerate() {
                if index > 0 {
                    let _ = writeln!(
                        text,
                        "  %separator{} = call i32 @putchar(i32 32)",
                        state.print_count
                    );
                    state.print_count += 1;
                }
                lower_print_value(
                    text,
                    *value,
                    analysis
                        .values
                        .get(value)
                        .expect("validated printable type"),
                    state,
                );
                if analysis.owned_string_results.contains(value) {
                    let id = value.0;
                    let _ = writeln!(
                        text,
                        "  %ownedstringerror{id} = extractvalue {{ i1, ptr, i64 }} %v{id}, 0"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringptr{id} = extractvalue {{ i1, ptr, i64 }} %v{id}, 1"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringfree{id} = select i1 %ownedstringerror{id}, ptr null, ptr %ownedstringptr{id}"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringna{id} = icmp eq ptr %ownedstringfree{id}, @.bn_na"
                    );
                    let _ = writeln!(
                        text,
                        "  %ownedstringstorage{id} = select i1 %ownedstringna{id}, ptr null, ptr %ownedstringfree{id}"
                    );
                    let _ = writeln!(text, "  call void @free(ptr %ownedstringstorage{id})");
                }
            }
            let _ = writeln!(
                text,
                "  %newline{} = call i32 @putchar(i32 10)",
                state.print_count
            );
            state.print_count += 1;
            if state.synchronize_prints {
                let _ = writeln!(text, "  call void @{unlock}(ptr {stdout})");
            }
        }
        _ => return false,
    }
    true
}

/// Floats reach `bn_rt` widened to `double`; the symbol keeps the BN type so
/// the text round-trips to it (console.md).
pub(crate) const fn float_print_symbol(kind: FloatType) -> &'static str {
    match kind {
        FloatType::Float32 => "bn_rt_print_float32",
        FloatType::Float64 => "bn_rt_print_float",
    }
}

pub(crate) fn lower_print_language_error_union(
    text: &mut String,
    value: ValueId,
    integer_value: bool,
    void_value: bool,
    sentinel: Option<Sentinel>,
    scalar: Option<&Type>,
    state: &mut EmissionState,
) {
    let count = state.print_count;
    let _ = writeln!(
        text,
        "  %unionerror{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
        value.0
    );
    let _ = writeln!(
        text,
        "  %unionmessage{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
        value.0
    );
    let _ = writeln!(
        text,
        "  %unionpayload{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
        value.0
    );
    let _ = writeln!(
        text,
        "  br i1 %unionerror{count}, label %unionerr{count}, label %unionvalue{count}"
    );
    state.control_flow.label(text, format!("unionerr{count}"));
    let _ = writeln!(
        text,
        "  call void @bn_rt_error_print(i64 %unionpayload{count}, ptr %unionmessage{count})"
    );
    let _ = writeln!(text, "  br label %unionjoin{count}");
    state.control_flow.label(text, format!("unionvalue{count}"));
    if let Some(sentinel) = sentinel {
        let (global, array) = sentinel.global();
        let _ = writeln!(
            text,
            "  %unionnaptr{count} = getelementptr {array}, ptr {global}, i64 0, i64 0"
        );
        let _ = writeln!(
            text,
            "  %unionisna{count} = icmp eq ptr %unionmessage{count}, %unionnaptr{count}"
        );
        let _ = writeln!(
            text,
            "  br i1 %unionisna{count}, label %unionna{count}, label %unionpresent{count}"
        );
        state.control_flow.label(text, format!("unionna{count}"));
        let _ = writeln!(
            text,
            "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %unionnaptr{count})"
        );
        let _ = writeln!(text, "  br label %unionjoin{count}");
        state
            .control_flow
            .label(text, format!("unionpresent{count}"));
    }
    if let Some(Type::Float(kind)) = scalar {
        let _ = writeln!(
            text,
            "  %unionfloat{count} = bitcast i64 %unionpayload{count} to double"
        );
        let _ = writeln!(
            text,
            "  call void @{}(double %unionfloat{count})",
            float_print_symbol(*kind)
        );
    } else if matches!(scalar, Some(Type::Boolean)) {
        let _ = writeln!(
            text,
            "  %unionbool{count} = icmp ne i64 %unionpayload{count}, 0"
        );
        let _ = writeln!(
            text,
            "  %unionboolstr{count} = select i1 %unionbool{count}, ptr @.bn_true, ptr @.bn_false"
        );
        let _ = writeln!(
            text,
            "  call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %unionboolstr{count})"
        );
    } else if integer_value || matches!(scalar, Some(Type::Integer(_))) {
        let _ = writeln!(
            text,
            "  %unionintprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %unionpayload{count})"
        );
    } else if void_value {
        let _ = writeln!(
            text,
            "  %unionnullprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr @.bn_null)"
        );
    } else {
        let _ = writeln!(
            text,
            "  %unionstrprint{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %unionmessage{count})"
        );
    }
    let _ = writeln!(text, "  br label %unionjoin{count}");
    state.control_flow.label(text, format!("unionjoin{count}"));
    state.print_count += 1;
}

/// `PRINT` of a HOST handle `OR Error` (`FS.File OR Error`, `HOST.Net.TCPStream
/// OR Error`): the `Error`, or the handle's type name as the interpreter
/// prints it. False for any other type.
pub(crate) fn lower_print_handle_error_union(
    text: &mut String,
    value: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) -> bool {
    let Type::Alternative(alternatives) = ty else {
        return false;
    };
    let [first, second] = alternatives.as_slice() else {
        return false;
    };
    let name = match (first, second) {
        (Type::Named(name), other) | (other, Type::Named(name))
            if is_error_type(other) && name != "Error" && name != "VOID" =>
        {
            name
        }
        _ => return false,
    };
    if llvm_type(ty) != Some("{ i1, ptr, i64 }") {
        return false;
    }
    let count = state.print_count;
    let label = type_name_global(name);
    let _ = writeln!(
        text,
        "  %handleerror{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 0\n  %handleptr{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 1\n  %handlecode{count} = extractvalue {{ i1, ptr, i64 }} %v{}, 2\n  br i1 %handleerror{count}, label %handleerr{count}, label %handleok{count}",
        value.0, value.0, value.0
    );
    state.control_flow.label(text, format!("handleerr{count}"));
    let _ = writeln!(
        text,
        "  call void @bn_rt_error_print(i64 %handlecode{count}, ptr %handleptr{count})\n  br label %handlejoin{count}"
    );
    state.control_flow.label(text, format!("handleok{count}"));
    let _ = writeln!(
        text,
        "  %handlename{count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr {label})\n  br label %handlejoin{count}"
    );
    state.control_flow.label(text, format!("handlejoin{count}"));
    state.print_count += 1;
    true
}

/// The global holding `name` as a C string, defined at the end of the module
/// by [`define_type_name_globals`]: the symbol carries the name in hex.
fn type_name_global(name: &str) -> String {
    let hex = name.bytes().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    format!("@.bn_typename_{hex}")
}

/// Defines every `@.bn_typename_<hex>` the module references.
pub(crate) fn define_type_name_globals(text: &mut String) {
    let mut names = std::collections::BTreeSet::new();
    for (index, _) in text.match_indices("@.bn_typename_") {
        let hex: String = text[index + "@.bn_typename_".len()..]
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        names.insert(hex);
    }
    for hex in names {
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .filter_map(|at| u8::from_str_radix(&hex[at..at + 2], 16).ok())
            .collect();
        let _ = writeln!(
            text,
            "@.bn_typename_{hex} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            bytes.len() + 1,
            escape_llvm(&String::from_utf8_lossy(&bytes))
        );
    }
}
