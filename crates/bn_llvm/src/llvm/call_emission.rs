#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::runtime::is_bndata_function;
use super::*;

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
            if name.ends_with("WriteCSV") {
                let _ = writeln!(
                    text,
                    "  %csvfilew{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                    arguments[0].0
                );
                if llvm_type(
                    analysis
                        .values
                        .get(&arguments[1])
                        .expect("validated DataFrame argument"),
                ) == Some("{ i1, ptr, i64 }")
                {
                    let _ = writeln!(
                        text,
                        "  %csvframew{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                        arguments[1].0
                    );
                } else {
                    let _ = writeln!(
                        text,
                        "  %csvframew{dest} = ptrtoint ptr %v{} to i64",
                        arguments[1].0
                    );
                }
                let _ = writeln!(
                    text,
                    "  %csvheaderw{dest} = zext i1 %v{} to i8",
                    arguments[2].0
                );
                emit_void_result(
                    text,
                    *destination,
                    format!(
                        "call i32 @bn_rt_dataframe_write_csv(i64 %csvfilew{dest}, i64 %csvframew{dest}, i8 %csvheaderw{dest}, ptr %v{})",
                        arguments[3].0
                    ),
                );
                return Ok(());
            }
            let _ = writeln!(
                text,
                "  %csvfile{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            let _ = writeln!(
                text,
                "  %csvheader{dest} = zext i1 %v{} to i8",
                arguments[1].0
            );
            let _ = writeln!(text, "  %csvout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %csvrc{dest} = call i32 @bn_rt_dataframe_read_csv(i64 %csvfile{dest}, i8 %csvheader{dest}, ptr %v{}, ptr %csvout{dest})",
                arguments[2].0
            );
            let _ = writeln!(text, "  %csverr{dest} = icmp ne i32 %csvrc{dest}, 0");
            let _ = writeln!(text, "  %csvvalue{dest} = load i64, ptr %csvout{dest}");
            let _ = writeln!(
                text,
                "  %csvmsg{dest} = select i1 %csverr{dest}, ptr @.bn_dataframe_error, ptr null"
            );
            let _ = writeln!(
                text,
                "  %csvpayload{dest} = select i1 %csverr{dest}, i64 1, i64 %csvvalue{dest}"
            );
            let _ = writeln!(
                text,
                "  %csvagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %csverr{dest}, 0"
            );
            let _ = writeln!(
                text,
                "  %csvagg1{dest} = insertvalue {{ i1, ptr, i64 }} %csvagg0{dest}, ptr %csvmsg{dest}, 1"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %csvagg1{dest}, i64 %csvpayload{dest}, 2"
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
                *destination,
                bnmath_method(module, name).expect("validated BNMath"),
                arguments,
                analysis,
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
            let argument = arguments[0];
            let _ = writeln!(
                text,
                "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                destination.0, argument.0
            );
        }
        name if bncrypto_method(module, name).is_some() => {
            let dest = destination.0;
            let argument = arguments[0].0;
            match bncrypto_method(module, name).expect("validated BNCrypto member") {
                "SHA256" => {
                    let _ = writeln!(
                        text,
                        "  %v{dest} = call ptr @bn_rt_crypto_sha256(ptr %v{argument})"
                    );
                }
                "SHA512" => {
                    let _ = writeln!(
                        text,
                        "  %v{dest} = call ptr @bn_rt_crypto_sha512(ptr %v{argument})"
                    );
                }
                // The handle travels as a ptr, as BNLog resources do.
                "FromText" => {
                    let _ = writeln!(
                        text,
                        "  %cryh{dest} = call i64 @bn_rt_crypto_bytes_from_text(ptr %v{argument})"
                    );
                    let _ = writeln!(text, "  %v{dest} = inttoptr i64 %cryh{dest} to ptr");
                }
                "Length" => {
                    emit_bncrypto_handle(text, analysis, &format!("%cryh{dest}"), arguments[0]);
                    let _ = writeln!(
                        text,
                        "  %cryl{dest} = call i64 @bn_rt_crypto_bytes_length(i64 %cryh{dest})"
                    );
                    let _ = writeln!(text, "  %v{dest} = trunc i64 %cryl{dest} to i32");
                }
                "ToHex" => {
                    emit_bncrypto_handle(text, analysis, &format!("%cryh{dest}"), arguments[0]);
                    let _ = writeln!(
                        text,
                        "  %v{dest} = call ptr @bn_rt_crypto_bytes_to_hex(i64 %cryh{dest})"
                    );
                }
                member @ ("SealAesGcm" | "OpenAesGcm" | "SealChaCha20" | "OpenChaCha20") => {
                    let selector = i32::from(member.ends_with("ChaCha20"));
                    let symbol = if member.starts_with("Seal") {
                        "bn_rt_crypto_seal"
                    } else {
                        "bn_rt_crypto_open"
                    };
                    for (index, operand) in arguments.iter().enumerate() {
                        emit_bncrypto_handle(
                            text,
                            analysis,
                            &format!("%cryarg{dest}_{index}"),
                            *operand,
                        );
                    }
                    let _ = writeln!(text, "  %cryout{dest} = alloca i64");
                    let _ = writeln!(
                        text,
                        "  %cryrc{dest} = call i32 @{symbol}(i32 {selector}, i64 %cryarg{dest}_0, i64 %cryarg{dest}_1, i64 %cryarg{dest}_2, i64 %cryarg{dest}_3, ptr %cryout{dest})"
                    );
                    let _ = writeln!(text, "  %cryhandle{dest} = load i64, ptr %cryout{dest}");
                    emit_handle_result(
                        text,
                        *destination,
                        format!("%cryrc{dest}"),
                        format!("%cryhandle{dest}"),
                    );
                }
                "FromHex" => {
                    let _ = writeln!(text, "  %cryout{dest} = alloca i64");
                    let _ = writeln!(
                        text,
                        "  %cryrc{dest} = call i32 @bn_rt_crypto_bytes_from_hex(ptr %v{argument}, ptr %cryout{dest})"
                    );
                    let _ = writeln!(text, "  %cryhandle{dest} = load i64, ptr %cryout{dest}");
                    emit_handle_result(
                        text,
                        *destination,
                        format!("%cryrc{dest}"),
                        format!("%cryhandle{dest}"),
                    );
                }
                other => unreachable!("unsupported BNCrypto member reached emission: {other}"),
            }
        }
        "TOLOWER" => {
            let dest = destination.0;
            let argument = arguments[0];
            let _ = writeln!(
                text,
                "  %v{dest} = call ptr @bn_rt_str_to_lower(ptr %v{})",
                argument.0
            );
        }
        "TOUPPER" => {
            let dest = destination.0;
            let argument = arguments[0];
            let _ = writeln!(
                text,
                "  %v{dest} = call ptr @bn_rt_str_to_upper(ptr %v{})",
                argument.0
            );
        }
        "ASC" => {
            let dest = destination.0;
            let argument = arguments[0];
            let _ = writeln!(
                text,
                "  %asccode{dest} = call i64 @bn_rt_str_asc(ptr %v{})",
                argument.0
            );
            let _ = writeln!(text, "  %ascerror{dest} = icmp slt i64 %asccode{dest}, 0");
            let _ = writeln!(
                text,
                "  %ascmessage{dest} = select i1 %ascerror{dest}, ptr @.bn_asc_error, ptr null"
            );
            let _ = writeln!(
                text,
                "  %ascpayload{dest} = select i1 %ascerror{dest}, i64 1, i64 %asccode{dest}"
            );
            let _ = writeln!(
                text,
                "  %ascagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %ascerror{dest}, 0"
            );
            let _ = writeln!(
                text,
                "  %ascagg1{dest} = insertvalue {{ i1, ptr, i64 }} %ascagg0{dest}, ptr %ascmessage{dest}, 1"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %ascagg1{dest}, i64 %ascpayload{dest}, 2"
            );
        }
        "CHAR" => {
            let dest = destination.0;
            let argument = arguments[0];
            let code = extend_to_i64(
                text,
                argument,
                analysis
                    .values
                    .get(&argument)
                    .expect("validated CHAR argument"),
            );
            let _ = writeln!(
                text,
                "  %charpacked{dest} = call i64 @bn_rt_str_char_utf8(i64 {code})"
            );
            let _ = writeln!(
                text,
                "  %charerror{dest} = icmp eq i64 %charpacked{dest}, -1"
            );
            let _ = writeln!(text, "  %charbuffer{dest} = alloca i64");
            let _ = writeln!(text, "  store i64 %charpacked{dest}, ptr %charbuffer{dest}");
            let _ = writeln!(
                text,
                "  %charvalue{dest} = select i1 %charerror{dest}, ptr @.bn_char_error, ptr %charbuffer{dest}"
            );
            let _ = writeln!(
                text,
                "  %charpayload{dest} = select i1 %charerror{dest}, i64 1, i64 0"
            );
            let _ = writeln!(
                text,
                "  %charagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %charerror{dest}, 0"
            );
            let _ = writeln!(
                text,
                "  %charagg1{dest} = insertvalue {{ i1, ptr, i64 }} %charagg0{dest}, ptr %charvalue{dest}, 1"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %charagg1{dest}, i64 %charpayload{dest}, 2"
            );
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
                &format!("call i32 @bn_rt_random_seed(i64 {seed})"),
                state,
            );
        }
        "HOST.Random.Random" => {
            let _ = writeln!(
                text,
                "  %v{} = call double @bn_rt_random_next()",
                destination.0
            );
        }
        _ => unreachable!("validated call target"),
    }
    Ok(())
}

/// Materialises `%cryh{dest}`, the `bn_rt` table index behind a `BNCrypto.Bytes`
/// operand. A plain handle arrives as a pointer; a value narrowed out of
/// `Bytes OR Error` arrives as the `{ i1, ptr, i64 }` aggregate, as `FS.File`
/// does.
fn emit_bncrypto_handle(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    slot: &str,
    operand: ValueId,
) {
    let source = operand.0;
    if matches!(analysis.values.get(&operand), Some(Type::Alternative(_))) {
        let _ = writeln!(
            text,
            "  {slot} = extractvalue {{ i1, ptr, i64 }} %v{source}, 2"
        );
    } else {
        let _ = writeln!(text, "  {slot} = ptrtoint ptr %v{source} to i64");
    }
}
