// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNMath` — an external library module (`IMPORT BNMath`), served through
//! the provider seam. Nothing here is language: the language globals
//! (`ASC`, `CHAR`, `TOLOWER`, `TOUPPER`) stay in the interpreter core.

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    integer_pub as integer, name_not_found, number_as_float_pub as number_as_float,
    numeric_overflow, parse_val_pub as parse_val, require_arity_pub as require_arity,
    runtime_error_pub as runtime_error, type_mismatch,
};
use bn_types::{FloatType, IntegerType};

mod reduce;
use reduce::reduce_vector;

pub const NAME: &str = "BNMath";

#[derive(Debug, Default)]
pub struct MathProvider;

impl Provider for MathProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let math_name = member.rsplit('.').next().unwrap_or(member);
        match math_name {
            "TODATE" | "TOTIME" => {
                require_arity(math_name, &arguments, 1, span)?;
                let (timestamp, _) = integer(&arguments[0], span)?;
                let timestamp = i64::try_from(timestamp).map_err(|_| {
                    runtime_error(
                        bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                        "TIMESTAMP is outside 0001-01-01..9999-12-31",
                        span,
                    )
                })?;
                if math_name == "TODATE" {
                    Ok(Value::Date(bn_interp::temporal::date_from_timestamp(
                        timestamp, span,
                    )?))
                } else {
                    Ok(Value::Time(bn_interp::temporal::time_from_timestamp(
                        timestamp, span,
                    )?))
                }
            }
            "TOTIMESTAMP" => {
                require_arity(math_name, &arguments, 2, span)?;
                let Value::Date(days) = arguments[0] else {
                    return Err(type_mismatch(
                        "DATE",
                        "non-DATE value",
                        "BNMath.TOTIMESTAMP date",
                        span,
                    ));
                };
                let Value::Time(millis) = arguments[1] else {
                    return Err(type_mismatch(
                        "TIME",
                        "non-TIME value",
                        "BNMath.TOTIMESTAMP time",
                        span,
                    ));
                };
                Ok(Value::Integer(
                    i128::from(bn_interp::temporal::timestamp_from_date_time(
                        days, millis, span,
                    )?),
                    IntegerType::Int64,
                ))
            }
            _ => math(math_name, &arguments, span, core),
        }
    }
}

#[allow(clippy::too_many_lines)] // One arm per BNMath function, as in the library contract.
fn math(
    math_name: &str,
    arguments: &[Value],
    span: Span,
    core: &dyn CoreContext,
) -> Result<Value, Diagnostic> {
    let memory = core.memory();
    if matches!(math_name, "TOHOUR" | "TOWEEKDAY") {
        let milliseconds = integer(&arguments[0], span)?.0;
        let days = milliseconds.div_euclid(86_400_000);
        let result = if math_name == "TOHOUR" {
            milliseconds.div_euclid(3_600_000).rem_euclid(24)
        } else {
            // 1970-01-01 was Thursday (ISO weekday 4).
            (days + 3).rem_euclid(7) + 1
        };
        return Ok(Value::Integer(result, IntegerType::Int32));
    }
    if math_name == "VAL" {
        let Value::String(text) = &arguments[0] else {
            return Err(type_mismatch(
                "STRING",
                "non-STRING value",
                "BNMath.VAL",
                span,
            ));
        };
        return Ok(Value::Float(parse_val(text), FloatType::Float64));
    }
    if matches!(
        math_name,
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "MODE" | "STDEV" | "VARIANCE" | "RANGE"
    ) || (matches!(math_name, "MIN" | "MAX") && arguments.len() == 1)
    {
        return reduce_vector(math_name, &arguments[0], span, memory);
    }
    if matches!(math_name, "ABS" | "MIN" | "MAX" | "SIGN")
        && arguments
            .iter()
            .all(|argument| matches!(argument, Value::Integer(_, _)))
    {
        let integers = arguments
            .iter()
            .map(|argument| integer(argument, span).map(|(value, _)| value))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = integer(&arguments[0], span)?.1;
        let result = match math_name {
            "ABS" => integers[0]
                .checked_abs()
                .ok_or_else(|| numeric_overflow("evaluating BNMath.ABS", span))?,
            "MIN" => integers[0].min(integers[1]),
            "MAX" => integers[0].max(integers[1]),
            "SIGN" => integers[0].signum(),
            _ => unreachable!(),
        };
        return Ok(Value::Integer(result, kind));
    }
    let numbers = arguments
        .iter()
        .map(|value| number_as_float(value, span))
        .collect::<Result<Vec<_>, _>>()?;
    let result = match math_name {
        "ABS" => numbers[0].abs(),
        "MIN" => bn_core_math::fmin(numbers[0], numbers[1]),
        "MAX" => bn_core_math::fmax(numbers[0], numbers[1]),
        "SIGN" => bn_core_math::fsign(numbers[0]),
        "FLOOR" => numbers[0].floor(),
        "CEIL" => numbers[0].ceil(),
        "TRUNC" => numbers[0].trunc(),
        "ROUND" => bn_core_math::round_ties_even(numbers[0], numbers[1]),
        "EXP" => numbers[0].exp(),
        "LOG" => numbers[0].ln(),
        "LOG10" => numbers[0].log10(),
        "LOG2" => numbers[0].log2(),
        "POW" => numbers[0].powf(numbers[1]),
        "SIN" => numbers[0].sin(),
        "COS" => numbers[0].cos(),
        "TAN" => numbers[0].tan(),
        "ASIN" => numbers[0].asin(),
        "ACOS" => numbers[0].acos(),
        "ATAN" => numbers[0].atan(),
        "ATAN2" => numbers[0].atan2(numbers[1]),
        "SQRT" => numbers[0].sqrt(),
        "HYPOT" => numbers[0].hypot(numbers[1]),
        "FMA" => numbers[0].mul_add(numbers[1], numbers[2]),
        _ => {
            return Err(name_not_found(math_name, "BNMath function", span));
        }
    };
    Ok(Value::Float(result, FloatType::Float64))
}
