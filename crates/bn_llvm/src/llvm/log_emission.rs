#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::handle_result_ty;

/// The `VOID OR Error` of a `bn_rt_log_*` call: on a non-zero status the
/// error record `bn_rt` left and its code.
fn lower_bnlog_status(text: &mut String, destination: ValueId, call: I) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("log{name}{dest}"));
    let union = handle_result_ty();
    text.assign(format!("logrc{dest}"), call);
    text.assign(
        format!("logerr{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, r("rc"), O::int(0)),
    );
    text.assign(
        format!("logerrint{dest}"),
        I::cast(CastOp::ZExt, T::I1, r("err"), T::I32),
    );
    let args = vec![(T::I32, r("errint")), (T::Ptr, O::null())];
    text.assign(
        format!("logfail{dest}"),
        I::call(T::Ptr, "bn_rt_error_take", args),
    );
    let args = vec![(T::Ptr, r("fail"))];
    text.assign(
        format!("logcode{dest}"),
        I::call(T::I64, "bn_rt_error_code", args),
    );
    let payload = I::select(r("err"), T::I64, r("code"), O::int(0));
    text.assign(format!("logpayload{dest}"), payload);
    let head = I::insert(union.clone(), O::undef(), T::I1, r("err"), 0);
    text.assign(format!("logagg0_{dest}"), head);
    let agg0 = O::reg(format!("logagg0_{dest}"));
    text.assign(
        format!("logagg1_{dest}"),
        I::insert(union.clone(), agg0, T::Ptr, r("fail"), 1),
    );
    let agg1 = O::reg(format!("logagg1_{dest}"));
    text.assign(
        format!("v{dest}"),
        I::insert(union, agg1, T::I64, r("payload"), 2),
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
    let v = |index: usize| (T::Ptr, O::reg(format!("v{}", arguments[index].0)));
    let handle = |text: &mut String, tag: &str, index: usize| {
        let name = format!("logarg{}_{tag}_handle", destination.0);
        let (_, pointer) = v(index);
        text.assign(&name, I::cast(CastOp::PtrToInt, T::Ptr, pointer, T::I64));
        (T::I64, O::reg(name))
    };
    let integer = |text: &mut String, index: usize| {
        let value = arguments[index];
        let ty = analysis
            .values
            .get(&value)
            .expect("validated BNLog integer");
        (T::I64, O::raw(extend_to_i64(text, value, ty)))
    };
    let (symbol, args) = match method {
        "fields_set_string" => {
            let receiver = handle(text, "receiver", 0);
            ("bn_rt_log_fields_set_string", vec![receiver, v(1), v(2)])
        }
        "fields_set_integer" => {
            let receiver = handle(text, "receiver", 0);
            let value = integer(text, 2);
            ("bn_rt_log_fields_set_integer", vec![receiver, v(1), value])
        }
        "fields_set_boolean" => {
            let receiver = handle(text, "receiver", 0);
            let byte = format!("logbool{}", destination.0);
            let (_, flag) = v(2);
            text.assign(&byte, I::cast(CastOp::ZExt, T::I1, flag, T::I8));
            (
                "bn_rt_log_fields_set_boolean",
                vec![receiver, v(1), (T::I8, O::reg(byte))],
            )
        }
        "logger_add_null" | "logger_add_console" => {
            let receiver = handle(text, "receiver", 0);
            let minimum = integer(text, 1);
            let symbol = if method == "logger_add_console" {
                "bn_rt_log_logger_add_console"
            } else {
                "bn_rt_log_logger_add_null"
            };
            (symbol, vec![receiver, minimum])
        }
        "logger_add_file" => {
            let receiver = handle(text, "receiver", 0);
            let minimum = integer(text, 2);
            ("bn_rt_log_logger_add_file", vec![receiver, v(1), minimum])
        }
        "logger_log" => {
            let receiver = handle(text, "receiver", 0);
            let fields = handle(text, "fields", 3);
            let level = integer(text, 1);
            ("bn_rt_log_logger_log", vec![receiver, level, v(2), fields])
        }
        "logger_flush" | "logger_close" => {
            let receiver = handle(text, "receiver", 0);
            let timeout = integer(text, 1);
            let symbol = if method == "logger_flush" {
                "bn_rt_log_logger_flush"
            } else {
                "bn_rt_log_logger_close"
            };
            (symbol, vec![receiver, timeout])
        }
        _ => unreachable!("validated BNLog method"),
    };
    lower_bnlog_status(text, destination, I::call(T::I32, symbol, args));
}
