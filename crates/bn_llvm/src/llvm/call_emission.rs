#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::runtime::is_bndata_function;
use super::*;
#[path = "crypto_calls.rs"]
mod crypto_calls;
#[path = "json_calls.rs"]
mod json_calls;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower_call_instruction(
    text: &mut String,
    module: &Module,
    _function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _symbols: &HashMap<SymbolId, usize>,
    _block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    let Instruction::Call {
        destination,
        callee,
        arguments,
        ..
    } = instruction
    else {
        unreachable!("call emitter received another instruction");
    };
    let v = |id: ValueId| O::reg(format!("v{}", id.0));
    if !analysis.functions.contains_key(callee)
        && matches!(analysis.values.get(callee), Some(Type::Function { .. }))
    {
        lower_indirect_call(text, *destination, *callee, arguments, analysis);
        return Ok(());
    }
    match analysis
        .functions
        .get(callee)
        .copied()
        .expect("validated callee")
    {
        "$for_condition" => lower_for_condition(text, *destination, arguments, analysis),
        name if is_bndata_dataframe_call(module, name) => {
            match bndata_dataframe_method(name).expect("validated BNData method") {
                "constructor" => {}
                "row_count" | "column_count" => lower_bndata_count(
                    text,
                    block_id,
                    *destination,
                    arguments,
                    if name.ends_with("RowCount") {
                        "bn_rt_dataframe_row_count"
                    } else {
                        "bn_rt_dataframe_column_count"
                    },
                    analysis,
                    state,
                ),
                "add_integer_column" => {
                    lower_bndata_add_integer_column(text, *destination, arguments, analysis);
                }
                "add_string_column" => lower_bndata_add_simple_column(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_add_string",
                ),
                "add_float_column" => lower_bndata_add_simple_column(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_add_float",
                ),
                "add_boolean_column" => lower_bndata_add_simple_column(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_add_boolean",
                ),
                "column_name" => {
                    lower_bndata_column_name(text, *destination, arguments, analysis);
                }
                "set_label" => lower_bndata_set_label(text, *destination, arguments),
                "get_string" => lower_bndata_status_call(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_get_string",
                ),
                "get_integer" => lower_bndata_status_call(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_get_integer",
                ),
                "get_float" => lower_bndata_status_call(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_get_float",
                ),
                "get_boolean" => lower_bndata_status_call(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_get_boolean",
                ),
                "mean" | "median" | "quartile1" | "quartile3" | "mode" | "stdev" | "variance"
                | "range" | "min" | "max" => {
                    let operation =
                        match bndata_dataframe_method(name).expect("validated reduction") {
                            "mean" => 0,
                            "median" => 1,
                            "quartile1" => 2,
                            "quartile3" => 3,
                            "mode" => 4,
                            "stdev" => 5,
                            "variance" => 6,
                            "range" => 7,
                            "min" => 8,
                            "max" => 9,
                            _ => unreachable!(),
                        };
                    lower_bndata_reduce(text, *destination, arguments, operation, analysis);
                }
                "zscore" => lower_bndata_zscore(text, *destination, arguments, analysis),
                "copy_integer" => lower_bndata_copy(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_copy_integer",
                ),
                "copy_float" => lower_bndata_copy(
                    text,
                    *destination,
                    arguments,
                    analysis,
                    "bn_rt_dataframe_copy_float",
                ),
                "select" => lower_bndata_select(text, *destination, arguments),
                "slice" => lower_bndata_slice(text, *destination, arguments, analysis),
                "transpose" => lower_bndata_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_transpose",
                ),
                "append_rows" => lower_bndata_binary_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_append_rows",
                ),
                "append_columns" => lower_bndata_binary_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_append_columns",
                ),
                "join" => lower_bndata_join(text, *destination, arguments, 0),
                "left_join" => lower_bndata_join(text, *destination, arguments, 1),
                "right_join" => lower_bndata_join(text, *destination, arguments, 2),
                "full_join" => lower_bndata_join(text, *destination, arguments, 3),
                "convert_integer" => lower_bndata_convert(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_convert_integer",
                    analysis,
                ),
                "convert_float" => lower_bndata_convert(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_convert_float",
                    analysis,
                ),
                _ => unreachable!("validated BNData DataFrame method"),
            }
        }
        name if is_bndata_function(name) => {
            let dest = destination.0;
            let union = T::struct_of([T::I1, T::Ptr, T::I64]);
            let r = |name: &str| O::reg(format!("csv{name}{dest}"));
            let flag = |text: &mut String, slot: String, operand: ValueId| {
                text.assign(slot, I::cast(CastOp::ZExt, T::I1, v(operand), T::I8));
            };
            if name.ends_with("WriteCSV") {
                let file = I::extract(union.clone(), v(arguments[0]), 2);
                text.assign(format!("csvfilew{dest}"), file);
                let frame = analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated DataFrame argument");
                let frame = if llvm_type(frame) == Some("{ i1, ptr, i64 }") {
                    I::extract(union, v(arguments[1]), 2)
                } else {
                    I::cast(CastOp::PtrToInt, T::Ptr, v(arguments[1]), T::I64)
                };
                text.assign(format!("csvframew{dest}"), frame);
                flag(text, format!("csvheaderw{dest}"), arguments[2]);
                let args = vec![
                    (T::I64, r("filew")),
                    (T::I64, r("framew")),
                    (T::I8, r("headerw")),
                    (T::Ptr, v(arguments[3])),
                ];
                let call = I::call(T::I32, "bn_rt_dataframe_write_csv", args);
                emit_void_result(text, *destination, call.to_string());
                return Ok(());
            }
            text.assign(
                format!("csvfile{dest}"),
                I::extract(union, v(arguments[0]), 2),
            );
            flag(text, format!("csvheader{dest}"), arguments[1]);
            text.assign(format!("csvout{dest}"), I::alloca(T::I64));
            let args = vec![
                (T::I64, r("file")),
                (T::I8, r("header")),
                (T::Ptr, v(arguments[2])),
                (T::Ptr, r("out")),
            ];
            let call = I::call(T::I32, "bn_rt_dataframe_read_csv", args);
            text.assign(format!("csvrc{dest}"), call);
            text.assign(format!("csvvalue{dest}"), I::load(T::I64, r("out")));
            emit_handle_result(
                text,
                *destination,
                format!("%csvrc{dest}"),
                format!("%csvvalue{dest}"),
            );
        }
        name if bnlog_method(module, name).is_some() => lower_bnlog_call(
            text,
            *destination,
            bnlog_method(module, name).expect("validated BNLog method"),
            arguments,
            analysis,
        ),
        name if bnmath_method(module, name).is_some() => {
            lower_bnmath_call(
                text,
                block_id,
                *destination,
                bnmath_method(module, name).expect("validated BNMath"),
                arguments,
                analysis,
                state,
            );
        }
        name if is_bn_rt_host_call(name) => {
            lower_bn_rt_call(
                text,
                block_id,
                *destination,
                name,
                arguments,
                analysis,
                state,
            );
        }
        name if name.ends_with(".Queue.Concurrent")
            || name.ends_with(".Queue.Serial")
            || name.ends_with(".Queue.Auto")
            || name.ends_with(".Queue.Join")
            || name.ends_with(".Queue.Close")
            || name.ends_with(".Ticket.Close")
            || name.ends_with(".Ticket.Id")
            || name.ends_with(".Ticket.Status")
            || name.ends_with(".Ticket.Wait")
            || name.ends_with(".Ticket.Cancel")
            || name.ends_with(".Ticket.Error")
            || name.ends_with(".Ticket.IsDone")
            || name.ends_with(".Group.New")
            || name.ends_with(".Group.Enter")
            || name.ends_with(".Group.Leave")
            || name.ends_with(".Group.Wait")
            || name.ends_with(".Barrier.New")
            || name.ends_with(".Barrier.Wait")
            || name.ends_with(".Semaphore.New")
            || name.ends_with(".Semaphore.Acquire")
            || name.ends_with(".Semaphore.Release")
            || name.ends_with(".Mutex.New")
            || name.ends_with(".Mutex.Lock")
            || name.ends_with(".Mutex.Unlock") =>
        {
            lower_bn_dispatch_call(text, *destination, name, arguments, analysis);
        }
        "TimeZone.Parse" => {
            let index = vec![(T::I64, O::int(0))];
            text.assign(
                format!("v{}", destination.0),
                I::gep(T::I8, v(arguments[0]), index),
            );
        }
        name if bnjson_member(module, name).is_some() => {
            let member = bnjson_member(module, name).expect("validated BNJson member");
            json_calls::lower_bnjson_call(text, analysis, *destination, member, arguments);
        }
        name if bncrypto_method(module, name).is_some() => {
            let member = bncrypto_method(module, name).expect("validated BNCrypto member");
            crypto_calls::lower_bncrypto_call(text, analysis, *destination, member, arguments);
        }
        name if bnsqlite_method(module, name).is_some() => {
            let method = bnsqlite_method(module, name).expect("validated BNSqlite member");
            lower_sqlite_call(text, *destination, method, arguments, analysis);
        }
        case @ ("TOLOWER" | "TOUPPER") => {
            let symbol = format!("bn_rt_str_to_{}", case[2..].to_ascii_lowercase());
            let args = vec![(T::Ptr, v(arguments[0]))];
            text.assign(
                format!("v{}", destination.0),
                I::call(T::Ptr, &symbol, args),
            );
        }
        "ASC" => {
            let dest = destination.0;
            let r = |name: &str| O::reg(format!("asc{name}{dest}"));
            let args = vec![(T::Ptr, v(arguments[0]))];
            text.assign(
                format!("asccode{dest}"),
                I::call(T::I64, "bn_rt_str_asc", args),
            );
            text.assign(
                format!("ascerror{dest}"),
                I::icmp(ICmpCond::Slt, T::I64, r("code"), O::int(0)),
            );
            let message = O::global(".bn_asc_error");
            text.assign(
                format!("ascmessage{dest}"),
                I::select(r("error"), T::Ptr, message, O::null()),
            );
            text.assign(
                format!("ascpayload{dest}"),
                I::select(r("error"), T::I64, O::int(1), r("code")),
            );
            emit_wrapped_error(text, "asc", dest, r("message"));
        }
        "CHAR" => {
            let dest = destination.0;
            let argument = arguments[0];
            let ty = analysis
                .values
                .get(&argument)
                .expect("validated CHAR argument");
            let code = O::raw(extend_to_i64(text, argument, ty));
            let r = |name: &str| O::reg(format!("char{name}{dest}"));
            let call = I::call(T::I64, "bn_rt_str_char_utf8", vec![(T::I64, code)]);
            text.assign(format!("charpacked{dest}"), call);
            text.assign(
                format!("charerror{dest}"),
                I::icmp(ICmpCond::Eq, T::I64, r("packed"), O::int(-1)),
            );
            text.assign(format!("charbuffer{dest}"), I::alloca(T::I64));
            text.emit(I::store(T::I64, r("packed"), r("buffer")));
            let message = O::global(".bn_char_error");
            text.assign(
                format!("charvalue{dest}"),
                I::select(r("error"), T::Ptr, message, r("buffer")),
            );
            text.assign(
                format!("charpayload{dest}"),
                I::select(r("error"), T::I64, O::int(1), O::int(0)),
            );
            emit_wrapped_error(text, "char", dest, r("value"));
        }
        name if module
            .functions
            .iter()
            .any(|function| function.name == name.strip_prefix("@super:").unwrap_or(name)) =>
        {
            lower_user_call(text, module, *destination, name, arguments, analysis, state);
        }
        "HOST.Random.Seed" => {
            let seed = extend_to_i64(
                text,
                *arguments.first().expect("validated seed argument"),
                analysis
                    .values
                    .get(arguments.first().expect("validated seed argument"))
                    .expect("validated seed type"),
            );
            emit_checked_i32_eq_zero(
                text,
                block_id,
                *destination,
                I::call(T::I32, "bn_rt_random_seed", vec![(T::I64, O::raw(seed))]),
                &[bn_diag::DiagId::EXECUTION_POLICY_DENIED],
                state,
            );
        }
        "HOST.Random.Random" => {
            let call = I::call(T::Double, "bn_rt_random_next", vec![]);
            text.assign(format!("v{}", destination.0), call);
        }
        _ => unreachable!("validated call target"),
    }
    Ok(())
}

/// Packs `%<prefix>error`, `message` wrapped as an error record and
/// `%<prefix>payload` into the `{ i1, ptr, i64 }` of a `T OR Error` result.
fn emit_wrapped_error(text: &mut String, prefix: &str, dest: u32, message: O) {
    let r = |name: &str| O::reg(format!("{prefix}{name}{dest}"));
    let union = T::struct_of([T::I1, T::Ptr, T::I64]);
    let head = I::insert(union.clone(), O::undef(), T::I1, r("error"), 0);
    text.assign(format!("{prefix}agg0{dest}"), head);
    let args = vec![(T::I1, r("error")), (T::Ptr, message), (T::Ptr, O::null())];
    text.assign(
        format!("{prefix}agg1wrap{dest}"),
        I::call(T::Ptr, "bn_rt_error_wrap", args),
    );
    let wrapped = I::insert(union.clone(), r("agg0"), T::Ptr, r("agg1wrap"), 1);
    text.assign(format!("{prefix}agg1{dest}"), wrapped);
    let payload = I::insert(union, r("agg1"), T::I64, r("payload"), 2);
    text.assign(format!("v{dest}"), payload);
}
