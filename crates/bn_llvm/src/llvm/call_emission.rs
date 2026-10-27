#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::runtime::is_bndata_function;
use super::*;
use runtime_abi::*;
use traps::Fact;
#[path = "crypto_calls.rs"]
mod crypto_calls;
#[path = "json_calls.rs"]
mod json_calls;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

#[allow(clippy::too_many_arguments)]
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower_call_instruction(
    text: &mut crate::ir::LlvmModule,
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
        bn_ir::names::FOR_CONDITION => {
            lower_for_condition(text, block_id, *destination, arguments, analysis, state);
        }
        bn_ir::names::FOR_FITS => {
            lower_for_fits(text, *destination, arguments, analysis);
        }
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
                "set_label" => lower_bndata_set_label(text, *destination, arguments, analysis),
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
                "select" => lower_bndata_select(text, *destination, arguments, analysis),
                "slice" => lower_bndata_slice(text, *destination, arguments, analysis),
                "transpose" => lower_bndata_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_transpose",
                    analysis,
                ),
                "append_rows" => lower_bndata_binary_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_append_rows",
                    analysis,
                ),
                "append_columns" => lower_bndata_binary_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_append_columns",
                    analysis,
                ),
                "join" => lower_bndata_join(text, *destination, arguments, 0, analysis),
                "left_join" => lower_bndata_join(text, *destination, arguments, 1, analysis),
                "right_join" => lower_bndata_join(text, *destination, arguments, 2, analysis),
                "full_join" => lower_bndata_join(text, *destination, arguments, 3, analysis),
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
                "count_na" => lower_bndata_count_na(text, *destination, arguments, analysis),
                "fill_na" => lower_bndata_fill_na(text, *destination, arguments, analysis),
                "drop_na" => lower_bndata_transform(
                    text,
                    *destination,
                    arguments,
                    "bn_rt_dataframe_drop_na",
                    analysis,
                ),
                _ => unreachable!("validated BNData DataFrame method"),
            }
        }
        name if is_bndata_function(name) => {
            let dest = destination.0;
            let union = T::struct_of([T::I1, T::Ptr, T::I64]);
            let r = |name: &str| O::reg(format!("csv{name}{dest}"));
            let flag = |text: &mut crate::ir::LlvmModule, slot: String, operand: ValueId| {
                text.assign(slot, I::cast(CastOp::ZExt, T::I1, v(operand), T::I8));
            };
            if name.ends_with("WriteCSV") {
                let file = I::extract(union.clone(), v(arguments[0]), 2);
                text.assign(format!("csvfilew{dest}"), file);
                let frame = analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated DataFrame argument");
                let frame = if llvm_type(frame) == Some(crate::layout::handle_result_ty()) {
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
                emit_void_call_result(text, *destination, call);
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
                &O::reg(format!("csvrc{dest}")),
                &O::reg(format!("csvvalue{dest}")),
            );
        }
        name if bnlog_method(module, name).is_some() => lower_bnlog_call(
            text,
            module,
            *destination,
            bnlog_method(module, name).expect("validated BNLog method"),
            arguments,
            analysis,
        ),
        name if bnweb_method(module, name).is_some() => lower_bnweb_call(
            text,
            module,
            *destination,
            bnweb_method(module, name).unwrap_or_else(|| {
                panic!("{}", bn_catalog::text("llvm", "msg.validated_bnweb_method"))
            }),
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
        "Date.Parse" => {
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::INVALID_DATE,
                    vec![(
                        "message",
                        Fact::Text(bn_catalog::text("llvm", "msg.date_must_be_yyyy_mm_dd").into()),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_DATE_PARSE.call([v(arguments[0]), trap]),
            );
        }
        "Time.Parse" => {
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::INVALID_TIME,
                    vec![(
                        "message",
                        Fact::Text(
                            bn_catalog::text("llvm", "msg.time_must_be_hh_mm_ss_mmm").into(),
                        ),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_TIME_PARSE.call([v(arguments[0]), trap]),
            );
        }
        "Timestamp.Parse" => {
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::PARSE_ERROR,
                    vec![(
                        "message",
                        Fact::Text(
                            bn_catalog::text("llvm", "msg.invalid_rfc_3339_timestamp").into(),
                        ),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_TIMESTAMP_PARSE.call([v(arguments[0]), trap]),
            );
        }
        "Timestamp.Format" => {
            let arg_ty = analysis.values.get(&arguments[0]).unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_timestamp_argument")
                )
            });
            let value = helpers::extend_to_i64(text, arguments[0], arg_ty);
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::FORMAT_OUT_OF_RANGE,
                    vec![(
                        "message",
                        Fact::Text(bn_core_text::civil::RANGE_ERROR.into()),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_TIMESTAMP_FORMAT.call([value, trap]),
            );
        }
        "TimeZone.Offset" => {
            let arg_ty = analysis.values.get(&arguments[1]).unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_timestamp_argument")
                )
            });
            let ts = helpers::extend_to_i64(text, arguments[1], arg_ty);
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::INVALID_TIMEZONE,
                    vec![(
                        "message",
                        Fact::Text(bn_catalog::text("temporal", "msg.invalid_timezone_2").into()),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_TIMEZONE_OFFSET.call([v(arguments[0]), ts, trap]),
            );
        }
        "TimeZone.IsDST" => {
            let arg_ty = analysis.values.get(&arguments[1]).unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_timestamp_argument")
                )
            });
            let ts = helpers::extend_to_i64(text, arguments[1], arg_ty);
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::INVALID_TIMEZONE,
                    vec![(
                        "message",
                        Fact::Text(bn_catalog::text("temporal", "msg.invalid_timezone_2").into()),
                    )],
                )
                .0,
            );
            let call_res = format!("dst_res{}", destination.0);
            text.assign(
                call_res.clone(),
                TEMPORAL_TIMEZONE_IS_DST.call([v(arguments[0]), ts, trap]),
            );
            text.assign(
                format!("v{}", destination.0),
                I::icmp(ICmpCond::Ne, T::I32, O::reg(call_res), O::int(0)),
            );
        }
        "Timestamp.AtZone" => {
            let arg_ty = analysis.values.get(&arguments[0]).unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_timestamp_argument")
                )
            });
            let ts = helpers::extend_to_i64(text, arguments[0], arg_ty);
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::INVALID_TIMEZONE,
                    vec![(
                        "message",
                        Fact::Text(bn_catalog::text("temporal", "msg.invalid_timezone_2").into()),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_TIMESTAMP_AT_ZONE.call([ts, v(arguments[1]), trap]),
            );
        }
        "Timestamp.FromLocal" => {
            let trap = O::global(
                traps::trap_symbol(
                    state,
                    bn_diag::DiagId::INVALID_TIMEZONE,
                    vec![(
                        "message",
                        Fact::Text(bn_catalog::text("temporal", "msg.invalid_timezone_2").into()),
                    )],
                )
                .0,
            );
            text.assign(
                format!("v{}", destination.0),
                TEMPORAL_TIMESTAMP_FROM_LOCAL.call([
                    v(arguments[0]),
                    v(arguments[1]),
                    v(arguments[2]),
                    trap,
                ]),
            );
        }
        #[cfg(feature = "json")]
        name if bnjson_member(module, name).is_some() => {
            let member = bnjson_member(module, name).expect("validated BNJson member");
            json_calls::lower_bnjson_call(text, analysis, *destination, member, arguments);
        }
        name if bncrypto_method(module, name).is_some() => {
            let member = bncrypto_method(module, name).expect("validated BNCrypto member");
            crypto_calls::lower_bncrypto_call(text, analysis, *destination, member, arguments);
        }
        #[cfg(feature = "sqlite")]
        name if bnsqlite_method(module, name).is_some() => {
            let method = bnsqlite_method(module, name).expect("validated BNSqlite member");
            lower_sqlite_call(text, *destination, method, arguments, analysis);
        }
        name if bnffi_method(module, name).is_some() => {
            let member = bnffi_method(module, name).expect("validated BNFfi member");
            lower_bnffi_call(text, analysis, *destination, member, arguments);
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
            let message = crate::layout::string_payload(".bn_asc_error");
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
            let code = extend_to_i64(text, argument, ty);
            let r = |name: &str| O::reg(format!("char{name}{dest}"));
            let call = I::call(T::Ptr, "bn_rt_str_from_char", vec![(T::I64, code)]);
            text.assign(format!("charptr{dest}"), call);
            text.assign(
                format!("charerror{dest}"),
                I::icmp(ICmpCond::Eq, T::Ptr, r("ptr"), O::null()),
            );
            let message = crate::layout::string_payload(".bn_char_error");
            text.assign(
                format!("charvalue{dest}"),
                I::select(r("error"), T::Ptr, message, r("ptr")),
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
            .any(|function| function.name == name.strip_prefix("@super:").unwrap_or(name))
            || crate::analysis_calls::interface_call_targets(module, name).is_some() =>
        {
            crate::traps::emit_stack_check(
                text,
                block_id,
                name.strip_prefix("@super:").unwrap_or(name),
                state,
            );
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
                I::call(T::I32, "bn_rt_random_seed", vec![(T::I64, seed)]),
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
fn emit_wrapped_error(text: &mut crate::ir::LlvmModule, prefix: &str, dest: u32, message: O) {
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
