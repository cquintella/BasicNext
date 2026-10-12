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

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_cast(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source_ty: &Type,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    // An absence (`NULL`, `NA`, `EOF`) is FALSE as BOOLEAN, as in the
    // interpreter (T.K9).
    if matches!(source_ty, Type::Null | Type::NotAvailable | Type::EndOfFile) {
        super::emission3::emit_boolean_assignment(text, destination, false);
        return;
    }
    if *target_ty == Type::String && *source_ty != Type::String {
        emit_to_string(text, destination, value, source_ty);
        return;
    }
    let own = format!("v{}", destination.0);
    match (llvm_type(source_ty), llvm_type(target_ty)) {
        (Some(source), Some(T::I1)) => {
            emit_to_boolean(text, destination, value, source);
        }
        (Some(source), Some(target)) if source.is_integer() && target.is_integer() => {
            let ext = if is_unsigned(source_ty) {
                CastOp::ZExt
            } else {
                CastOp::SExt
            };
            let wide = I::cast(ext, source, v(value), T::I128);
            text.assign(format!("castw{}", destination.0), wide);
            emit_i128_fit_trunc(text, block_id, destination.0, target, target_ty, state);
        }
        (Some(source), Some(target)) if source.is_integer() && target.is_floating() => {
            let op = if is_unsigned(source_ty) {
                CastOp::UIToFP
            } else {
                CastOp::SIToFP
            };
            text.assign(own, I::cast(op, source, v(value), target));
        }
        (Some(source), Some(target)) if source.is_floating() && target.is_integer() => {
            emit_float_to_integer(
                text,
                block_id,
                destination,
                value,
                &source,
                target,
                target_ty,
                state,
            );
        }
        (Some(T::Float), Some(T::Double)) => {
            text.assign(own, I::cast(CastOp::FPExt, T::Float, v(value), T::Double));
        }
        (Some(T::Double), Some(T::Float)) => {
            text.assign(own, I::cast(CastOp::FPTrunc, T::Double, v(value), T::Float));
        }
        (Some(T::Ptr), Some(T::Ptr))
        | (Some(T::Float), Some(T::Float))
        | (Some(T::Double), Some(T::Double)) => {
            emit_same_type_copy(text, destination, value, target_ty);
        }
        (Some(source), Some(target))
            if source == crate::layout::vector_ty() && target == crate::layout::region_ty() =>
        {
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
            let fat_void = crate::layout::region_ty();
            text.assign(
                ptr_val.clone(),
                I::extract(crate::layout::vector_ty(), val.clone(), 0),
            );
            text.assign(
                len_val.clone(),
                I::extract(crate::layout::vector_ty(), val, 1),
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
        (Some(source), Some(target))
            if source == crate::layout::region_ty() && target == crate::layout::vector_ty() =>
        {
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
                I::select(O::bool(true), crate::layout::vector_ty(), res.clone(), res),
            );
        }
        _ => unreachable!("validated cast shape"),
    }
}

pub(crate) fn emit_pointer_void_restore(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    tag: &str,
    val: O,
    target_ty: &Type,
    state: &mut EmissionState,
) -> crate::ir::LlvmOperand {
    let r = |name: &str| O::reg(format!("vp{name}{tag}"));
    let fat_void = crate::layout::region_ty();
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
        &O::reg(&type_mismatch),
        after_type_check,
        bn_diag::DiagId::POINTER_TYPE_MISMATCH,
        vec![(
            "message",
            Fact::Text(bn_diag::trap_texts::POINTER_TYPE_MISMATCH.into()),
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
            &O::reg(&len_mismatch),
            after_len_check,
            bn_diag::DiagId::POINTER_LENGTH_MISMATCH,
            vec![(
                "message",
                Fact::RuntimeDynamic(
                    format!("pointer length {{}} does not match {expected}"),
                    O::reg(&wide_len),
                ),
            )],
        );
    }

    let fat_typed = crate::layout::vector_ty();
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
    O::reg(&res)
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
        ) | (
            Type::Boolean | Type::Null | Type::NotAvailable | Type::EndOfFile,
            Type::Boolean
        ) | (Type::String, Type::Boolean | Type::String)
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
fn emit_to_string(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    value: ValueId,
    source_ty: &Type,
) {
    let dest = destination.0;
    let source = llvm_type(source_ty).expect("validated text cast source");
    let wide = O::reg(format!("textwide{dest}"));
    let widen = |text: &mut crate::ir::LlvmModule, op, to| {
        let inst = I::cast(op, source.clone(), v(value), to);
        text.assign(format!("textwide{dest}"), inst);
    };
    let integer = if is_unsigned(source_ty) {
        ("bn_rt_text_uint", CastOp::ZExt)
    } else {
        ("bn_rt_text_int", CastOp::SExt)
    };
    let (symbol, argument) = match source {
        T::I1 => {
            let words = I::select(
                v(value),
                T::Ptr,
                O::global(".bn_true"),
                O::global(".bn_false"),
            );
            text.assign(format!("v{dest}"), words);
            return;
        }
        T::I64 => (integer.0, (T::I64, v(value))),
        T::I8 | T::I16 | T::I32 => {
            widen(text, integer.1, T::I64);
            (integer.0, (T::I64, wide))
        }
        T::Float => {
            widen(text, CastOp::FPExt, T::Double);
            ("bn_rt_text_float32", (T::Double, wide))
        }
        T::Double => ("bn_rt_text_float", (T::Double, v(value))),
        _ => unreachable!("validated text cast source"),
    };
    text.assign(format!("v{dest}"), I::call(T::Ptr, symbol, vec![argument]));
}

fn emit_same_type_copy(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    value: ValueId,
    ty: &Type,
) {
    let llvm_ty = llvm_type(ty).expect("validated copy type");
    let inst = match llvm_ty {
        T::I1 => I::binary(BinaryOp::Or, T::I1, O::bool(false), v(value)),
        T::Ptr => I::gep(T::I8, v(value), vec![(T::I64, O::int(0))]),
        T::Float | T::Double => I::binary(BinaryOp::FAdd, llvm_ty, O::float(0.0), v(value)),
        other => I::binary(BinaryOp::Add, other, O::int(0), v(value)),
    };
    text.assign(format!("v{}", destination.0), inst);
}

fn emit_to_boolean(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    value: ValueId,
    source: T,
) {
    let dest = destination.0;
    let inst = match source {
        T::I1 => I::binary(BinaryOp::Or, T::I1, O::bool(false), v(value)),
        integer if integer.is_integer() => I::icmp(ICmpCond::Ne, integer, v(value), O::int(0)),
        floating if floating.is_floating() => {
            I::fcmp(FCmpCond::Une, floating, v(value), O::float(0.0))
        }
        T::Ptr => {
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
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    value: ValueId,
    source: &T,
    target: T,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("cast{name}{dest}"));
    let compare = |text: &mut crate::ir::LlvmModule, name: &str, cond, constant: f64| {
        let inst = I::fcmp(cond, source.clone(), v(value), O::float(constant));
        text.assign(format!("cast{name}{dest}"), inst);
    };
    compare(text, "nan", FCmpCond::Uno, 0.0);
    compare(text, "pinf", FCmpCond::Oeq, f64::INFINITY);
    compare(text, "ninf", FCmpCond::Oeq, f64::NEG_INFINITY);
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
        &O::reg(format!("castbad{dest}")),
        finite,
        bn_diag::DiagId::INVALID_NUMERIC_CONVERSION,
        vec![(
            "message",
            Fact::Text(bn_diag::trap_texts::NAN_OR_INF_TO_INT.into()),
        )],
    );
    let wide = I::cast(CastOp::FPToSI, source.clone(), v(value), T::I128);
    text.assign(format!("castw{dest}"), wide);
    emit_i128_fit_trunc(text, block_id, dest, target, target_ty, state);
}

fn emit_i128_fit_trunc(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    dest: u32,
    target: T,
    target_ty: &Type,
    state: &mut EmissionState,
) {
    let r = |name: &str| O::reg(format!("cast{name}{dest}"));
    let (min, max) = i128_bounds(target_ty);
    text.assign(
        format!("castlo{dest}"),
        I::icmp(ICmpCond::Slt, T::I128, r("w"), min),
    );
    text.assign(
        format!("casthi{dest}"),
        I::icmp(ICmpCond::Sgt, T::I128, r("w"), max),
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
        &O::reg(format!("castov{dest}")),
        ok,
        &O::reg(format!("castw{dest}")),
        target_ty,
    );
    let narrow = I::cast(CastOp::Trunc, T::I128, r("w"), target);
    text.assign(format!("v{dest}"), narrow);
}

/// The inclusive range of an integer type as `i128` literals.
pub(crate) fn i128_bounds(ty: &Type) -> (O, O) {
    let (low, high) = crate::helpers::integer_range(integer_kind(ty));
    (O::int128(low), O::int128(high))
}
