// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// BNMath lowering: the `bn_rt` math declarations, support checks, and scalar,
// integer and vector math calls through the typed `bn_rt` signatures.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{self, Double, I1, I32, I64, I128},
    RuntimeFn,
};
use runtime_abi::*;

pub(crate) const BN_RT_MATH_DECLS: &str = "\
declare i64 @bn_rt_math_iabs(i64)
declare i64 @bn_rt_math_isign(i64)
declare i64 @bn_rt_math_imin(i64, i64)
declare i64 @bn_rt_math_imax(i64, i64)
declare i32 @bn_rt_math_tohour(i64)
declare i32 @bn_rt_math_toweekday(i64)
declare double @bn_rt_math_val(ptr)
declare double @bn_rt_math_fabs(double)
declare double @bn_rt_math_fsign(double)
declare double @bn_rt_math_floor(double)
declare double @bn_rt_math_ceil(double)
declare double @bn_rt_math_trunc(double)
declare double @bn_rt_math_exp(double)
declare double @bn_rt_math_log(double)
declare double @bn_rt_math_log10(double)
declare double @bn_rt_math_log2(double)
declare double @bn_rt_math_sin(double)
declare double @bn_rt_math_cos(double)
declare double @bn_rt_math_tan(double)
declare double @bn_rt_math_asin(double)
declare double @bn_rt_math_acos(double)
declare double @bn_rt_math_atan(double)
declare double @bn_rt_math_sqrt(double)
declare double @bn_rt_math_pow(double, double)
declare double @bn_rt_math_atan2(double, double)
declare double @bn_rt_math_hypot(double, double)
declare double @bn_rt_math_fmin(double, double)
declare double @bn_rt_math_fmax(double, double)
declare double @bn_rt_math_round(double, double)
declare double @bn_rt_math_fma(double, double, double)
declare i32 @bn_rt_math_vmin_i32(ptr, i32, ptr)
declare i32 @bn_rt_math_vmax_i32(ptr, i32, ptr)
declare double @bn_rt_math_vmin_f64(ptr, i32, ptr)
declare double @bn_rt_math_vmax_f64(ptr, i32, ptr)
declare double @bn_rt_math_mean_i32(ptr, i32)
declare double @bn_rt_math_median_i32(ptr, i32)
declare double @bn_rt_math_quartile1_i32(ptr, i32)
declare double @bn_rt_math_quartile3_i32(ptr, i32)
declare double @bn_rt_math_range_i32(ptr, i32)
declare double @bn_rt_math_stdev_i32(ptr, i32)
declare double @bn_rt_math_variance_i32(ptr, i32)
declare i32 @bn_rt_math_mode_i32(ptr, i32, ptr)
declare double @bn_rt_math_mean_f64(ptr, i32)
declare double @bn_rt_math_median_f64(ptr, i32)
declare double @bn_rt_math_quartile1_f64(ptr, i32)
declare double @bn_rt_math_quartile3_f64(ptr, i32)
declare double @bn_rt_math_range_f64(ptr, i32)
declare double @bn_rt_math_stdev_f64(ptr, i32)
declare double @bn_rt_math_variance_f64(ptr, i32)
declare i32 @bn_rt_math_mode_f64(ptr, i32, ptr)
declare i32 @bn_rt_math_todate(i64, ptr)
declare i32 @bn_rt_math_totime(i64, ptr)
declare i64 @bn_rt_math_totimestamp(i32, i32)
";

pub(crate) fn bnmath_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    let method = name.rsplit('.').next()?;
    (!module.bnmath_providers.is_empty()
        && matches!(
            method,
            "VAL"
                | "TOHOUR"
                | "TOWEEKDAY"
                | "TODATE"
                | "TOTIME"
                | "TOTIMESTAMP"
                | "ABS"
                | "SIGN"
                | "FLOOR"
                | "CEIL"
                | "TRUNC"
                | "EXP"
                | "LOG"
                | "LOG10"
                | "LOG2"
                | "SIN"
                | "COS"
                | "TAN"
                | "ASIN"
                | "ACOS"
                | "ATAN"
                | "SQRT"
                | "MIN"
                | "MAX"
                | "MEAN"
                | "MEDIAN"
                | "QUARTILE1"
                | "QUARTILE3"
                | "RANGE"
                | "STDEV"
                | "VARIANCE"
                | "MODE"
                | "POW"
                | "ATAN2"
                | "HYPOT"
                | "ROUND"
                | "FMA"
        ))
    .then_some(method)
}

pub(crate) fn bnmath_call_supported(
    method: &str,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> bool {
    let args = arguments
        .iter()
        .filter_map(|argument| values.get(argument))
        .collect::<Vec<_>>();
    if args.len() != arguments.len() {
        return false;
    }
    match method {
        "VAL" => arguments.len() == 1 && args[0] == &Type::String,
        "TOHOUR" | "TOWEEKDAY" | "TODATE" | "TOTIME" => {
            arguments.len() == 1 && integer_arg(args[0])
        }
        "TOTIMESTAMP" => arguments.len() == 2 && integer_arg(args[0]) && integer_arg(args[1]),
        "ABS" | "SIGN" | "FLOOR" | "CEIL" | "TRUNC" | "EXP" | "LOG" | "LOG10" | "LOG2" | "SIN"
        | "COS" | "TAN" | "ASIN" | "ACOS" | "ATAN" | "SQRT" => {
            arguments.len() == 1 && numeric_arg(args[0])
        }
        "MIN" | "MAX" if arguments.len() == 1 => is_supported_numeric_vector(args[0]),
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV" | "VARIANCE" | "MODE"
            if arguments.len() == 1 =>
        {
            is_supported_numeric_vector(args[0])
        }
        "MIN" | "MAX" | "POW" | "ATAN2" | "HYPOT" | "ROUND" => {
            arguments.len() == 2 && numeric_arg(args[0]) && numeric_arg(args[1])
        }
        "FMA" => arguments.len() == 3 && args.iter().all(|ty| numeric_arg(ty)),
        _ => false,
    }
}

fn integer_arg(ty: &Type) -> bool {
    llvm_type(ty).is_some_and(integer_llvm)
}

fn is_f64_vector(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Vector { element, dimensions }
            if dimensions.len() == 1
                && matches!(element.as_ref(), Type::Float(FloatType::Float64))
    )
}

fn is_supported_numeric_vector(ty: &Type) -> bool {
    is_int_vector(ty)
        || is_f64_vector(ty)
        || matches!(ty, Type::Pointer { element, .. } if matches!(element.as_ref(), Type::Integer(_) | Type::Float(_)))
}

fn numeric_arg(ty: &Type) -> bool {
    llvm_type(ty).is_some_and(|llvm| integer_llvm(llvm) || float_llvm(llvm))
}

pub(crate) fn lower_bnmath_call(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let types = arguments
        .iter()
        .map(|argument| {
            analysis
                .values
                .get(argument)
                .expect("validated BNMath argument")
        })
        .collect::<Vec<_>>();
    let integer_op = types.iter().all(|ty| integer_arg(ty));
    let dest = destination.0;
    match method {
        "VAL" => text.assign(format!("v{dest}"), MATH_VAL.call([value_reg(arguments[0])])),
        "TOHOUR" | "TOWEEKDAY" | "TODATE" | "TOTIME" => {
            let value = LlvmOperand::raw(extend_to_i64(text, arguments[0], types[0]));
            let call = match method {
                "TOHOUR" => MATH_TOHOUR.call([value]),
                "TOWEEKDAY" => MATH_TOWEEKDAY.call([value]),
                // Outside 0001..9999 `bn_rt` prints this site's diagnostic.
                other => {
                    let trap = LlvmOperand::global(
                        trap_symbol(
                            state,
                            bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                            vec![(
                                "message",
                                Fact::Text(bn_core_text::civil::RANGE_ERROR.into()),
                            )],
                        )
                        .0,
                    );
                    if other == "TODATE" {
                        MATH_TODATE.call([value, trap])
                    } else {
                        MATH_TOTIME.call([value, trap])
                    }
                }
            };
            text.assign(format!("v{dest}"), call);
        }
        "TOTIMESTAMP" => text.assign(
            format!("v{dest}"),
            MATH_TOTIMESTAMP.call([value_reg(arguments[0]), value_reg(arguments[1])]),
        ),
        "MIN" | "MAX" | "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV"
        | "VARIANCE" | "MODE"
            if arguments.len() == 1
                && types
                    .first()
                    .is_some_and(|ty| is_supported_numeric_vector(ty)) =>
        {
            lower_vector_math(text, destination, method, arguments[0], types[0], state);
        }
        "ABS" | "SIGN" | "MIN" | "MAX" if integer_op => {
            let result_ty = analysis
                .values
                .get(&destination)
                .expect("validated BNMath result type");
            lower_integer_math(
                text,
                block_id,
                destination,
                method,
                arguments,
                types.as_slice(),
                result_ty,
                state,
            );
        }
        _ => {
            let result_ty = analysis
                .values
                .get(&destination)
                .expect("validated BNMath result type");
            lower_float_math(
                text,
                destination,
                method,
                arguments,
                types.as_slice(),
                result_ty,
            );
        }
    }
}

fn lower_vector_math(
    text: &mut String,
    destination: ValueId,
    method: &str,
    vector: ValueId,
    vector_ty: &Type,
    state: &EmissionState,
) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let float_vector = match vector_ty {
        Type::Pointer { element, .. } => {
            matches!(element.as_ref(), Type::Float(FloatType::Float64))
        }
        _ => is_f64_vector(vector_ty),
    };
    let pair = crate::layout::vector_ty();
    text.assign(
        format!("statptr{dest}"),
        LlvmInst::extract(pair.clone(), value_reg(vector), 0),
    );
    text.assign(
        format!("statlen{dest}"),
        LlvmInst::extract(pair, value_reg(vector), 1),
    );
    let data = reg(format!("statptr{dest}"));
    let length = reg(format!("statlen{dest}"));
    match method {
        "MIN" | "MAX" => {
            // MIN and MAX of an empty vector: `bn_rt` prints this site's diagnostic.
            let empty = LlvmOperand::global(
                trap_symbol(
                    state,
                    bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
                    vec![
                        ("index", Fact::Text("0".into())),
                        ("bound", Fact::Text("0".into())),
                        (
                            "context",
                            Fact::Text(format!("the input of BNMath.{method}")),
                        ),
                    ],
                )
                .0,
            );
            let function = match (method, float_vector) {
                ("MIN", true) => &MATH_VMIN_F64,
                ("MAX", true) => &MATH_VMAX_F64,
                ("MIN", false) => &MATH_VMIN_I32,
                _ => &MATH_VMAX_I32,
            };
            text.assign(format!("v{dest}"), function.call([data, length, empty]));
        }
        "MODE" => {
            let out = reg(format!("modeout{dest}"));
            text.assign(format!("modeout{dest}"), LlvmInst::alloca(Double));
            let function = if float_vector {
                &MATH_MODE_F64
            } else {
                &MATH_MODE_I32
            };
            text.assign(
                format!("modena{dest}"),
                function.call([data, length, out.clone()]),
            );
            text.assign(
                format!("modeis{dest}"),
                LlvmInst::icmp(
                    ICmpCond::Ne,
                    I32,
                    reg(format!("modena{dest}")),
                    LlvmOperand::int(0),
                ),
            );
            text.assign(format!("modeval{dest}"), LlvmInst::load(Double, out));
            text.assign(
                format!("modebits{dest}"),
                LlvmInst::cast(CastOp::BitCast, Double, reg(format!("modeval{dest}")), I64),
            );
            general_alternative::from_flag(
                text,
                &format!("v{dest}"),
                reg(format!("modeis{dest}")),
                &Type::NotAvailable,
                &Type::Float(FloatType::Float64),
                reg(format!("modebits{dest}")),
            );
        }
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV" | "VARIANCE" => {
            let function: &RuntimeFn<2> = match (method, float_vector) {
                ("MEAN", false) => &MATH_MEAN_I32,
                ("MEDIAN", false) => &MATH_MEDIAN_I32,
                ("QUARTILE1", false) => &MATH_QUARTILE1_I32,
                ("QUARTILE3", false) => &MATH_QUARTILE3_I32,
                ("RANGE", false) => &MATH_RANGE_I32,
                ("STDEV", false) => &MATH_STDEV_I32,
                ("MEAN", true) => &MATH_MEAN_F64,
                ("MEDIAN", true) => &MATH_MEDIAN_F64,
                ("QUARTILE1", true) => &MATH_QUARTILE1_F64,
                ("QUARTILE3", true) => &MATH_QUARTILE3_F64,
                ("RANGE", true) => &MATH_RANGE_F64,
                ("STDEV", true) => &MATH_STDEV_F64,
                (_, false) => &MATH_VARIANCE_I32,
                (_, true) => &MATH_VARIANCE_F64,
            };
            text.assign(format!("v{dest}"), function.call([data, length]));
        }
        _ => unreachable!("validated vector BNMath"),
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_integer_math(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    types: &[&Type],
    result_type: &Type,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let left = LlvmOperand::raw(extend_to_i64(text, arguments[0], types[0]));
    let result_ty = crate::layout::typed_llvm(
        llvm_type(result_type).expect("validated integer math result type"),
    );
    if method == "ABS" {
        // |x| of the most negative value does not fit its type; the exact
        // magnitude is reported as the interpreter does (i128 holds it).
        let (low, high) = i128_bounds(result_type);
        let wide = reg(format!("absw{dest}"));
        let exact = reg(format!("absexact{dest}"));
        text.assign(
            format!("absw{dest}"),
            LlvmInst::cast(CastOp::SExt, I64, left, I128),
        );
        text.assign(
            format!("absneg{dest}"),
            LlvmInst::icmp(ICmpCond::Slt, I128, wide.clone(), LlvmOperand::int(0)),
        );
        text.assign(
            format!("absflip{dest}"),
            LlvmInst::binary(BinaryOp::Sub, I128, LlvmOperand::int(0), wide.clone()),
        );
        text.assign(
            format!("absexact{dest}"),
            LlvmInst::select(
                reg(format!("absneg{dest}")),
                I128,
                reg(format!("absflip{dest}")),
                wide,
            ),
        );
        text.assign(
            format!("abslo{dest}"),
            LlvmInst::icmp(ICmpCond::Slt, I128, exact.clone(), LlvmOperand::raw(low)),
        );
        text.assign(
            format!("abshi{dest}"),
            LlvmInst::icmp(ICmpCond::Sgt, I128, exact.clone(), LlvmOperand::raw(high)),
        );
        text.assign(
            format!("absbad{dest}"),
            LlvmInst::binary(
                BinaryOp::Or,
                I1,
                reg(format!("abslo{dest}")),
                reg(format!("abshi{dest}")),
            ),
        );
        let ok = take_continuation(block_id, state);
        emit_overflow_trap(
            text,
            block_id,
            state,
            &format!("%absbad{dest}"),
            ok,
            &format!("%absexact{dest}"),
            result_type,
        );
        text.assign(
            format!("v{dest}"),
            LlvmInst::cast(CastOp::Trunc, I128, exact, result_ty),
        );
        return;
    }
    let call = match method {
        "SIGN" => MATH_ISIGN.call([left]),
        "MIN" | "MAX" => {
            let right = LlvmOperand::raw(extend_to_i64(text, arguments[1], types[1]));
            if method == "MIN" {
                MATH_IMIN.call([left, right])
            } else {
                MATH_IMAX.call([left, right])
            }
        }
        _ => unreachable!("validated integer BNMath"),
    };
    if result_ty == I64 {
        text.assign(format!("v{dest}"), call);
    } else {
        text.assign(format!("mathi64{dest}"), call);
        text.assign(
            format!("v{dest}"),
            LlvmInst::cast(CastOp::Trunc, I64, reg(format!("mathi64{dest}")), result_ty),
        );
    }
}

fn lower_float_math(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    types: &[&Type],
    result_type: &Type,
) {
    let args = arguments
        .iter()
        .zip(types)
        .map(|(argument, ty)| LlvmOperand::raw(extend_to_double(text, *argument, ty)))
        .collect::<Vec<_>>();
    let unary = |method: &str| -> &'static RuntimeFn<1> {
        match method {
            "ABS" => &MATH_FABS,
            "SIGN" => &MATH_FSIGN,
            "FLOOR" => &MATH_FLOOR,
            "CEIL" => &MATH_CEIL,
            "TRUNC" => &MATH_TRUNC,
            "EXP" => &MATH_EXP,
            "LOG" => &MATH_LOG,
            "LOG10" => &MATH_LOG10,
            "LOG2" => &MATH_LOG2,
            "SIN" => &MATH_SIN,
            "COS" => &MATH_COS,
            "TAN" => &MATH_TAN,
            "ASIN" => &MATH_ASIN,
            "ACOS" => &MATH_ACOS,
            "ATAN" => &MATH_ATAN,
            "SQRT" => &MATH_SQRT,
            _ => unreachable!("validated unary float BNMath"),
        }
    };
    let binary = |method: &str| -> &'static RuntimeFn<2> {
        match method {
            "POW" => &MATH_POW,
            "ATAN2" => &MATH_ATAN2,
            "HYPOT" => &MATH_HYPOT,
            "MIN" => &MATH_FMIN,
            "MAX" => &MATH_FMAX,
            "ROUND" => &MATH_ROUND,
            _ => unreachable!("validated binary float BNMath"),
        }
    };
    let call = match <[LlvmOperand; 3]>::try_from(args) {
        Ok(three) if method == "FMA" => MATH_FMA.call(three),
        Ok(_) => unreachable!("validated float BNMath arity"),
        Err(args) => match <[LlvmOperand; 2]>::try_from(args) {
            Ok(two) => binary(method).call(two),
            Err(args) => unary(method)
                .call(<[LlvmOperand; 1]>::try_from(args).expect("validated float BNMath arity")),
        },
    };
    // `bn_rt` computes in `double`; a FLOAT32 result narrows it back.
    if llvm_type(result_type) == Some("float") {
        let dest = destination.0;
        text.assign(format!("mathres{dest}"), call);
        text.assign(
            format!("v{dest}"),
            LlvmInst::cast(
                CastOp::FPTrunc,
                Double,
                LlvmOperand::reg(format!("mathres{dest}")),
                LlvmType::Float,
            ),
        );
    } else {
        text.assign(format!("v{}", destination.0), call);
    }
}

fn extend_to_double(text: &mut String, value: ValueId, ty: &Type) -> String {
    let llvm_ty = llvm_type(ty).expect("validated numeric type");
    if llvm_ty == "double" {
        return format!("%v{}", value.0);
    }
    let op = match llvm_ty {
        "float" => CastOp::FPExt,
        _ if is_unsigned(ty) => CastOp::UIToFP,
        _ => CastOp::SIToFP,
    };
    let temp = format!("mathf64{}", value.0);
    text.assign(
        temp.clone(),
        LlvmInst::cast(
            op,
            crate::layout::typed_llvm(llvm_ty),
            value_reg(value),
            Double,
        ),
    );
    format!("%{temp}")
}
