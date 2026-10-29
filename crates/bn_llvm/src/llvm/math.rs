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
use bn_types::numeric_abi::{NumericElement, statistic_code};
use runtime_abi::*;

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
                | "DegreeToRad"
                | "RadToDegree"
                | "Gcd"
                | "Lcm"
                | "MIN"
                | "MAX"
                | "MEAN"
                | "MEDIAN"
                | "QUARTILE1"
                | "QUARTILE3"
                | "RANGE"
                | "STDEV"
                | "VARIANCE"
                | "PVariance"
                | "PStdev"
                | "Skewness"
                | "Kurtosis"
                | "PSkewness"
                | "PKurtosis"
                | "Quantile"
                | "Covariance"
                | "Correlation"
                | "Dot"
                | "Sum"
                | "Product"
                | "StudentT"
                | "StudentTPvalue"
                | "StudentTQuantile"
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
        | "COS" | "TAN" | "ASIN" | "ACOS" | "ATAN" | "SQRT" | "DegreeToRad" | "RadToDegree" => {
            arguments.len() == 1 && numeric_arg(args[0])
        }
        "MIN" | "MAX" if arguments.len() == 1 => is_supported_numeric_vector(args[0]),
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV" | "VARIANCE" | "MODE"
        | "PVariance" | "PStdev" | "Skewness" | "Kurtosis" | "PSkewness" | "PKurtosis"
            if arguments.len() == 1 =>
        {
            is_supported_numeric_vector(args[0])
        }
        "MIN" | "MAX" | "POW" | "ATAN2" | "HYPOT" | "ROUND" => {
            arguments.len() == 2 && numeric_arg(args[0]) && numeric_arg(args[1])
        }
        "FMA" => arguments.len() == 3 && args.iter().all(|ty| numeric_arg(ty)),
        "Gcd" | "Lcm" => arguments.len() == 2 && integer_arg(args[0]) && integer_arg(args[1]),
        "Quantile" => {
            arguments.len() == 2 && is_supported_numeric_vector(args[0]) && numeric_arg(args[1])
        }
        "Covariance" | "Correlation" | "Dot" => {
            arguments.len() == 2 && args.iter().all(|ty| is_supported_numeric_vector(ty))
        }
        "Sum" | "Product" if arguments.len() == 1 => is_supported_numeric_vector(args[0]),
        "StudentT" | "StudentTPvalue" | "StudentTQuantile" => {
            arguments.len() == 2 && numeric_arg(args[0]) && numeric_arg(args[1])
        }
        _ => false,
    }
}

fn integer_arg(ty: &Type) -> bool {
    llvm_type(ty).is_some_and(|ty| ty.is_integer())
}

/// The element type of a one-dimensional numeric vector or of a numeric
/// region, the operands the descriptive statistics accept (math.md).
fn numeric_vector_element(ty: &Type) -> Option<NumericElement> {
    match ty {
        Type::Vector {
            element,
            dimensions,
        } if dimensions.len() == 1 => match element.as_ref() {
            Type::IntegerLiteral(_) => Some(NumericElement::Integer(IntegerType::Int32)),
            other => NumericElement::of(other),
        },
        Type::Pointer { element, .. } => NumericElement::of(element),
        _ => None,
    }
}

fn is_supported_numeric_vector(ty: &Type) -> bool {
    numeric_vector_element(ty).is_some()
}

fn numeric_arg(ty: &Type) -> bool {
    llvm_type(ty).is_some_and(|llvm| llvm.is_integer() || llvm.is_floating())
}

pub(crate) fn lower_bnmath_call(
    text: &mut crate::ir::LlvmModule,
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
            let value = extend_to_i64(text, arguments[0], types[0]);
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
        | "VARIANCE" | "MODE" | "PVariance" | "PStdev" | "Skewness" | "Kurtosis" | "PSkewness"
        | "PKurtosis"
            if arguments.len() == 1
                && types
                    .first()
                    .is_some_and(|ty| is_supported_numeric_vector(ty)) =>
        {
            lower_vector_math(text, destination, method, arguments[0], types[0], state);
        }
        "Sum" | "Product" => {
            lower_vector_math(text, destination, method, arguments[0], types[0], state);
        }
        "Quantile" => lower_quantile(text, destination, arguments, types.as_slice()),
        "Dot" | "Covariance" | "Correlation" => {
            lower_association(
                text,
                destination,
                method,
                arguments,
                types.as_slice(),
                state,
            );
        }
        "StudentT" | "StudentTPvalue" | "StudentTQuantile" => {
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
        "Gcd" | "Lcm" => {
            let result_ty = analysis
                .values
                .get(&destination)
                .expect("validated BNMath result type");
            lower_integer_divisor(
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

/// `BNMath.Quantile(values, p)` (math.md, R type 7): the vector's data and
/// length with its element code, and `p` widened to `FLOAT`, go to `bn_rt`.
fn lower_quantile(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    arguments: &[ValueId],
    types: &[&Type],
) {
    let dest = destination.0;
    let element = numeric_vector_element(types[0]).expect("validated numeric vector");
    let pair = crate::layout::vector_ty();
    text.assign(
        format!("qptr{dest}"),
        LlvmInst::extract(pair.clone(), value_reg(arguments[0]), 0),
    );
    text.assign(
        format!("qlen{dest}"),
        LlvmInst::extract(pair, value_reg(arguments[0]), 1),
    );
    let probability = extend_to_double(text, arguments[1], types[1]);
    text.assign(
        format!("v{dest}"),
        MATH_VECTOR_QUANTILE.call([
            LlvmOperand::reg(format!("qptr{dest}")),
            LlvmOperand::reg(format!("qlen{dest}")),
            LlvmOperand::int(i64::from(element.code())),
            probability,
        ]),
    );
}

/// `BNMath.Covariance(x, y)` / `Correlation(x, y)` (math.md): each vector's
/// data, length and element code go to `bn_rt`, with this site's
/// `VECTOR_LENGTH_MISMATCH` for vectors of different lengths.
fn lower_association(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    types: &[&Type],
    state: &EmissionState,
) {
    let dest = destination.0;
    let pair = crate::layout::vector_ty();
    let mut operands = Vec::new();
    for (side, (argument, ty)) in arguments.iter().zip(types).enumerate() {
        let element = numeric_vector_element(ty).expect("validated numeric vector");
        text.assign(
            format!("assocptr{dest}_{side}"),
            LlvmInst::extract(pair.clone(), value_reg(*argument), 0),
        );
        text.assign(
            format!("assoclen{dest}_{side}"),
            LlvmInst::extract(pair.clone(), value_reg(*argument), 1),
        );
        operands.extend([
            LlvmOperand::reg(format!("assocptr{dest}_{side}")),
            LlvmOperand::reg(format!("assoclen{dest}_{side}")),
            LlvmOperand::int(i64::from(element.code())),
        ]);
    }
    let mismatch = LlvmOperand::global(
        trap_symbol(
            state,
            bn_diag::DiagId::VECTOR_LENGTH_MISMATCH,
            vec![(
                "message",
                Fact::Text(bn_diag::trap_texts::STATISTIC_VECTORS_DIFFER.into()),
            )],
        )
        .0,
    );
    let [x_ptr, x_len, x_element, y_ptr, y_len, y_element]: [LlvmOperand; 6] = operands
        .try_into()
        .expect("two vectors, three operands each");
    let operands = [x_ptr, x_len, x_element, y_ptr, y_len, y_element, mismatch];
    let call = match method {
        "Covariance" => MATH_VECTOR_COVARIANCE.call(operands),
        "Dot" => MATH_VECTOR_DOT.call(operands),
        _ => MATH_VECTOR_CORRELATION.call(operands),
    };
    text.assign(format!("v{dest}"), call);
}

fn lower_vector_math(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    method: &str,
    vector: ValueId,
    vector_ty: &Type,
    state: &EmissionState,
) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let element = numeric_vector_element(vector_ty).expect("validated numeric vector");
    // INTEGER and FLOAT keep their dedicated entry points; every other
    // element type goes through the generic ones with its element code.
    let generic = !matches!(
        element,
        NumericElement::Integer(IntegerType::Int32) | NumericElement::Float(FloatType::Float64)
    );
    let float_vector = matches!(element, NumericElement::Float(_));
    let element_code = LlvmOperand::int(i64::from(element.code()));
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
            if generic {
                lower_generic_extreme(
                    text,
                    dest,
                    method,
                    element,
                    [data, length, element_code, empty],
                );
                return;
            }
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
            let flag = if generic {
                MATH_VECTOR_MODE.call([data, length, element_code, out.clone()])
            } else if float_vector {
                MATH_MODE_F64.call([data, length, out.clone()])
            } else {
                MATH_MODE_I32.call([data, length, out.clone()])
            };
            text.assign(format!("modena{dest}"), flag);
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
        "Sum" | "Product" => {
            let function = if method == "Sum" {
                &MATH_VECTOR_SUM
            } else {
                &MATH_VECTOR_PRODUCT
            };
            text.assign(
                format!("v{dest}"),
                function.call([data, length, element_code]),
            );
        }
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV" | "VARIANCE"
        | "PVariance" | "PStdev" | "Skewness" | "Kurtosis" | "PSkewness" | "PKurtosis" => {
            // The statistics added in 0.6.10 have no dedicated entry point.
            let generic_only = matches!(
                method,
                "PVariance" | "PStdev" | "Skewness" | "Kurtosis" | "PSkewness" | "PKurtosis"
            );
            if generic || generic_only {
                let statistic = LlvmOperand::int(i64::from(
                    statistic_code(method).expect("a BNMath statistic name"),
                ));
                text.assign(
                    format!("v{dest}"),
                    MATH_VECTOR_STATISTIC.call([data, length, element_code, statistic]),
                );
                return;
            }
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

/// One-argument `MIN` / `MAX` of a vector whose element is neither `INTEGER`
/// nor `FLOAT`: the runtime returns the element in 64 bits (`i64` or
/// `double`), narrowed here back to the element type, which the result keeps.
fn lower_generic_extreme(
    text: &mut crate::ir::LlvmModule,
    dest: u32,
    method: &str,
    element: NumericElement,
    arguments: [LlvmOperand; 4],
) {
    let wide = format!("statwide{dest}");
    let (call, wide_type, narrow) = match (method, element) {
        ("MIN", NumericElement::Integer(_)) => {
            (MATH_VECTOR_MIN_INTEGER.call(arguments), I64, CastOp::Trunc)
        }
        (_, NumericElement::Integer(_)) => {
            (MATH_VECTOR_MAX_INTEGER.call(arguments), I64, CastOp::Trunc)
        }
        ("MIN", NumericElement::Float(_)) => (
            MATH_VECTOR_MIN_FLOAT.call(arguments),
            Double,
            CastOp::FPTrunc,
        ),
        (_, NumericElement::Float(_)) => (
            MATH_VECTOR_MAX_FLOAT.call(arguments),
            Double,
            CastOp::FPTrunc,
        ),
    };
    let element_type = match element {
        NumericElement::Integer(kind) => Type::Integer(kind),
        NumericElement::Float(kind) => Type::Float(kind),
    };
    let element_llvm = llvm_type(&element_type).expect("a numeric element has a layout");
    if element_llvm == wide_type {
        text.assign(format!("v{dest}"), call);
        return;
    }
    text.assign(wide.clone(), call);
    text.assign(
        format!("v{dest}"),
        LlvmInst::cast(narrow, wide_type, LlvmOperand::reg(wide), element_llvm),
    );
}

#[allow(clippy::too_many_arguments)]
fn lower_integer_math(
    text: &mut crate::ir::LlvmModule,
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
    let left = extend_to_i64(text, arguments[0], types[0]);
    let result_ty = llvm_type(result_type).expect("validated integer math result type");
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
            LlvmInst::icmp(ICmpCond::Slt, I128, exact.clone(), low),
        );
        text.assign(
            format!("abshi{dest}"),
            LlvmInst::icmp(ICmpCond::Sgt, I128, exact.clone(), high),
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
            &LlvmOperand::reg(format!("absbad{dest}")),
            ok,
            &LlvmOperand::reg(format!("absexact{dest}")),
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
            let right = extend_to_i64(text, arguments[1], types[1]);
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

/// `BNMath.Gcd` / `Lcm` (math.md, "Integer divisors"): `bn_rt` returns the
/// divisor's magnitude (at most `2^63`); the exact result is formed in
/// `i128` (`Lcm` is at most `2^126`), checked against `INT64` with the
/// interpreter's overflow diagnostic, then narrowed.
#[allow(clippy::too_many_arguments)] // the same lowering context as `lower_integer_math`
fn lower_integer_divisor(
    text: &mut crate::ir::LlvmModule,
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
    let left = extend_to_i64(text, arguments[0], types[0]);
    let right = extend_to_i64(text, arguments[1], types[1]);
    text.assign(
        format!("gcdbits{dest}"),
        MATH_GCD.call([left.clone(), right.clone()]),
    );
    text.assign(
        format!("gcd{dest}"),
        LlvmInst::cast(CastOp::ZExt, I64, reg(format!("gcdbits{dest}")), I128),
    );
    let exact = if method == "Gcd" {
        reg(format!("gcd{dest}"))
    } else {
        let left = magnitude_i128(text, &format!("lcml{dest}"), left);
        let right = magnitude_i128(text, &format!("lcmr{dest}"), right);
        // Both arguments zero: the divisor is 0 and so is the result.
        text.assign(
            format!("gcdzero{dest}"),
            LlvmInst::icmp(
                ICmpCond::Eq,
                I128,
                reg(format!("gcd{dest}")),
                LlvmOperand::int(0),
            ),
        );
        text.assign(
            format!("gcdsafe{dest}"),
            LlvmInst::select(
                reg(format!("gcdzero{dest}")),
                I128,
                LlvmOperand::int(1),
                reg(format!("gcd{dest}")),
            ),
        );
        text.assign(
            format!("lcmq{dest}"),
            LlvmInst::binary(BinaryOp::UDiv, I128, left, reg(format!("gcdsafe{dest}"))),
        );
        text.assign(
            format!("lcm{dest}"),
            LlvmInst::binary(BinaryOp::Mul, I128, reg(format!("lcmq{dest}")), right),
        );
        reg(format!("lcm{dest}"))
    };
    let (_, high) = i128_bounds(result_type);
    text.assign(
        format!("divbad{dest}"),
        LlvmInst::icmp(ICmpCond::Sgt, I128, exact.clone(), high),
    );
    let ok = take_continuation(block_id, state);
    emit_overflow_trap(
        text,
        block_id,
        state,
        &reg(format!("divbad{dest}")),
        ok,
        &exact,
        result_type,
    );
    let result_ty = llvm_type(result_type).expect("validated integer divisor result type");
    text.assign(
        format!("v{dest}"),
        LlvmInst::cast(CastOp::Trunc, I128, exact, result_ty),
    );
}

/// `|value|` of an `i64` operand, exact in `i128` (`|MIN_INT64|` is `2^63`).
fn magnitude_i128(text: &mut crate::ir::LlvmModule, name: &str, value: LlvmOperand) -> LlvmOperand {
    let reg = LlvmOperand::reg;
    text.assign(
        format!("{name}w"),
        LlvmInst::cast(CastOp::SExt, I64, value, I128),
    );
    text.assign(
        format!("{name}neg"),
        LlvmInst::icmp(
            ICmpCond::Slt,
            I128,
            reg(format!("{name}w")),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("{name}flip"),
        LlvmInst::binary(
            BinaryOp::Sub,
            I128,
            LlvmOperand::int(0),
            reg(format!("{name}w")),
        ),
    );
    text.assign(
        format!("{name}m"),
        LlvmInst::select(
            reg(format!("{name}neg")),
            I128,
            reg(format!("{name}flip")),
            reg(format!("{name}w")),
        ),
    );
    reg(format!("{name}m"))
}

fn lower_float_math(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    types: &[&Type],
    result_type: &Type,
) {
    let args = arguments
        .iter()
        .zip(types)
        .map(|(argument, ty)| extend_to_double(text, *argument, ty))
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
            "DegreeToRad" => &MATH_DEGREE_TO_RAD,
            "RadToDegree" => &MATH_RAD_TO_DEGREE,
            _ => unreachable!("validated unary float BNMath"),
        }
    };
    let binary = |method: &str| -> &'static RuntimeFn<2> {
        match method {
            "POW" => &MATH_POW,
            "ATAN2" => &MATH_ATAN2,
            "StudentT" => &MATH_STUDENT_T,
            "StudentTPvalue" => &MATH_STUDENT_T_PVALUE,
            "StudentTQuantile" => &MATH_STUDENT_T_QUANTILE,
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
    if llvm_type(result_type) == Some(LlvmType::Float) {
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

fn extend_to_double(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> crate::ir::LlvmOperand {
    let llvm_ty = llvm_type(ty).expect("validated numeric type");
    if llvm_ty == LlvmType::Double {
        return LlvmOperand::reg(format!("v{}", value.0));
    }
    let op = match llvm_ty {
        LlvmType::Float => CastOp::FPExt,
        _ if is_unsigned(ty) => CastOp::UIToFP,
        _ => CastOp::SIToFP,
    };
    let temp = format!("mathf64{}", value.0);
    text.assign(
        temp.clone(),
        LlvmInst::cast(op, llvm_ty, value_reg(value), Double),
    );
    LlvmOperand::reg(&temp)
}
