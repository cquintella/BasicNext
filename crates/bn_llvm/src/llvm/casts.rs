// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `AS` conversions in native code: which casts `bnc` supports and their
// lowering, including range-checked integer narrowing and `AS STRING` (C3).
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, FCmpCond, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O,
    LlvmType as T,
};
use crate::layout::typed_llvm;

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_cast(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source_ty: &Type,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    if *target_ty == Type::String && *source_ty != Type::String {
        emit_to_string(text, destination, value, source_ty);
        return;
    }
    let own = format!("v{}", destination.0);
    match (llvm_type(source_ty), llvm_type(target_ty)) {
        (Some(source), Some("i1")) => emit_to_boolean(text, destination, value, source),
        (Some(source), Some(target)) if integer_llvm(source) && integer_llvm(target) => {
            let ext = if is_unsigned(source_ty) {
                CastOp::ZExt
            } else {
                CastOp::SExt
            };
            let wide = I::cast(ext, typed_llvm(source), v(value), T::I128);
            text.assign(format!("castw{}", destination.0), wide);
            emit_i128_fit_trunc(text, block_id, destination.0, target, target_ty, state);
        }
        (Some(source), Some(target)) if integer_llvm(source) && float_llvm(target) => {
            let op = if is_unsigned(source_ty) {
                CastOp::UIToFP
            } else {
                CastOp::SIToFP
            };
            text.assign(
                own,
                I::cast(op, typed_llvm(source), v(value), typed_llvm(target)),
            );
        }
        (Some(source), Some(target)) if float_llvm(source) && integer_llvm(target) => {
            emit_float_to_integer(
                text,
                block_id,
                destination,
                value,
                source,
                target,
                target_ty,
                state,
            );
        }
        (Some("float"), Some("double")) => {
            text.assign(own, I::cast(CastOp::FPExt, T::Float, v(value), T::Double));
        }
        (Some("double"), Some("float")) => {
            text.assign(own, I::cast(CastOp::FPTrunc, T::Double, v(value), T::Float));
        }
        (Some("ptr"), Some("ptr"))
        | (Some("float"), Some("float"))
        | (Some("double"), Some("double")) => {
            emit_same_type_copy(text, destination, value, target_ty);
        }
        (Some("{ ptr, i32 }"), Some("{ ptr, i32, i32 }")) => {
            let elem_code = match source_ty {
                Type::Pointer { element, .. } => {
                    bn_types::alternatives::member_code(element).unwrap_or(0)
                }
                _ => 0,
            };
            let tag = destination.0;
            let val = v(value);
            let ptr_val = format!("pvptr{tag}");
            let len_val = format!("pvlen{tag}");
            let fat_void = typed_llvm("{ ptr, i32, i32 }");
            text.assign(
                ptr_val.clone(),
                I::extract(typed_llvm("{ ptr, i32 }"), val.clone(), 0),
            );
            text.assign(
                len_val.clone(),
                I::extract(typed_llvm("{ ptr, i32 }"), val, 1),
            );
            let ins0 = format!("pvins0_{tag}");
            text.assign(
                ins0.clone(),
                I::insert(fat_void.clone(), O::undef(), T::Ptr, O::reg(ptr_val), 0),
            );
            let ins1 = format!("pvins1_{tag}");
            text.assign(
                ins1.clone(),
                I::insert(fat_void.clone(), O::reg(ins0), T::I32, O::reg(len_val), 1),
            );
            text.assign(
                own,
                I::insert(
                    fat_void,
                    O::reg(ins1),
                    T::I32,
                    O::int(i64::from(elem_code)),
                    2,
                ),
            );
        }
        (Some("{ ptr, i32, i32 }"), Some("{ ptr, i32 }")) => {
            let res = emit_pointer_void_restore(
                text,
                block_id,
                &format!("cast{}", destination.0),
                v(value),
                target_ty,
                state,
            );
            text.assign(
                own,
                I::select(
                    O::bool(true),
                    typed_llvm("{ ptr, i32 }"),
                    O::raw(res.clone()),
                    O::raw(res),
                ),
            );
        }
        _ => unreachable!("validated cast shape"),
    }
}

pub(crate) fn emit_pointer_void_restore(
    text: &mut String,
    block_id: BlockId,
    tag: &str,
    val: O,
    target_ty: &Type,
    state: &mut EmissionState,
) -> String {
    let r = |name: &str| O::reg(format!("vp{name}{tag}"));
    let fat_void = typed_llvm("{ ptr, i32, i32 }");
    text.assign(
        format!("vpptr{tag}"),
        I::extract(fat_void.clone(), val.clone(), 0),
    );
    text.assign(
        format!("vplen{tag}"),
        I::extract(fat_void.clone(), val.clone(), 1),
    );
    text.assign(format!("vpcode{tag}"), I::extract(fat_void, val, 2));

    let (to_code, fixed_len) = match target_ty {
        Type::Pointer { element, length } => {
            let code = bn_types::alternatives::member_code(element).unwrap_or(0);
            let len = match length {
                bn_types::PointerLength::Fixed(n) => Some(*n),
                bn_types::PointerLength::One => Some(1),
                bn_types::PointerLength::Dynamic => None,
            };
            (code, len)
        }
        _ => (0, None),
    };

    let type_ok = format!("vptypeok{tag}");
    let type_mismatch = format!("vptypemm{tag}");
    text.assign(
        type_ok.clone(),
        I::icmp(ICmpCond::Eq, T::I32, r("code"), O::int(i64::from(to_code))),
    );
    text.assign(
        type_mismatch.clone(),
        I::binary(BinaryOp::Xor, T::I1, O::reg(type_ok), O::bool(true)),
    );
    let after_type_check = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &format!("%{type_mismatch}"),
        after_type_check,
        bn_diag::DiagId::POINTER_TYPE_MISMATCH,
        vec![(
            "message",
            Fact::Text("pointer element type does not match destination type".into()),
        )],
    );

    if let Some(expected) = fixed_len {
        let expected_val = i64::try_from(expected).unwrap_or(i64::MAX);
        let len_ok = format!("vplenok{tag}");
        let len_mismatch = format!("vplenmm{tag}");
        text.assign(
            len_ok.clone(),
            I::icmp(ICmpCond::Eq, T::I32, r("len"), O::int(expected_val)),
        );
        text.assign(
            len_mismatch.clone(),
            I::binary(BinaryOp::Xor, T::I1, O::reg(len_ok), O::bool(true)),
        );
        let wide_len = format!("vplenwide{tag}");
        text.assign(
            wide_len.clone(),
            I::cast(CastOp::SExt, T::I32, r("len"), T::I128),
        );
        let after_len_check = take_continuation(block_id, state);
        emit_trap(
            text,
            block_id,
            state,
            &format!("%{len_mismatch}"),
            after_len_check,
            bn_diag::DiagId::POINTER_LENGTH_MISMATCH,
            vec![(
                "message",
                Fact::RuntimeDynamic(
                    format!("pointer length {{}} does not match {expected}"),
                    format!("%{wide_len}"),
                ),
            )],
        );
    }

    let fat_typed = typed_llvm("{ ptr, i32 }");
    let ins0 = format!("vpins0_{tag}");
    text.assign(
        ins0.clone(),
        I::insert(fat_typed.clone(), O::undef(), T::Ptr, r("ptr"), 0),
    );
    let res = format!("vprest_{tag}");
    text.assign(
        res.clone(),
        I::insert(fat_typed, O::reg(ins0), T::I32, r("len"), 1),
    );
    format!("%{res}")
}

/// Whether native code implements `source AS target`.
pub(crate) fn cast_supported(source: Option<&Type>, target: &Type) -> bool {
    let Some(source) = source else {
        return false;
    };
    if matches!(
        (source, target),
        (
            Type::Pointer { .. },
            Type::Pointer { element, .. }
        ) if matches!(element.as_ref(), Type::Named(name) if name == "VOID")
    ) || matches!(
        (source, target),
        (
            Type::Pointer { element, .. },
            Type::Pointer { .. }
        ) if matches!(element.as_ref(), Type::Named(name) if name == "VOID")
    ) {
        return true;
    }
    matches!(
        (source, target),
        (
            Type::Integer(_) | Type::IntegerLiteral(_),
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::Boolean
        ) | (
            Type::Float(_) | Type::FloatLiteral,
            Type::Float(_) | Type::Integer(_) | Type::Boolean
        ) | (
            Type::Integer(_)
                | Type::IntegerLiteral(_)
                | Type::Float(_)
                | Type::FloatLiteral
                | Type::Boolean,
            Type::String
        ) | (Type::Boolean, Type::Boolean)
            | (Type::String, Type::Boolean | Type::String)
    )
}

/// A cast to STRING, which calls the `bn_rt` text ABI (C3).
pub(crate) const fn is_text_cast(instruction: &Instruction) -> bool {
    matches!(
        instruction,
        Instruction::Cast {
            ty: Type::String,
            ..
        }
    )
}

/// `AS STRING` (C3): `bn_rt` formats with the same code the interpreter uses.
fn emit_to_string(text: &mut String, destination: ValueId, value: ValueId, source_ty: &Type) {
    let dest = destination.0;
    let source = llvm_type(source_ty).expect("validated text cast source");
    let wide = O::reg(format!("textwide{dest}"));
    let widen = |text: &mut String, op, to| {
        let inst = I::cast(op, typed_llvm(source), v(value), to);
        text.assign(format!("textwide{dest}"), inst);
    };
    let integer = if is_unsigned(source_ty) {
        ("bn_rt_text_uint", CastOp::ZExt)
    } else {
        ("bn_rt_text_int", CastOp::SExt)
    };
    let (symbol, argument) = match source {
        "i1" => {
            let words = I::select(
                v(value),
                T::Ptr,
                O::global(".bn_true"),
                O::global(".bn_false"),
            );
            text.assign(format!("v{dest}"), words);
            return;
        }
        "i64" => (integer.0, (T::I64, v(value))),
        "i8" | "i16" | "i32" => {
            widen(text, integer.1, T::I64);
            (integer.0, (T::I64, wide))
        }
        "float" => {
            widen(text, CastOp::FPExt, T::Double);
            ("bn_rt_text_float32", (T::Double, wide))
        }
        "double" => ("bn_rt_text_float", (T::Double, v(value))),
        _ => unreachable!("validated text cast source"),
    };
    text.assign(format!("v{dest}"), I::call(T::Ptr, symbol, vec![argument]));
}

fn emit_same_type_copy(text: &mut String, destination: ValueId, value: ValueId, ty: &Type) {
    let llvm_ty = llvm_type(ty).expect("validated copy type");
    let inst = match llvm_ty {
        "i1" => I::binary(BinaryOp::Or, T::I1, O::bool(false), v(value)),
        "ptr" => I::gep(T::I8, v(value), vec![(T::I64, O::int(0))]),
        "float" | "double" => {
            I::binary(BinaryOp::FAdd, typed_llvm(llvm_ty), O::raw("0.0"), v(value))
        }
        other => I::binary(BinaryOp::Add, typed_llvm(other), O::int(0), v(value)),
    };
    text.assign(format!("v{}", destination.0), inst);
}

fn emit_to_boolean(text: &mut String, destination: ValueId, value: ValueId, source: &str) {
    let dest = destination.0;
    let inst = match source {
        "i1" => I::binary(BinaryOp::Or, T::I1, O::bool(false), v(value)),
        "i8" | "i16" | "i32" | "i64" => {
            I::icmp(ICmpCond::Ne, typed_llvm(source), v(value), O::int(0))
        }
        "float" | "double" => I::fcmp(FCmpCond::Une, typed_llvm(source), v(value), O::raw("0.0")),
        "ptr" => {
            text.assign(format!("boolch{dest}"), I::load(T::I8, v(value)));
            I::icmp(
                ICmpCond::Ne,
                T::I8,
                O::reg(format!("boolch{dest}")),
                O::int(0),
            )
        }
        _ => unreachable!("validated boolean cast source"),
    };
    text.assign(format!("v{dest}"), inst);
}

#[allow(clippy::too_many_arguments)]
fn emit_float_to_integer(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source: &str,
    target: &str,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("cast{name}{dest}"));
    let source = typed_llvm(source);
    let compare = |text: &mut String, name: &str, cond, constant: &str| {
        let inst = I::fcmp(cond, source.clone(), v(value), O::raw(constant));
        text.assign(format!("cast{name}{dest}"), inst);
    };
    compare(text, "nan", FCmpCond::Uno, "0.0");
    compare(text, "pinf", FCmpCond::Oeq, "0x7FF0000000000000");
    compare(text, "ninf", FCmpCond::Oeq, "0xFFF0000000000000");
    text.assign(
        format!("castinf{dest}"),
        I::binary(BinaryOp::Or, T::I1, r("pinf"), r("ninf")),
    );
    text.assign(
        format!("castbad{dest}"),
        I::binary(BinaryOp::Or, T::I1, r("nan"), r("inf")),
    );
    let finite = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &format!("%castbad{dest}"),
        finite,
        bn_diag::DiagId::INVALID_NUMERIC_CONVERSION,
        vec![(
            "message",
            Fact::Text("NAN and infinity cannot convert to an integer".into()),
        )],
    );
    let wide = I::cast(CastOp::FPToSI, source.clone(), v(value), T::I128);
    text.assign(format!("castw{dest}"), wide);
    emit_i128_fit_trunc(text, block_id, dest, target, target_ty, state);
}

fn emit_i128_fit_trunc(
    text: &mut String,
    block_id: BlockId,
    dest: u32,
    target: &str,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let r = |name: &str| O::reg(format!("cast{name}{dest}"));
    let (min, max) = i128_bounds(target_ty);
    text.assign(
        format!("castlo{dest}"),
        I::icmp(ICmpCond::Slt, T::I128, r("w"), O::raw(min)),
    );
    text.assign(
        format!("casthi{dest}"),
        I::icmp(ICmpCond::Sgt, T::I128, r("w"), O::raw(max)),
    );
    text.assign(
        format!("castov{dest}"),
        I::binary(BinaryOp::Or, T::I1, r("lo"), r("hi")),
    );
    let ok = take_continuation(block_id, state);
    emit_overflow_trap(
        text,
        block_id,
        state,
        &format!("%castov{dest}"),
        ok,
        &format!("%castw{dest}"),
        target_ty,
    );
    let narrow = I::cast(CastOp::Trunc, T::I128, r("w"), typed_llvm(target));
    text.assign(format!("v{dest}"), narrow);
}

/// The inclusive range of an integer type as `i128` literals.
pub(crate) fn i128_bounds(ty: &Type) -> (&'static str, &'static str) {
    match integer_kind(ty) {
        IntegerType::Byte => ("0", "255"),
        IntegerType::Int8 => ("-128", "127"),
        IntegerType::Int16 => ("-32768", "32767"),
        IntegerType::Int32 => ("-2147483648", "2147483647"),
        IntegerType::Int64 => ("-9223372036854775808", "9223372036854775807"),
        IntegerType::UInt16 => ("0", "65535"),
        IntegerType::UInt32 => ("0", "4294967295"),
        IntegerType::UInt64 => ("0", "18446744073709551615"),
    }
}
