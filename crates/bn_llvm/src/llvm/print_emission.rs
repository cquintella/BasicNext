// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `PRINT` statements: stream locking, the text of floats by BN type, and
// printing `T OR Error` and sentinel alternatives.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{handle_result_ty, typed_llvm};

/// `printf(fmt, value)`, named `dest` when it is `Some`.
fn printf(text: &mut String, dest: Option<String>, format: &str, value: (T, O)) {
    let args = vec![(T::Ptr, O::global(format)), value];
    let call = I::call_variadic(T::I32, vec![T::Ptr], "printf", args);
    match dest {
        Some(dest) => text.assign(dest, call),
        None => text.emit(call),
    }
}

fn putchar(text: &mut String, dest: String, byte: i64) {
    text.assign(
        dest,
        I::call(T::I32, "putchar", vec![(T::I32, O::int(byte))]),
    );
}

fn br(text: &mut String, dest: String) {
    text.emit(I::Br { dest });
}

fn cond_br(text: &mut String, cond: O, yes: String, no: String) {
    text.emit(I::CondBr {
        cond,
        true_dest: yes,
        false_dest: no,
    });
}

pub(crate) fn lower_print_emission(
    text: &mut String,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    let Instruction::Print {
        values: printed, ..
    } = instruction
    else {
        return false;
    };
    let stdout = format!("stdout{}", state.print_count);
    let (lock, unlock) = crate::platform_stdio::stdout_lock_functions();
    let stream = |text: &mut String, function: &str| {
        let args = vec![(T::Ptr, O::reg(&stdout))];
        text.emit(I::call(T::Void, function, args));
    };
    if state.synchronize_prints {
        text.assign(&stdout, crate::platform_stdio::stdout_stream());
        stream(text, lock);
    }
    for (index, value) in printed.iter().enumerate() {
        if index > 0 {
            putchar(text, format!("separator{}", state.print_count), 32);
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
            let r = |name: &str| O::reg(format!("ownedstring{name}{id}"));
            let union = handle_result_ty();
            let own = O::reg(format!("v{id}"));
            text.assign(
                format!("ownedstringerror{id}"),
                I::extract(union.clone(), own.clone(), 0),
            );
            text.assign(format!("ownedstringptr{id}"), I::extract(union, own, 1));
            let free = I::select(r("error"), T::Ptr, O::null(), r("ptr"));
            text.assign(format!("ownedstringfree{id}"), free);
            let na = I::icmp(ICmpCond::Eq, T::Ptr, r("free"), O::global(".bn_na"));
            text.assign(format!("ownedstringna{id}"), na);
            let storage = I::select(r("na"), T::Ptr, O::null(), r("free"));
            text.assign(format!("ownedstringstorage{id}"), storage);
            text.emit(I::call(T::Void, "free", vec![(T::Ptr, r("storage"))]));
        }
    }
    putchar(text, format!("newline{}", state.print_count), 10);
    state.print_count += 1;
    if state.synchronize_prints {
        stream(text, unlock);
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
    let r = |name: &str| O::reg(format!("union{name}{count}"));
    let label = |name: &str| format!("union{name}{count}");
    let union = handle_result_ty();
    let own = O::reg(format!("v{}", value.0));
    for (index, name) in ["error", "message", "payload"].into_iter().enumerate() {
        text.assign(label(name), I::extract(union.clone(), own.clone(), index));
    }
    cond_br(text, r("error"), label("err"), label("value"));
    state.control_flow.label(text, label("err"));
    let args = vec![(T::I64, r("payload")), (T::Ptr, r("message"))];
    text.emit(I::call(T::Void, "bn_rt_error_print", args));
    br(text, label("join"));
    state.control_flow.label(text, label("value"));
    if let Some(sentinel) = sentinel {
        let (global, array) = sentinel.global();
        let zero = vec![(T::I64, O::int(0)), (T::I64, O::int(0))];
        let marker = I::gep(typed_llvm(array), O::global(global), zero);
        text.assign(label("naptr"), marker);
        let is_na = I::icmp(ICmpCond::Eq, T::Ptr, r("message"), r("naptr"));
        text.assign(label("isna"), is_na);
        cond_br(text, r("isna"), label("na"), label("present"));
        state.control_flow.label(text, label("na"));
        printf(text, None, ".bn_fmt_str", (T::Ptr, r("naptr")));
        br(text, label("join"));
        state.control_flow.label(text, label("present"));
    }
    if let Some(Type::Float(kind)) = scalar {
        let float = I::cast(CastOp::BitCast, T::I64, r("payload"), T::Double);
        text.assign(label("float"), float);
        let args = vec![(T::Double, r("float"))];
        text.emit(I::call(T::Void, float_print_symbol(*kind), args));
    } else if matches!(scalar, Some(Type::Boolean)) {
        let flag = I::icmp(ICmpCond::Ne, T::I64, r("payload"), O::int(0));
        text.assign(label("bool"), flag);
        let words = I::select(
            r("bool"),
            T::Ptr,
            O::global(".bn_true"),
            O::global(".bn_false"),
        );
        text.assign(label("boolstr"), words);
        printf(text, None, ".bn_fmt_str", (T::Ptr, r("boolstr")));
    } else if integer_value || matches!(scalar, Some(Type::Integer(_))) {
        printf(
            text,
            Some(label("intprint")),
            ".bn_fmt_int",
            (T::I64, r("payload")),
        );
    } else if void_value {
        let null = (T::Ptr, O::global(".bn_null"));
        printf(text, Some(label("nullprint")), ".bn_fmt_str", null);
    } else {
        printf(
            text,
            Some(label("strprint")),
            ".bn_fmt_str",
            (T::Ptr, r("message")),
        );
    }
    br(text, label("join"));
    state.control_flow.label(text, label("join"));
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
    let r = |name: &str| O::reg(format!("handle{name}{count}"));
    let label = |name: &str| format!("handle{name}{count}");
    let union = handle_result_ty();
    let own = O::reg(format!("v{}", value.0));
    for (index, name) in ["error", "ptr", "code"].into_iter().enumerate() {
        text.assign(label(name), I::extract(union.clone(), own.clone(), index));
    }
    cond_br(text, r("error"), label("err"), label("ok"));
    state.control_flow.label(text, label("err"));
    let args = vec![(T::I64, r("code")), (T::Ptr, r("ptr"))];
    text.emit(I::call(T::Void, "bn_rt_error_print", args));
    br(text, label("join"));
    state.control_flow.label(text, label("ok"));
    let type_name = (T::Ptr, O::raw(type_name_global(name)));
    printf(text, Some(label("name")), ".bn_fmt_str", type_name);
    br(text, label("join"));
    state.control_flow.label(text, label("join"));
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
