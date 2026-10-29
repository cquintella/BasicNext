// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The `bn_rt` C ABI declarations, support checks for HOST and library calls,
// the HOST call dispatcher (domain lowering lives in `runtime_net.rs` and
// `runtime_process.rs`), and the shared status-check helpers.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand, LlvmType};

pub(crate) fn is_bn_rt_host_call(name: &str) -> bool {
    FS_CALLS.contains(&name)
        || DirectoryCall::of(name).is_some()
        || crate::runtime_net::NetMember::of(name).is_some()
        || matches!(
            name,
            "HOST.NumProcs"
                | "HOST.Clock.Now"
                | "HOST.Clock.Timer"
                | "HOST.Console.Cls"
                | "HOST.Console.Beep"
                | "HOST.Console.PrintAt"
                | "HOST.Console.NumCols"
                | "HOST.Console.NumRows"
                | "HOST.Env.Get"
                | "HOST.Env.Has"
                | "HOST.Exec.Run"
                | "HOST.Exec.Result.ReturnCode"
                | "HOST.Exec.Result.Stdout"
                | "HOST.Exec.Result.Stderr"
                | "HOST.Exec.Result.Close"
                | "HOST.Net.Address.Parse"
                | "HOST.Net.Address.ToString"
                | "HOST.Net.Endpoint.Create"
                | "HOST.Net.Endpoint.Port"
                | "HOST.Net.Endpoint.Address"
                | "HOST.Net.UDPBind"
                | "HOST.Net.TCPConnect"
                | "HOST.Net.TCPListen"
                | "HOST.Net.TCPListener.Accept"
                | "HOST.Net.TCPListener.LocalEndpoint"
                | "HOST.Net.TCPStream.Write"
                | "HOST.Net.TCPStream.Read"
                | "HOST.Net.TCPStream.LocalEndpoint"
                | "HOST.Net.TCPStream.RemoteEndpoint"
                | "HOST.Net.TCPStream.Close"
                | "HOST.Net.TCPListener.Close"
                | "HOST.Net.UDPSocket.Close"
                | "HOST.Net.UDPSocket.LocalEndpoint"
                | "HOST.Net.UDPSocket.SendTo"
                | "HOST.Net.UDPSocket.Receive"
                | "HOST.Net.UDPPacket.Size"
                | "HOST.Net.UDPPacket.WasTruncated"
                | "HOST.Net.UDPPacket.CopyTo"
                | "HOST.Net.UDPPacket.Source"
                | "HOST.Net.Ping"
                | "HOST.Net.Reverse"
                | "HOST.Net.Neighbor"
                | "HOST.Net.Resolve"
                | "HOST.Net.Addresses.Count"
                | "HOST.Net.Addresses.Get"
                | "HOST.Net.PingReply.RoundTripMicroseconds"
                | "HOST.Net.PingReply.Address"
        )
}

pub(crate) fn is_bndata_dataframe_call(_module: &Module, name: &str) -> bool {
    let Some(rest) = name
        .strip_prefix('#')
        .and_then(|value| value.split_once('.').map(|(_, rest)| rest))
    else {
        return false;
    };
    // Imported standard-module functions are lowered with the caller's
    // canonical function prefix in some module-graph paths.  The provider
    // set is the authority that BNData is imported; the numeric prefix is
    // only a symbol identity and may differ after graph normalization.
    // `DataFrame` is a reserved standard-provider class name; no user class
    // can declare a colliding imported provider symbol.
    matches!(
        rest.strip_prefix("DataFrame."),
        Some(
            "CONSTRUCTOR"
                | "RowCount"
                | "ColumnCount"
                | "AddStringColumn"
                | "AddIntegerColumn"
                | "AddFloatColumn"
                | "AddBooleanColumn"
                | "ColumnName"
                | "SetLabel"
                | "GetString"
                | "GetInteger"
                | "GetFloat"
                | "GetBoolean"
                | "Mean"
                | "Median"
                | "Quartile1"
                | "Quartile3"
                | "Mode"
                | "Stdev"
                | "Variance"
                | "Range"
                | "Min"
                | "Max"
                | "ZScore"
                | "CopyIntegerColumn"
                | "CopyFloatColumn"
                | "Select"
                | "Slice"
                | "Transpose"
                | "AppendRows"
                | "AppendColumns"
                | "Join"
                | "LeftJoin"
                | "RightJoin"
                | "FullJoin"
                | "ConvertToInteger"
                | "ConvertToFloat"
                | "CountNA"
                | "DropNA"
                | "FillNA"
        )
    )
}

pub(crate) fn is_bndata_function(name: &str) -> bool {
    matches!(name.rsplit('.').next(), Some("ReadCSV" | "WriteCSV"))
}

pub(crate) fn bnlog_method(module: &Module, name: &str) -> Option<&'static str> {
    let (module_id, rest) = name.strip_prefix('#')?.split_once('.')?;
    let module_id = bn_ir::ModuleId(module_id.parse().ok()?);
    if !module.bnlog_providers.contains(&module_id) {
        return None;
    }
    // Provider-backed members have no lowered body; they are named by the
    // library contract (bnlog.md, "API (0.6.5)").
    if let Some(method) = crate::log_emission::level_method(rest) {
        return Some(method);
    }
    match rest {
        "Logger.New" => Some("logger_new"),
        "Logger.Access" => Some("logger_access"),
        "Logger.SetLevel" => Some("logger_set_level"),
        "Logger.SetDefaults" => Some("logger_set_defaults"),
        "Logger.SetAccessDefaults" => Some("logger_set_access_defaults"),
        "Logger.Flush" => Some("logger_flush"),
        "Logger.Close" => Some("logger_close"),
        _ => None,
    }
}

pub(crate) fn bndata_dataframe_method(name: &str) -> Option<&'static str> {
    match name.rsplit('.').next()? {
        "CONSTRUCTOR" => Some("constructor"),
        "RowCount" => Some("row_count"),
        "ColumnCount" => Some("column_count"),
        "AddIntegerColumn" => Some("add_integer_column"),
        "AddStringColumn" => Some("add_string_column"),
        "AddFloatColumn" => Some("add_float_column"),
        "AddBooleanColumn" => Some("add_boolean_column"),
        "ColumnName" => Some("column_name"),
        "SetLabel" => Some("set_label"),
        "GetString" => Some("get_string"),
        "GetInteger" => Some("get_integer"),
        "GetFloat" => Some("get_float"),
        "GetBoolean" => Some("get_boolean"),
        "Mean" => Some("mean"),
        "Median" => Some("median"),
        "Quartile1" => Some("quartile1"),
        "Quartile3" => Some("quartile3"),
        "Mode" => Some("mode"),
        "Stdev" => Some("stdev"),
        "Variance" => Some("variance"),
        "Range" => Some("range"),
        "Min" => Some("min"),
        "Max" => Some("max"),
        "ZScore" => Some("zscore"),
        "CopyIntegerColumn" => Some("copy_integer"),
        "CopyFloatColumn" => Some("copy_float"),
        "Select" => Some("select"),
        "Slice" => Some("slice"),
        "Transpose" => Some("transpose"),
        "AppendRows" => Some("append_rows"),
        "AppendColumns" => Some("append_columns"),
        "Join" => Some("join"),
        "LeftJoin" => Some("left_join"),
        "RightJoin" => Some("right_join"),
        "FullJoin" => Some("full_join"),
        "ConvertToInteger" => Some("convert_integer"),
        "ConvertToFloat" => Some("convert_float"),
        "CountNA" => Some("count_na"),
        "DropNA" => Some("drop_na"),
        "FillNA" => Some("fill_na"),
        _ => None,
    }
}

pub(crate) fn is_float_vector(ty: &Type) -> bool {
    matches!(ty, Type::Vector { element, dimensions } if dimensions.len() == 1 && matches!(element.as_ref(), Type::Float(_)))
}

pub(crate) fn is_bool_vector(ty: &Type) -> bool {
    matches!(ty, Type::Vector { element, dimensions } if dimensions.len() == 1 && **element == Type::Boolean)
}

pub(crate) fn is_string_vector(ty: &Type) -> bool {
    matches!(ty, Type::Vector { element, dimensions } if dimensions.len() == 1 && **element == Type::String)
}

pub(crate) fn bn_rt_call_supported(
    name: &str,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> bool {
    if let Some(supported) = fs_call_supported(name, arguments, values) {
        return supported;
    }
    if let Some(member) = crate::runtime_net::NetMember::of(name) {
        return member.arguments_supported(arguments, values);
    }
    match name {
        "HOST.NumProcs"
        | "HOST.Clock.Now"
        | "HOST.Clock.Timer"
        | "HOST.Console.Cls"
        | "HOST.Console.Beep"
        | "HOST.Console.NumCols"
        | "HOST.Console.NumRows" => arguments.is_empty(),
        "HOST.Env.Get" | "HOST.Env.Has" => {
            arguments.len() == 1 && values.get(&arguments[0]) == Some(&Type::String)
        }
        "HOST.Exec.Run" => {
            arguments.len() == 2
                && values.get(&arguments[0]) == Some(&Type::String)
                && values.get(&arguments[1]).and_then(llvm_type) == Some(crate::layout::vector_ty())
        }
        "HOST.Exec.Result.ReturnCode" | "HOST.Exec.Result.Stdout" | "HOST.Exec.Result.Stderr" => {
            arguments.len() == 1
                && values.get(&arguments[0]).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
        }
        "HOST.Exec.Result.Close" => {
            arguments.len() == 1
                && values.get(&arguments[0]).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
        }
        "HOST.Console.PrintAt" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .and_then(llvm_type)
                        .is_some_and(|ty| ty.is_integer())
                })
                && arguments.get(1).is_some_and(|value| {
                    values
                        .get(value)
                        .and_then(llvm_type)
                        .is_some_and(|ty| ty.is_integer())
                })
                && arguments
                    .get(2)
                    .is_some_and(|value| values.get(value) == Some(&Type::String))
        }
        "HOST.Net.Address.Parse" => {
            arguments.len() == 1
                && arguments
                    .first()
                    .is_some_and(|value| values.get(value) == Some(&Type::String))
        }
        "HOST.Net.Address.ToString" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .is_some_and(|ty| {
                            is_net_address_type(ty)
                                || matches!(ty, Type::Alternative(alternatives) if alternatives.iter().any(is_net_address_type) && alternatives.iter().any(is_error_type))
                        })
                })
        }
        "HOST.Net.Endpoint.Create" => {
            arguments.len() == 2
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.Endpoint.Port" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type).is_some_and(|ty| crate::layout::is_one_of(&ty, &[crate::layout::status_endpoint_ty(), crate::layout::vector_ty()]))
                })
        }
        "HOST.Net.Endpoint.Address" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type).is_some_and(|ty| crate::layout::is_one_of(&ty, &[crate::layout::status_endpoint_ty(), crate::layout::vector_ty()]))
                })
        }
        "HOST.Net.UDPBind" => endpoint_net_call_supported(arguments, values, 1),
        "HOST.Net.TCPConnect" => endpoint_net_call_supported(arguments, values, 2),
        "HOST.Net.TCPListen" => {
            // Native listeners hold one socket: a set of one endpoint.
            arguments.len() == 2
                && arguments.first().and_then(|value| values.get(value)).is_some_and(
                    |ty| matches!(ty, Type::Vector { dimensions, .. } if dimensions.as_slice() == [1]),
                )
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.TCPListener.Accept" => {
            arguments.len() == 2
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::handle_result_ty())
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.TCPListener.LocalEndpoint" | "HOST.Net.UDPSocket.LocalEndpoint" => {
            arguments.len() == 1
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::handle_result_ty())
        }
        "HOST.Net.TCPStream.Write" => {
            arguments.len() == 3
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::handle_result_ty())
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::vector_ty())
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.TCPStream.Read" => {
            arguments.len() == 3
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::handle_result_ty())
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::vector_ty())
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.TCPStream.LocalEndpoint" | "HOST.Net.TCPStream.RemoteEndpoint" => {
            arguments.len() == 1
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some(crate::layout::handle_result_ty())
        }
        "HOST.Net.TCPStream.Close" | "HOST.Net.TCPListener.Close" | "HOST.Net.UDPSocket.Close" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
        }
        "HOST.Net.UDPSocket.SendTo" => {
            arguments.len() == 4
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
                && arguments.get(1).is_some_and(|value| {
                    values.get(value).and_then(llvm_type).is_some_and(|ty| crate::layout::is_one_of(&ty, &[crate::layout::vector_ty(), crate::layout::status_endpoint_ty()]))
                })
                && arguments.get(2).is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::vector_ty())
                })
                && arguments.get(3).is_some_and(|value| {
                    matches!(values.get(value).and_then(llvm_type), Some(LlvmType::I32 | LlvmType::I64))
                })
        }
        "HOST.Net.UDPSocket.Receive" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.UDPPacket.Size"
        | "HOST.Net.UDPPacket.WasTruncated" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
        }
        "HOST.Net.UDPPacket.CopyTo" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
                && arguments.get(1).is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::vector_ty())
                })
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.UDPPacket.Source" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::handle_result_ty())
                })
        }
        "HOST.Net.Ping" | "HOST.Net.Reverse" => {
            arguments.len() == 2
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .is_some_and(|ty| llvm_type(ty) == Some(crate::layout::handle_result_ty()))
                })
                && arguments.get(1).is_some_and(|value| {
                    values
                        .get(value)
                        .and_then(llvm_type)
                        .is_some_and(|ty| ty.is_integer())
                })
        }
        "HOST.Net.Resolve" => {
            arguments.len() == 2
                && arguments
                    .first()
                    .is_some_and(|value| values.get(value) == Some(&Type::String))
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.Addresses.Count" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::status_pointer_ty())
                })
        }
        "HOST.Net.Addresses.Get" => {
            arguments.len() == 2
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some(crate::layout::status_pointer_ty())
                })
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(|ty| ty.is_integer())
        }
        "HOST.Net.Neighbor"
        | "HOST.Net.PingReply.RoundTripMicroseconds"
        | "HOST.Net.PingReply.Address" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .is_some_and(|ty| llvm_type(ty) == Some(crate::layout::handle_result_ty()))
                })
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_bn_rt_call(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    if lower_fs_call(
        text,
        block_id,
        destination,
        name,
        arguments,
        analysis,
        state,
    ) {
        return;
    }
    if name.starts_with("HOST.Net.") {
        lower_net_call(
            text,
            block_id,
            destination,
            name,
            arguments,
            analysis,
            state,
        );
    } else {
        lower_process_call(
            text,
            block_id,
            destination,
            name,
            arguments,
            analysis,
            state,
        );
    }
}

fn endpoint_net_call_supported(
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
    count: usize,
) -> bool {
    arguments.len() == count
        && arguments.first().is_some_and(|value| {
            values.get(value).and_then(llvm_type).is_some_and(|ty| {
                crate::layout::is_one_of(
                    &ty,
                    &[
                        crate::layout::vector_ty(),
                        crate::layout::status_endpoint_ty(),
                    ],
                )
            })
        })
}

/// A `bn_rt` call whose status 0 is success; otherwise the failure it
/// recorded is printed with this site's texts for `failures`, and the
/// program leaves through `trap_bn_rt`.
pub(crate) fn emit_checked_i32_eq_zero(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    call: LlvmInst,
    failures: &[bn_diag::DiagId],
    state: &mut EmissionState,
) {
    let status = format!("bnrtrc{}", destination.0);
    emit_checked_i32(
        text,
        block_id,
        destination,
        call,
        &status,
        ICmpCond::Ne,
        failures,
        state,
    );
}

/// Writes `%<status> = call` and traps through `failures` when
/// `status <fail_when> 0` holds (`ne`: nonzero status; `slt`: negative count).
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_checked_i32(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    call: LlvmInst,
    status: &str,
    fail_when: ICmpCond,
    failures: &[bn_diag::DiagId],
    state: &mut EmissionState,
) {
    let dest = destination.0;
    text.assign(status, call);
    text.assign(
        format!("bnrtfail{dest}"),
        LlvmInst::icmp(
            fail_when,
            LlvmType::I32,
            LlvmOperand::reg(status),
            LlvmOperand::int(0),
        ),
    );
    let ok = take_continuation(block_id, state);
    emit_failure_trap(
        text,
        block_id,
        state,
        &LlvmOperand::reg(format!("bnrtfail{dest}")),
        ok,
        failures,
    );
}

pub(crate) fn extend_to_i32(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> crate::ir::LlvmOperand {
    let llvm_ty = llvm_type(ty).unwrap_or(LlvmType::I32);
    match llvm_ty {
        LlvmType::I32 => LlvmOperand::reg(format!("v{}", value.0)),
        llvm_ty => {
            let op = match llvm_ty {
                LlvmType::I64 => CastOp::Trunc,
                _ if is_unsigned(ty) => CastOp::ZExt,
                _ => CastOp::SExt,
            };
            let from = llvm_ty;
            let temp = format!("bnrti32{}", value.0);
            text.assign(
                temp.clone(),
                LlvmInst::cast(op, from, value_reg(value), LlvmType::I32),
            );
            LlvmOperand::reg(&temp)
        }
    }
}

pub(crate) fn take_continuation(block_id: BlockId, state: &mut EmissionState) -> String {
    let name = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    name
}

/// `%v<id>`: the register that holds a BN IR value.
pub(crate) fn value_reg(value: ValueId) -> LlvmOperand {
    LlvmOperand::reg(format!("v{}", value.0))
}
