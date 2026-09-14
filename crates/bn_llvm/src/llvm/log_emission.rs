#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_bnlog_status(text: &mut String, destination: ValueId, call: &str) {
    let dest = destination.0;
    let _ = writeln!(text, "  %logrc{dest} = {call}");
    let _ = writeln!(text, "  %logerr{dest} = icmp ne i32 %logrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %logmsg{dest} = select i1 %logerr{dest}, ptr @.bn_log_error, ptr null"
    );
    let _ = writeln!(text, "  %logcode{dest} = sext i32 %logrc{dest} to i64");
    let _ = writeln!(
        text,
        "  %logagg0_{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %logerr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %logagg1_{dest} = insertvalue {{ i1, ptr, i64 }} %logagg0_{dest}, ptr %logmsg{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %logagg1_{dest}, i64 %logcode{dest}, 2"
    );
}

pub(crate) fn lower_bnlog_call(
    text: &mut String,
    destination: ValueId,
    method: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    if matches!(method, "fields_constructor" | "logger_constructor") {
        return;
    }
    let handle = |text: &mut String, tag: &str, value: ValueId| {
        let name = format!("%logarg{}_{}_handle", destination.0, tag);
        let _ = writeln!(text, "  {name} = ptrtoint ptr %v{} to i64", value.0);
        name
    };
    let integer = |text: &mut String, value: ValueId| {
        extend_to_i64(
            text,
            value,
            analysis
                .values
                .get(&value)
                .expect("validated BNLog integer"),
        )
    };
    let call = match method {
        "fields_set_string" => {
            let receiver = handle(text, "receiver", arguments[0]);
            format!(
                "call i32 @bn_rt_log_fields_set_string(i64 {receiver}, ptr %v{}, ptr %v{})",
                arguments[1].0, arguments[2].0
            )
        }
        "logger_add_file" => {
            let receiver = handle(text, "receiver", arguments[0]);
            let minimum = integer(text, arguments[2]);
            format!(
                "call i32 @bn_rt_log_logger_add_file(i64 {receiver}, ptr %v{}, i64 {minimum})",
                arguments[1].0
            )
        }
        "logger_log" => {
            let receiver = handle(text, "receiver", arguments[0]);
            let fields = handle(text, "fields", arguments[3]);
            let level = integer(text, arguments[1]);
            format!(
                "call i32 @bn_rt_log_logger_log(i64 {receiver}, i64 {level}, ptr %v{}, i64 {fields})",
                arguments[2].0
            )
        }
        "logger_flush" | "logger_close" => {
            let receiver = handle(text, "receiver", arguments[0]);
            let timeout = integer(text, arguments[1]);
            let symbol = if method == "logger_flush" {
                "bn_rt_log_logger_flush"
            } else {
                "bn_rt_log_logger_close"
            };
            format!("call i32 @{symbol}(i64 {receiver}, i64 {timeout})")
        }
        _ => unreachable!("validated BNLog method"),
    };
    lower_bnlog_status(text, destination, &call);
}
