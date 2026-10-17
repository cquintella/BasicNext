#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

pub(crate) fn emit_integer_not(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    operand: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let dest = destination.0;
    if is_unsigned(ty) {
        // NOT u is -u - 1, never representable unsigned (the interpreter
        // reports that exact value).
        let wide = I::cast(CastOp::ZExt, llvm_ty.clone(), v(operand), T::I128);
        text.assign(format!("notwide{dest}"), wide);
        let wide = O::reg(format!("notwide{dest}"));
        text.assign(
            format!("notexact{dest}"),
            I::binary(BinaryOp::Sub, T::I128, O::int(-1), wide),
        );
        let cont = format!("bnot{dest}.dead");
        emit_overflow_trap(
            text,
            block_id,
            state,
            &O::bool(true),
            cont,
            &O::reg(format!("notexact{dest}")),
            ty,
        );
        let zero = I::binary(BinaryOp::Add, llvm_ty, O::int(0), O::int(0));
        text.assign(format!("v{dest}"), zero);
        return;
    }
    text.assign(
        format!("v{dest}"),
        I::binary(BinaryOp::Xor, llvm_ty, v(operand), O::int(-1)),
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_shift(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("sh{name}{dest}"));
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let left_llvm = llvm_type(left_ty).expect("validated shift left type");
    let right_llvm = llvm_type(right_ty).expect("validated shift count type");
    let width = bit_width(ty);
    let count = format!("shcnt{dest}");
    emit_cast_integer(
        text,
        &count,
        v(right),
        &right_llvm,
        &T::I64,
        extend_op(right_ty),
    );
    let negative = if is_unsigned(right_ty) {
        I::binary(BinaryOp::Or, T::I1, O::bool(false), O::bool(false))
    } else {
        I::icmp(ICmpCond::Slt, T::I64, r("cnt"), O::int(0))
    };
    text.assign(format!("shneg{dest}"), negative);
    let wide = I::icmp(ICmpCond::Uge, T::I64, r("cnt"), O::uint(u64::from(width)));
    text.assign(format!("shwide{dest}"), wide);
    text.assign(
        format!("shbad{dest}"),
        I::binary(BinaryOp::Or, T::I1, r("neg"), r("wide")),
    );
    let ok = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &O::reg(format!("shbad{dest}")),
        ok,
        bn_diag::DiagId::INVALID_SHIFT_COUNT,
        vec![(
            "detail",
            Fact::Text(format!("shift count must be in 0..{width}")),
        )],
    );
    text.assign(
        format!("shamt{dest}"),
        I::cast(CastOp::ZExt, T::I64, r("cnt"), T::I128),
    );
    if operator == "SHR" {
        let shift_left = if left_llvm == llvm_ty {
            v(left)
        } else {
            let narrow = I::cast(CastOp::Trunc, left_llvm, v(left), llvm_ty.clone());
            text.assign(format!("shnarrow{dest}"), narrow);
            r("narrow")
        };
        let bits = format!("shbits{dest}");
        emit_cast_integer(text, &bits, shift_left, &llvm_ty, &T::I128, CastOp::ZExt);
        let raw = I::binary(BinaryOp::LShr, T::I128, r("bits"), r("amt"));
        text.assign(format!("shraw{dest}"), raw);
        text.assign(
            format!("v{dest}"),
            I::cast(CastOp::Trunc, T::I128, r("raw"), llvm_ty),
        );
        return;
    }
    let value = format!("shval{dest}");
    emit_cast_integer(
        text,
        &value,
        v(left),
        &left_llvm,
        &T::I128,
        extend_op(left_ty),
    );
    text.assign(
        format!("shraw{dest}"),
        I::binary(BinaryOp::Shl, T::I128, r("val"), r("amt")),
    );
    emit_i128_range_trunc(text, block_id, dest, llvm_ty, ty, state);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_integer_power(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("p{name}{dest}"));
    let llvm_ty = llvm_type(ty).expect("validated integer type");
    let left_llvm = llvm_type(left_ty).expect("validated power base type");
    let right_llvm = llvm_type(right_ty).expect("validated power exponent type");
    let base = format!("pbase{dest}");
    emit_cast_integer(
        text,
        &base,
        v(left),
        &left_llvm,
        &T::I128,
        extend_op(left_ty),
    );
    let exponent = format!("pexp{dest}");
    emit_cast_integer(
        text,
        &exponent,
        v(right),
        &right_llvm,
        &T::I128,
        extend_op(right_ty),
    );
    text.assign(
        format!("pneg{dest}"),
        I::icmp(ICmpCond::Slt, T::I128, r("exp"), O::int(0)),
    );
    let big = I::icmp(
        ICmpCond::Ugt,
        T::I128,
        r("exp"),
        O::uint(u64::from(u32::MAX)),
    );
    text.assign(format!("pbig{dest}"), big);
    let positive = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &O::reg(format!("pneg{dest}")),
        positive,
        bn_diag::DiagId::INVALID_EXPONENT,
        vec![(
            "detail",
            Fact::Text(bn_diag::trap_texts::EXPONENT_NEGATIVE.into()),
        )],
    );
    let setup = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &O::reg(format!("pbig{dest}")),
        setup.clone(),
        bn_diag::DiagId::INVALID_EXPONENT,
        vec![(
            "detail",
            Fact::Text(bn_diag::trap_texts::EXPONENT_TOO_LARGE.into()),
        )],
    );
    let label = |name: &str| format!("b{}.pow{dest}.{name}", block_id.0);
    let (loop_h, work, mulr) = (label("loop"), label("work"), label("mulr"));
    let (after, square, done) = (label("after"), label("sq"), label("done"));
    let br = |text: &mut crate::ir::LlvmModule, dest: &str| text.emit(I::Br { dest: dest.into() });
    let cond_br = |text: &mut crate::ir::LlvmModule, cond: O, yes: &str, no: &str| {
        text.emit(I::CondBr {
            cond,
            true_dest: yes.into(),
            false_dest: no.into(),
        });
    };
    let phi = |incoming: Vec<(O, &String)>| I::Phi {
        ty: T::I128,
        incoming: incoming
            .into_iter()
            .map(|(value, from)| (value, from.clone()))
            .collect(),
    };
    br(text, &loop_h);
    state.control_flow.label(text, loop_h.clone());
    let square_ok = format!("{square}.ok");
    let mulr_ok = format!("{mulr}.ok");
    text.assign(
        format!("pb{dest}"),
        phi(vec![(r("base"), &setup), (r("b2"), &square_ok)]),
    );
    text.assign(
        format!("pe{dest}"),
        phi(vec![(r("exp"), &setup), (r("e1"), &square_ok)]),
    );
    text.assign(
        format!("pr{dest}"),
        phi(vec![(O::int(1), &setup), (r("r2"), &square_ok)]),
    );
    text.assign(
        format!("pez{dest}"),
        I::icmp(ICmpCond::Eq, T::I128, r("e"), O::int(0)),
    );
    cond_br(text, r("ez"), &done, &work);
    state.control_flow.label(text, work.clone());
    text.assign(
        format!("podd{dest}"),
        I::cast(CastOp::Trunc, T::I128, r("e"), T::I1),
    );
    cond_br(text, r("odd"), &mulr, &after);
    state.control_flow.label(text, mulr.clone());
    emit_checked_i128_mul(text, block_id, state, dest, "pr", "pb", "prm", &mulr);
    br(text, &after);
    state.control_flow.label(text, after.clone());
    text.assign(
        format!("pr2{dest}"),
        phi(vec![(r("rm"), &mulr_ok), (r("r"), &work)]),
    );
    text.assign(
        format!("pe1{dest}"),
        I::binary(BinaryOp::LShr, T::I128, r("e"), O::int(1)),
    );
    text.assign(
        format!("pmore{dest}"),
        I::icmp(ICmpCond::Ne, T::I128, r("e1"), O::int(0)),
    );
    cond_br(text, r("more"), &square, &done);
    state.control_flow.label(text, square.clone());
    emit_checked_i128_mul(text, block_id, state, dest, "pb", "pb", "pb2", &square);
    br(text, &loop_h);
    state.control_flow.label(text, done.clone());
    text.assign(
        format!("shraw{dest}"),
        phi(vec![(r("r"), &loop_h), (r("r2"), &after)]),
    );
    emit_i128_range_trunc(text, block_id, dest, llvm_ty, ty, state);
}

pub(crate) fn emit_float_power(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    ty: &Type,
) {
    let llvm_ty = llvm_type(ty).expect("validated float type");
    let intrinsic = match llvm_ty {
        T::Float => "llvm.pow.f32",
        T::Double => "llvm.pow.f64",
        _ => unreachable!("validated float power type"),
    };
    let left_op = float_power_operand(text, left, left_ty, &llvm_ty);
    let right_op = float_power_operand(text, right, right_ty, &llvm_ty);
    let args = vec![(llvm_ty.clone(), left_op), (llvm_ty.clone(), right_op)];
    text.assign(
        format!("v{}", destination.0),
        I::call(llvm_ty, intrinsic, args),
    );
}

fn float_power_operand(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    from: &Type,
    target: &T,
) -> O {
    let source = llvm_type(from).expect("validated power operand");
    if source == *target {
        return v(value);
    }
    let opcode = if source.is_integer() {
        if is_unsigned(from) {
            CastOp::UIToFP
        } else {
            CastOp::SIToFP
        }
    } else {
        CastOp::FPExt
    };
    let name = format!("powarg{}_{}", value.0, text.fresh_tag());
    text.assign(
        name.clone(),
        I::cast(opcode, source, v(value), target.clone()),
    );
    O::reg(name)
}

pub(crate) fn emit_string_concat(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    right: ValueId,
) {
    let dest = destination.0;
    let call = I::call(
        T::Ptr,
        "bn_rt_str_concat",
        vec![
            (T::Ptr, v(left)),
            (T::I64, O::int(-1)),
            (T::Ptr, v(right)),
            (T::I64, O::int(-1)),
        ],
    );
    text.assign(format!("v{dest}"), call);
}

/// The intrinsic a `^` on `ty` calls: `llvm.pow` for floating point, the
/// checked `i128` multiplication for integers.
pub(crate) fn pow_intrinsic(ty: &Type) -> Option<crate::ir::Intrinsic> {
    use crate::{
        ir::LlvmType,
        runtime_abi::{float_power, overflow},
    };
    match llvm_type(ty)? {
        T::Float => Some(float_power("llvm.pow.f32", LlvmType::Float)),
        T::Double => Some(float_power("llvm.pow.f64", LlvmType::Double)),
        T::I8 | T::I16 | T::I32 | T::I64 => {
            Some(overflow("llvm.smul.with.overflow.i128", LlvmType::I128))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_checked_i128_mul(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    state: &mut EmissionState,
    dest: u32,
    left: &str,
    right: &str,
    out: &str,
    from: &str,
) {
    let pair = T::struct_of([T::I128, T::I1]);
    let args = vec![
        (T::I128, O::reg(format!("{left}{dest}"))),
        (T::I128, O::reg(format!("{right}{dest}"))),
    ];
    let call = I::call(pair.clone(), "llvm.smul.with.overflow.i128", args);
    text.assign(format!("{out}ov{dest}"), call);
    let product = O::reg(format!("{out}ov{dest}"));
    text.assign(
        format!("{out}{dest}"),
        I::extract(pair.clone(), product.clone(), 0),
    );
    text.assign(format!("{out}f{dest}"), I::extract(pair, product, 1));
    // The power's magnitude passed i128: the interpreter's checked_pow
    // reports it without a value.
    emit_trap(
        text,
        block_id,
        state,
        &O::reg(format!("{out}f{dest}")),
        format!("{from}.ok"),
        bn_diag::DiagId::NUMERIC_OVERFLOW,
        vec![(
            "operation",
            Fact::Text(bn_diag::trap_texts::PERFORMING_INTEGER_OP.into()),
        )],
    );
}

fn emit_i128_range_trunc(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    dest: u32,
    llvm_ty: T,
    ty: &Type,
    state: &mut EmissionState,
) {
    let r = |name: &str| O::reg(format!("sh{name}{dest}"));
    let (min, max) = i128_bounds(ty);
    text.assign(
        format!("shlo{dest}"),
        I::icmp(ICmpCond::Slt, T::I128, r("raw"), min),
    );
    text.assign(
        format!("shhi{dest}"),
        I::icmp(ICmpCond::Sgt, T::I128, r("raw"), max),
    );
    text.assign(
        format!("shov{dest}"),
        I::binary(BinaryOp::Or, T::I1, r("lo"), r("hi")),
    );
    let ok = take_continuation(block_id, state);
    emit_overflow_trap(
        text,
        block_id,
        state,
        &O::reg(format!("shov{dest}")),
        ok,
        &O::reg(format!("shraw{dest}")),
        ty,
    );
    text.assign(
        format!("v{dest}"),
        I::cast(CastOp::Trunc, T::I128, r("raw"), llvm_ty),
    );
}

/// `%{result}` = `value` (of `from`) as `to`: `add 0` when the widths
/// agree, `ext` to widen, `trunc` to narrow.
fn emit_cast_integer(
    text: &mut crate::ir::LlvmModule,
    result: &str,
    value: O,
    from: &T,
    to: &T,
    ext: CastOp,
) {
    let inst = if from == to {
        I::binary(BinaryOp::Add, from.clone(), O::int(0), value)
    } else if llvm_int_width(from) < llvm_int_width(to) {
        I::cast(ext, from.clone(), value, to.clone())
    } else {
        I::cast(CastOp::Trunc, from.clone(), value, to.clone())
    };
    text.assign(result, inst);
}

fn llvm_int_width(llvm_ty: &T) -> u8 {
    match llvm_ty {
        T::I8 => 8,
        T::I16 => 16,
        T::I32 => 32,
        T::I64 => 64,
        T::I128 => 128,
        _ => unreachable!("integer LLVM type"),
    }
}

fn bit_width(ty: &Type) -> u32 {
    match integer_kind(ty) {
        IntegerType::Byte | IntegerType::Int8 => 8,
        IntegerType::Int16 | IntegerType::UInt16 => 16,
        IntegerType::Int32 | IntegerType::UInt32 => 32,
        IntegerType::Int64 | IntegerType::UInt64 => 64,
    }
}

fn extend_op(ty: &Type) -> CastOp {
    if is_unsigned(ty) {
        CastOp::ZExt
    } else {
        CastOp::SExt
    }
}
