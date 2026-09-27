#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;

pub(crate) fn lower_dispatch_emission(
    text: &mut String,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::DispatchSubmit {
            destination,
            queue,
            task,
            arguments,
            ..
        } => {
            let task_name = analysis
                .functions
                .get(task)
                .copied()
                .expect("validated async task target");
            let _ = writeln!(
                text,
                "  %dispatchqueue{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, queue.0
            );
            let _ = writeln!(text, "  %dispatchticket{} = alloca i64", destination.0);
            let (arg_ptr, arg_count) = if arguments.is_empty() {
                ("null".into(), "0".into())
            } else {
                let byte_count = arguments.len() * 16;
                let _ = writeln!(
                    text,
                    "  %dispatchargs{} = alloca [{byte_count} x i8]",
                    destination.0
                );
                let _ = writeln!(
                    text,
                    "  %dispatchargbase{} = getelementptr [{byte_count} x i8], ptr %dispatchargs{}, i64 0, i64 0",
                    destination.0, destination.0,
                );
                for (index, argument) in arguments.iter().enumerate() {
                    let argument_ty = analysis
                        .values
                        .get(argument)
                        .expect("validated dispatch argument");
                    let offset = index * 16;
                    let _ = writeln!(
                        text,
                        "  %dispatcharg{}_{index} = getelementptr i8, ptr %dispatchargbase{}, i64 {offset}",
                        destination.0, destination.0,
                    );
                    let kind = match argument_ty {
                        Type::Boolean => 1,
                        Type::Integer(_) | Type::IntegerLiteral(_) => 2,
                        Type::Float(_) | Type::FloatLiteral => 3,
                        Type::String => 4,
                        _ => unreachable!("validated dispatch scalar argument"),
                    };
                    let _ = writeln!(
                        text,
                        "  store i32 {kind}, ptr %dispatcharg{}_{index}",
                        destination.0
                    );
                    let _ = writeln!(
                        text,
                        "  %dispatchargpayload{}_{index} = getelementptr i8, ptr %dispatcharg{}_{index}, i64 8",
                        destination.0, destination.0,
                    );
                    match argument_ty {
                        Type::Boolean => {
                            let _ = writeln!(
                                text,
                                "  %dispatchargbool{}_{index} = zext i1 %v{} to i64",
                                destination.0, argument.0
                            );
                            let _ = writeln!(
                                text,
                                "  store i64 %dispatchargbool{}_{index}, ptr %dispatchargpayload{}_{index}",
                                destination.0, destination.0
                            );
                        }
                        Type::Integer(_) | Type::IntegerLiteral(_) => {
                            let value = coerce_to_type(
                                text,
                                *argument,
                                argument_ty,
                                &Type::Integer(IntegerType::Int64),
                            );
                            let _ = writeln!(
                                text,
                                "  store i64 {value}, ptr %dispatchargpayload{}_{index}",
                                destination.0
                            );
                        }
                        Type::Float(_) | Type::FloatLiteral => {
                            let _ = writeln!(
                                text,
                                "  store double %v{}, ptr %dispatchargpayload{}_{index}",
                                argument.0, destination.0
                            );
                        }
                        Type::String => {
                            let _ = writeln!(
                                text,
                                "  store ptr %v{}, ptr %dispatchargpayload{}_{index}",
                                argument.0, destination.0
                            );
                        }
                        _ => unreachable!("validated dispatch scalar argument"),
                    }
                }
                (
                    format!("%dispatchargbase{}", destination.0),
                    arguments.len().to_string(),
                )
            };
            let _ = writeln!(
                text,
                "  %dispatchrc{} = call i32 @bn_rt_dispatch_submit(i64 %dispatchqueue{}, ptr @{}, ptr null, ptr {arg_ptr}, i32 {arg_count}, ptr %dispatchticket{})",
                destination.0,
                destination.0,
                dispatch_trampoline_symbol(task_name),
                destination.0
            );
            let _ = writeln!(
                text,
                "  %dispatchhandle{} = load i64, ptr %dispatchticket{}",
                destination.0, destination.0
            );
            emit_handle_result(
                text,
                *destination,
                format!("%dispatchrc{}", destination.0),
                format!("%dispatchhandle{}", destination.0),
            );
        }
        Instruction::DispatchAwait {
            destination,
            ticket,
            timeout,
            ty,
            ..
        } => {
            let _ = writeln!(
                text,
                "  %dispatchticket{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, ticket.0
            );
            let timeout_ty = analysis
                .values
                .get(timeout)
                .expect("validated timeout type");
            let timeout = extend_to_i64(text, *timeout, timeout_ty);
            let _ = writeln!(
                text,
                "  %dispatchresult{} = alloca [32 x i8]",
                destination.0
            );
            let _ = writeln!(text, "  %dispatcherror{} = alloca [24 x i8]", destination.0);
            let call = format!(
                "call i32 @bn_rt_dispatch_await(i64 %dispatchticket{}, i64 {}, ptr %dispatchresult{}, ptr %dispatcherror{})",
                destination.0, timeout, destination.0, destination.0
            );
            match llvm_type(ty).unwrap_or("") {
                "{ i1, ptr, i64 }" => {
                    if matches!(ty, Type::Alternative(alternatives) if integer_or_error(alternatives))
                    {
                        emit_integer_dispatch_result(text, *destination, &call);
                    } else if matches!(ty, Type::Alternative(alternatives) if float_or_error(alternatives))
                    {
                        emit_float_dispatch_result(text, *destination, &call);
                    } else if matches!(ty, Type::Alternative(alternatives) if string_or_error(alternatives))
                    {
                        emit_string_dispatch_result(text, *destination, &call);
                    } else if matches!(ty, Type::Alternative(alternatives) if boolean_or_error(alternatives))
                    {
                        emit_boolean_dispatch_result(text, *destination, &call);
                    } else {
                        emit_void_result(text, *destination, call);
                    }
                }
                _ => emit_void_result(text, *destination, call),
            }
        }
        _ => return false,
    }
    true
}
