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

use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{
    integer_pub as integer, name_not_found, number_as_float_pub as number_as_float,
    numeric_overflow, parse_val_pub as parse_val, require_arity_pub as require_arity,
    runtime_error_pub as runtime_error, type_mismatch,
};
use crate::types::{FloatType, IntegerType};

#[path = "math_reduce.rs"]
mod reduce;
use reduce::reduce_vector;

pub const NAME: &str = "BNMath";

#[derive(Default)]
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
                        crate::diagnostic::DiagId::FORMAT_OUT_OF_RANGE,
                        "TIMESTAMP is outside 0001-01-01..9999-12-31",
                        span,
                    )
                })?;
                if math_name == "TODATE" {
                    Ok(Value::Date(crate::temporal::date_from_timestamp(
                        timestamp, span,
                    )?))
                } else {
                    Ok(Value::Time(crate::temporal::time_from_timestamp(
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
                    i128::from(crate::temporal::timestamp_from_date_time(
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
        "ABS" => bn_rt::bn_rt_math_fabs(numbers[0]),
        "MIN" => bn_rt::bn_rt_math_fmin(numbers[0], numbers[1]),
        "MAX" => bn_rt::bn_rt_math_fmax(numbers[0], numbers[1]),
        "SIGN" => bn_rt::bn_rt_math_fsign(numbers[0]),
        "FLOOR" => bn_rt::bn_rt_math_floor(numbers[0]),
        "CEIL" => bn_rt::bn_rt_math_ceil(numbers[0]),
        "TRUNC" => bn_rt::bn_rt_math_trunc(numbers[0]),
        "ROUND" => bn_rt::bn_rt_math_round(numbers[0], numbers[1]),
        "EXP" => bn_rt::bn_rt_math_exp(numbers[0]),
        "LOG" => bn_rt::bn_rt_math_log(numbers[0]),
        "LOG10" => bn_rt::bn_rt_math_log10(numbers[0]),
        "LOG2" => bn_rt::bn_rt_math_log2(numbers[0]),
        "POW" => bn_rt::bn_rt_math_pow(numbers[0], numbers[1]),
        "SIN" => bn_rt::bn_rt_math_sin(numbers[0]),
        "COS" => bn_rt::bn_rt_math_cos(numbers[0]),
        "TAN" => bn_rt::bn_rt_math_tan(numbers[0]),
        "ASIN" => bn_rt::bn_rt_math_asin(numbers[0]),
        "ACOS" => bn_rt::bn_rt_math_acos(numbers[0]),
        "ATAN" => bn_rt::bn_rt_math_atan(numbers[0]),
        "ATAN2" => bn_rt::bn_rt_math_atan2(numbers[0], numbers[1]),
        "SQRT" => bn_rt::bn_rt_math_sqrt(numbers[0]),
        "HYPOT" => bn_rt::bn_rt_math_hypot(numbers[0], numbers[1]),
        "FMA" => bn_rt::bn_rt_math_fma(numbers[0], numbers[1], numbers[2]),
        _ => {
            return Err(name_not_found(math_name, "BNMath function", span));
        }
    };
    Ok(Value::Float(result, FloatType::Float64))
}
