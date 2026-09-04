#![allow(clippy::wildcard_imports)]
use super::*;

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
declare i32 @bn_rt_math_vmin_i32(ptr, i32)
declare i32 @bn_rt_math_vmax_i32(ptr, i32)
declare double @bn_rt_math_mean_i32(ptr, i32)
declare double @bn_rt_math_median_i32(ptr, i32)
declare double @bn_rt_math_quartile1_i32(ptr, i32)
declare double @bn_rt_math_quartile3_i32(ptr, i32)
declare double @bn_rt_math_range_i32(ptr, i32)
declare double @bn_rt_math_stdev_i32(ptr, i32)
declare double @bn_rt_math_variance_i32(ptr, i32)
declare i32 @bn_rt_math_mode_i32(ptr, i32, ptr)
declare i32 @bn_rt_math_todate(i64)
declare i32 @bn_rt_math_totime(i64)
declare i64 @bn_rt_math_totimestamp(i32, i32)
";

pub(crate) fn bnmath_method<'a>(module: &Module, name: &'a str) -> Option<&'a str> {
    let method = name.rsplit('.').next()?;
    let module_id = name
        .strip_prefix('#')?
        .split('.')
        .next()?
        .parse::<u32>()
        .ok()?;
    module
        .bnmath_providers
        .iter()
        .any(|provider| provider.0 == module_id)
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
        "MIN" | "MAX" if arguments.len() == 1 => is_int_vector(args[0]),
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV" | "VARIANCE" | "MODE"
            if arguments.len() == 1 =>
        {
            is_int_vector(args[0])
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

fn numeric_arg(ty: &Type) -> bool {
    llvm_type(ty).is_some_and(|llvm| integer_llvm(llvm) || float_llvm(llvm))
}

pub(crate) fn lower_bnmath_call(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
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
    match method {
        "VAL" => {
            let _ = writeln!(
                text,
                "  %v{} = call double @bn_rt_math_val(ptr %v{})",
                destination.0, arguments[0].0
            );
        }
        "TOHOUR" | "TOWEEKDAY" | "TODATE" | "TOTIME" => {
            let value = extend_to_i64(text, arguments[0], types[0]);
            let intrinsic = match method {
                "TOHOUR" => "bn_rt_math_tohour",
                "TOWEEKDAY" => "bn_rt_math_toweekday",
                "TODATE" => "bn_rt_math_todate",
                _ => "bn_rt_math_totime",
            };
            let _ = writeln!(
                text,
                "  %v{} = call i32 @{intrinsic}(i64 {value})",
                destination.0
            );
        }
        "TOTIMESTAMP" => {
            let _ = writeln!(
                text,
                "  %v{} = call i64 @bn_rt_math_totimestamp(i32 %v{}, i32 %v{})",
                destination.0, arguments[0].0, arguments[1].0
            );
        }
        "MIN" | "MAX" | "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV"
        | "VARIANCE" | "MODE"
            if types.first().is_some_and(|ty| is_int_vector(ty)) =>
        {
            lower_vector_math(text, destination, method, arguments[0]);
        }
        "ABS" | "SIGN" | "MIN" | "MAX" if integer_op => {
            let result_ty = analysis
                .values
                .get(&destination)
                .expect("validated BNMath result type");
            lower_integer_math(
                text,
                destination,
                method,
                arguments,
                types.as_slice(),
                result_ty,
            );
        }
        _ => lower_float_math(text, destination, method, arguments, types.as_slice()),
    }
}

fn lower_vector_math(text: &mut String, destination: ValueId, method: &str, vector: ValueId) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %statptr{dest} = extractvalue {{ ptr, i32 }} %v{}, 0",
        vector.0
    );
    let _ = writeln!(
        text,
        "  %statlen{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
        vector.0
    );
    match method {
        "MIN" => {
            let _ = writeln!(
                text,
                "  %v{dest} = call i32 @bn_rt_math_vmin_i32(ptr %statptr{dest}, i32 %statlen{dest})"
            );
        }
        "MAX" => {
            let _ = writeln!(
                text,
                "  %v{dest} = call i32 @bn_rt_math_vmax_i32(ptr %statptr{dest}, i32 %statlen{dest})"
            );
        }
        "MODE" => {
            let _ = writeln!(text, "  %modeout{dest} = alloca double");
            let _ = writeln!(
                text,
                "  %modena{dest} = call i32 @bn_rt_math_mode_i32(ptr %statptr{dest}, i32 %statlen{dest}, ptr %modeout{dest})"
            );
            let _ = writeln!(text, "  %modeis{dest} = icmp ne i32 %modena{dest}, 0");
            let _ = writeln!(text, "  %modeval{dest} = load double, ptr %modeout{dest}");
            let _ = writeln!(
                text,
                "  %modefat{dest} = insertvalue {{ i1, double }} undef, i1 %modeis{dest}, 0"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, double }} %modefat{dest}, double %modeval{dest}, 1"
            );
        }
        "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "RANGE" | "STDEV" | "VARIANCE" => {
            let intrinsic = match method {
                "MEAN" => "bn_rt_math_mean_i32",
                "MEDIAN" => "bn_rt_math_median_i32",
                "QUARTILE1" => "bn_rt_math_quartile1_i32",
                "QUARTILE3" => "bn_rt_math_quartile3_i32",
                "RANGE" => "bn_rt_math_range_i32",
                "STDEV" => "bn_rt_math_stdev_i32",
                _ => "bn_rt_math_variance_i32",
            };
            let _ = writeln!(
                text,
                "  %v{dest} = call double @{intrinsic}(ptr %statptr{dest}, i32 %statlen{dest})"
            );
        }
        _ => unreachable!("validated vector BNMath"),
    }
}

fn lower_integer_math(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    types: &[&Type],
    result_type: &Type,
) {
    let left = extend_to_i64(text, arguments[0], types[0]);
    let result_ty = llvm_type(result_type).expect("validated integer math result type");
    let call = match method {
        "ABS" => format!("call i64 @bn_rt_math_iabs(i64 {left})"),
        "SIGN" => format!("call i64 @bn_rt_math_isign(i64 {left})"),
        "MIN" => {
            let right = extend_to_i64(text, arguments[1], types[1]);
            format!("call i64 @bn_rt_math_imin(i64 {left}, i64 {right})")
        }
        "MAX" => {
            let right = extend_to_i64(text, arguments[1], types[1]);
            format!("call i64 @bn_rt_math_imax(i64 {left}, i64 {right})")
        }
        _ => unreachable!("validated integer BNMath"),
    };
    if result_ty == "i64" {
        let _ = writeln!(text, "  %v{} = {call}", destination.0);
    } else {
        let _ = writeln!(text, "  %mathi64{} = {call}", destination.0);
        let _ = writeln!(
            text,
            "  %v{} = trunc i64 %mathi64{} to {result_ty}",
            destination.0, destination.0
        );
    }
}

fn lower_float_math(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    types: &[&Type],
) {
    let args = arguments
        .iter()
        .zip(types)
        .map(|(argument, ty)| extend_to_double(text, *argument, ty))
        .collect::<Vec<_>>();
    let intrinsic = match method {
        "ABS" => "bn_rt_math_fabs",
        "SIGN" => "bn_rt_math_fsign",
        "FLOOR" => "bn_rt_math_floor",
        "CEIL" => "bn_rt_math_ceil",
        "TRUNC" => "bn_rt_math_trunc",
        "EXP" => "bn_rt_math_exp",
        "LOG" => "bn_rt_math_log",
        "LOG10" => "bn_rt_math_log10",
        "LOG2" => "bn_rt_math_log2",
        "SIN" => "bn_rt_math_sin",
        "COS" => "bn_rt_math_cos",
        "TAN" => "bn_rt_math_tan",
        "ASIN" => "bn_rt_math_asin",
        "ACOS" => "bn_rt_math_acos",
        "ATAN" => "bn_rt_math_atan",
        "SQRT" => "bn_rt_math_sqrt",
        "POW" => "bn_rt_math_pow",
        "ATAN2" => "bn_rt_math_atan2",
        "HYPOT" => "bn_rt_math_hypot",
        "MIN" => "bn_rt_math_fmin",
        "MAX" => "bn_rt_math_fmax",
        "ROUND" => "bn_rt_math_round",
        "FMA" => "bn_rt_math_fma",
        _ => unreachable!("validated float BNMath"),
    };
    let joined = args
        .iter()
        .map(|value| format!("double {value}"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        text,
        "  %v{} = call double @{intrinsic}({joined})",
        destination.0
    );
}

fn extend_to_double(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated numeric type") {
        "double" => format!("%v{}", value.0),
        "float" => {
            let temp = format!("mathf64{}", value.0);
            let _ = writeln!(text, "  %{temp} = fpext float %v{} to double", value.0);
            format!("%{temp}")
        }
        llvm_ty => {
            let opcode = if is_unsigned(ty) { "uitofp" } else { "sitofp" };
            let temp = format!("mathf64{}", value.0);
            let _ = writeln!(
                text,
                "  %{temp} = {opcode} {llvm_ty} %v{} to double",
                value.0
            );
            format!("%{temp}")
        }
    }
}
