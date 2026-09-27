#![allow(clippy::wildcard_imports)]
use super::runtime::{is_bndata_function, is_bool_vector, is_float_vector, is_string_vector};
use super::*;

#[allow(clippy::too_many_arguments, clippy::trivially_copy_pass_by_ref)]
pub(crate) fn call_instruction_supported(
    module: &Module,
    function: &Function,
    instruction: &Instruction,
    callee: &ValueId,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
    functions: &HashMap<ValueId, &str>,
    strings: &[(ValueId, String)],
    module_functions: &std::collections::HashSet<&str>,
) -> Result<bool, String> {
    Ok(match functions.get(callee).copied() {
        Some("$for_condition") => for_condition_supported(arguments, values),
        Some("HOST.Random.Seed") => arguments.len() == 1,
        Some("HOST.Random.Random") => arguments.is_empty(),
        Some(name) if is_bn_rt_host_call(name) => bn_rt_call_supported(name, arguments, values),
        Some(name)
            if name.ends_with(".Queue.Concurrent")
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
            arguments.iter().all(|argument| {
                values
                    .get(argument)
                    .is_some_and(|ty| llvm_type(ty).is_some())
            })
        }
        Some(name) if bnmath_method(module, name).is_some() => bnmath_call_supported(
            bnmath_method(module, name).unwrap_or(name),
            arguments,
            values,
        ),
        Some(name) if bnjson_member(module, name).is_some() => {
            let doc = |argument: &ValueId| {
                values
                    .get(argument)
                    .is_some_and(|ty| carries_bnjson(module, ty))
            };
            let string = |argument: &ValueId| values.get(argument) == Some(&Type::String);
            let integer = |argument: &ValueId| {
                values
                    .get(argument)
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
            };
            let float = |argument: &ValueId| {
                matches!(
                    values.get(argument),
                    Some(Type::Float(_) | Type::FloatLiteral)
                )
            };
            let boolean = |argument: &ValueId| values.get(argument) == Some(&Type::Boolean);
            match bnjson_member(module, name).unwrap_or(name) {
                "Object" | "Array" => arguments.is_empty(),
                "Kind" | "Length" | "Clone" | "AppendNull" | "Stringify" => {
                    arguments.len() == 1 && doc(&arguments[0])
                }
                "Parse" => arguments.len() == 1 && string(&arguments[0]),
                "Has" | "GetString" | "GetInteger" | "GetFloat" | "GetBoolean" | "GetJson"
                | "SetNull" => arguments.len() == 2 && doc(&arguments[0]) && string(&arguments[1]),
                "SetString" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && string(&arguments[1])
                        && string(&arguments[2])
                }
                "SetInteger" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && string(&arguments[1])
                        && integer(&arguments[2])
                }
                "SetFloat" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && string(&arguments[1])
                        && float(&arguments[2])
                }
                "SetBoolean" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && string(&arguments[1])
                        && boolean(&arguments[2])
                }
                "SetJson" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && string(&arguments[1])
                        && doc(&arguments[2])
                }
                "AppendString" => {
                    arguments.len() == 2 && doc(&arguments[0]) && string(&arguments[1])
                }
                "AppendInteger" => {
                    arguments.len() == 2 && doc(&arguments[0]) && integer(&arguments[1])
                }
                "AppendFloat" => arguments.len() == 2 && doc(&arguments[0]) && float(&arguments[1]),
                "AppendBoolean" => {
                    arguments.len() == 2 && doc(&arguments[0]) && boolean(&arguments[1])
                }
                "AppendJson" => arguments.len() == 2 && doc(&arguments[0]) && doc(&arguments[1]),
                "GetStringAt" | "GetIntegerAt" | "GetFloatAt" | "GetBooleanAt" | "GetJsonAt"
                | "SetNullAt" => {
                    arguments.len() == 2 && doc(&arguments[0]) && integer(&arguments[1])
                }
                "SetStringAt" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && integer(&arguments[1])
                        && string(&arguments[2])
                }
                "SetIntegerAt" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && integer(&arguments[1])
                        && integer(&arguments[2])
                }
                "SetFloatAt" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && integer(&arguments[1])
                        && float(&arguments[2])
                }
                "SetBooleanAt" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && integer(&arguments[1])
                        && boolean(&arguments[2])
                }
                "SetJsonAt" => {
                    arguments.len() == 3
                        && doc(&arguments[0])
                        && integer(&arguments[1])
                        && doc(&arguments[2])
                }
                _ => false,
            }
        }
        Some(name) if bncrypto_method(module, name).is_some() => {
            match bncrypto_method(module, name).unwrap_or(name) {
                "SealAesGcm" | "OpenAesGcm" | "SealChaCha20" | "OpenChaCha20" => {
                    arguments.len() == 4
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "Slice" => {
                    arguments.len() == 3
                        && values
                            .get(&arguments[0])
                            .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        && arguments[1..].iter().all(|argument| {
                            values
                                .get(argument)
                                .and_then(llvm_type)
                                .is_some_and(integer_llvm)
                        })
                }
                "MlKemKeypair" | "MlKemEncapsulate" | "MlDsaKeypair" => {
                    arguments.len() == 1
                        && values
                            .get(&arguments[0])
                            .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                }
                "MlKemDecapsulate" | "MlDsaSign" => {
                    arguments.len() == 2
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "MlDsaVerify" => {
                    arguments.len() == 3
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "Ed25519PublicKey" | "EcdsaP256PublicKey" => {
                    arguments.len() == 1
                        && values
                            .get(&arguments[0])
                            .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                }
                "Ed25519Sign" | "EcdsaP256Sign" => {
                    arguments.len() == 2
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "Ed25519Verify" | "EcdsaP256Verify" => {
                    arguments.len() == 3
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "HmacSha256" => {
                    arguments.len() == 2
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "VerifyHmacSha256" => {
                    arguments.len() == 3
                        && arguments.iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                }
                "Argon2id" => {
                    arguments.len() == 5
                        && arguments[..2].iter().all(|argument| {
                            values
                                .get(argument)
                                .is_some_and(|ty| carries_bncrypto_bytes(module, ty))
                        })
                        && arguments[2..].iter().all(|argument| {
                            values
                                .get(argument)
                                .and_then(llvm_type)
                                .is_some_and(integer_llvm)
                        })
                }
                method => {
                    arguments.len() == 1
                        && values.get(&arguments[0]).is_some_and(|ty| match method {
                            "Length" | "ToHex" => carries_bncrypto_bytes(module, ty),
                            _ => *ty == Type::String,
                        })
                }
            }
        }
        Some(name) if is_bndata_dataframe_call(module, name) => {
            matches!(
                bndata_dataframe_method(name),
                Some("constructor") if arguments.len() == 1
            ) || matches!(
                bndata_dataframe_method(name),
                Some("row_count" | "column_count") if arguments.len() == 1
            ) || matches!(
                bndata_dataframe_method(name),
                Some("add_integer_column") if arguments.len() == 3
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values.get(&arguments[2]).is_some_and(is_int_vector)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("add_string_column") if arguments.len() == 3
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values.get(&arguments[2]).is_some_and(is_string_vector)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("add_float_column") if arguments.len() == 3
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values.get(&arguments[2]).is_some_and(is_float_vector)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("add_boolean_column") if arguments.len() == 3
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values.get(&arguments[2]).is_some_and(is_bool_vector)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("column_name") if arguments.len() == 2
                    && values.get(&arguments[1]).and_then(llvm_type).is_some_and(integer_llvm)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("set_label") if arguments.len() == 3
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values.get(&arguments[2]) == Some(&Type::String)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("get_string" | "get_integer" | "get_float" | "get_boolean") if arguments.len() == 3
                    && values.get(&arguments[1]).and_then(llvm_type).is_some_and(integer_llvm)
                    && values.get(&arguments[2]).is_some_and(|ty| {
                        *ty == Type::String || llvm_type(ty) == Some("{ i1, ptr, i64 }")
                    })
            ) || matches!(
                bndata_dataframe_method(name),
                    Some("mean" | "median" | "quartile1" | "quartile3" | "mode" | "stdev" | "variance" | "range" | "min" | "max") if arguments.len() == 2
                    && values.get(&arguments[1]).is_some_and(|ty| {
                        *ty == Type::String || llvm_type(ty) == Some("{ i1, ptr, i64 }")
                    })
            ) || matches!(
                bndata_dataframe_method(name),
                Some("zscore") if arguments.len() == 2
                    && values.get(&arguments[1]).is_some_and(|ty| {
                        *ty == Type::String || llvm_type(ty) == Some("{ i1, ptr, i64 }")
                    })
            ) || matches!(
                bndata_dataframe_method(name),
                Some("copy_integer" | "copy_float") if arguments.len() == 3
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && matches!(values.get(&arguments[2]), Some(Type::Pointer { .. }))
            ) || matches!(
                bndata_dataframe_method(name),
                Some("select") if arguments.len() == 3
                    && matches!(values.get(&arguments[1]), Some(Type::Vector { .. }))
                    && matches!(values.get(&arguments[2]), Some(Type::Vector { .. }))
            ) || matches!(
                bndata_dataframe_method(name),
                Some("slice") if arguments.len() == 5
                    && arguments[1..].iter().all(|argument| values.get(argument).and_then(llvm_type).is_some_and(integer_llvm))
            ) || matches!(
                bndata_dataframe_method(name),
                Some("transpose") if arguments.len() == 1
            ) || matches!(
                bndata_dataframe_method(name),
                Some("append_rows" | "append_columns") if arguments.len() == 2
            ) || matches!(
                bndata_dataframe_method(name),
                Some("join" | "left_join" | "right_join" | "full_join") if arguments.len() == 4
                    && values.get(&arguments[2]) == Some(&Type::String)
                    && values.get(&arguments[3]) == Some(&Type::String)
            ) || matches!(
                bndata_dataframe_method(name),
                Some("convert_integer" | "convert_float") if arguments.len() == 2
                    && values.get(&arguments[1]) == Some(&Type::String)
            )
        }
        Some(name) if is_bndata_function(name) => {
            (name.ends_with("ReadCSV") && arguments.len() == 3)
                || (name.ends_with("WriteCSV") && arguments.len() == 4)
        }
        Some(name) if bnlog_method(module, name).is_some() => match bnlog_method(module, name) {
            Some("fields_constructor" | "logger_constructor") => arguments.len() == 1,
            Some("fields_set_string") => {
                arguments.len() == 3
                    && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Fields")
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values.get(&arguments[2]) == Some(&Type::String)
            }
            Some("logger_add_file") => {
                arguments.len() == 3
                    && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Logger")
                    && values.get(&arguments[1]) == Some(&Type::String)
                    && values
                        .get(&arguments[2])
                        .and_then(llvm_type)
                        .is_some_and(integer_llvm)
            }
            Some("logger_log") => {
                arguments.len() == 4
                    && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Logger")
                    && values
                        .get(&arguments[1])
                        .and_then(llvm_type)
                        .is_some_and(integer_llvm)
                    && values.get(&arguments[2]) == Some(&Type::String)
                    && bnlog_resource_kind(module, &values[&arguments[3]]) == Some("Fields")
            }
            Some("logger_flush" | "logger_close") => {
                arguments.len() == 2
                    && bnlog_resource_kind(module, &values[&arguments[0]]) == Some("Logger")
                    && values
                        .get(&arguments[1])
                        .and_then(llvm_type)
                        .is_some_and(integer_llvm)
            }
            _ => false,
        },
        Some("TimeZone.Parse") => {
            arguments.len() == 1
                && strings.iter().any(|(value, text)| {
                    Some(value) == arguments.first() && is_canonical_timezone(text)
                })
        }
        Some("ASC") => arguments.len() == 1 && values.get(&arguments[0]) == Some(&Type::String),
        Some("TOLOWER" | "TOUPPER") => {
            arguments.len() == 1 && values.get(&arguments[0]) == Some(&Type::String)
        }
        Some("CHAR") => {
            arguments.len() == 1
                && values
                    .get(&arguments[0])
                    .is_some_and(|ty| matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_)))
        }
        Some(name) if module_functions.contains(name.strip_prefix("@super:").unwrap_or(name)) => {
            arguments.iter().all(|argument| {
                values
                    .get(argument)
                    .is_some_and(|ty| llvm_type(ty).is_some() || is_void_type(ty))
            })
        }
        Some(name) => {
            return Err(unsupported_instruction(
                module,
                function,
                instruction,
                &unsupported_call_detail(module, name),
            ));
        }
        None => {
            matches!(values.get(callee), Some(Type::Function { parameters, .. })
                if parameters.len() == arguments.len()
                    && arguments.iter().all(|argument| values.get(argument).and_then(llvm_type).is_some()))
        }
    })
}
