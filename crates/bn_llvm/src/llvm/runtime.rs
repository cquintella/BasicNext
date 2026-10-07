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

pub(crate) const BN_RT_DECLS: &str = "\
declare i32 @bn_rt_policy_init(i32, i64)
declare void @bn_rt_policy_check(i32)
declare i32 @bn_rt_policy_filesystem_sandboxed()
declare i32 @bn_rt_policy_filesystem_root(i32, ptr)
declare i32 @bn_rt_exec_run(ptr, ptr, i32, ptr)
declare i64 @bn_rt_exec_result_return_code(i64)
declare ptr @bn_rt_exec_result_stdout(i64)
declare ptr @bn_rt_exec_result_stderr(i64)
declare i32 @bn_rt_exec_result_close(i64)
declare i32 @bn_rt_dataframe_create(ptr, i32, ptr)
declare i32 @bn_rt_dataframe_row_count(i64, ptr)
declare i32 @bn_rt_dataframe_column_count(i64, ptr)
declare i32 @bn_rt_dataframe_add_integer_start(i64, ptr, i32)
declare i32 @bn_rt_dataframe_set_integer_cell(i64, i32, i32, i64)
declare i32 @bn_rt_dataframe_add_float(i64, ptr, ptr, i32)
declare i32 @bn_rt_dataframe_add_boolean(i64, ptr, ptr, i32)
declare i32 @bn_rt_dataframe_add_string(i64, ptr, ptr, i32)
declare i32 @bn_rt_dataframe_get_integer(i64, i32, ptr, ptr, ptr)
declare i32 @bn_rt_dataframe_get_float(i64, i32, ptr, ptr, ptr)
declare i32 @bn_rt_dataframe_get_boolean(i64, i32, ptr, ptr, ptr)
declare i32 @bn_rt_dataframe_get_string(i64, i32, ptr, ptr, ptr)
declare i32 @bn_rt_dataframe_set_label(i64, ptr, ptr)
declare i32 @bn_rt_dataframe_reduce(i64, ptr, i32, ptr, ptr)
declare i32 @bn_rt_dataframe_zscore(i64, ptr, ptr)
declare i32 @bn_rt_dataframe_copy_integer(i64, ptr, ptr, i32)
declare i32 @bn_rt_dataframe_copy_float(i64, ptr, ptr, i32)
declare i32 @bn_rt_dataframe_select(i64, ptr, i32, ptr, i32, ptr)
declare i32 @bn_rt_dataframe_slice(i64, i32, i32, i32, i32, ptr)
declare i32 @bn_rt_dataframe_transpose(i64, ptr)
declare i32 @bn_rt_dataframe_append_rows(i64, i64, ptr)
declare i32 @bn_rt_dataframe_append_columns(i64, i64, ptr)
declare i32 @bn_rt_dataframe_join(i64, i64, ptr, ptr, i32, ptr)
declare i32 @bn_rt_dataframe_convert_integer(i64, ptr)
declare i32 @bn_rt_dataframe_convert_float(i64, ptr)
declare ptr @bn_rt_dataframe_column_name_owned(i64, i32)
declare i32 @bn_rt_dataframe_close(i64)
declare i32 @bn_rt_file_open(ptr, i32, ptr)
declare i32 @bn_rt_file_new(ptr)
declare i32 @bn_rt_file_close(i64)
declare i32 @bn_rt_file_release(i64)
declare i32 @bn_rt_file_read_all(i64, ptr)
declare i32 @bn_rt_file_read_line(i64, ptr)
declare i32 @bn_rt_file_write(i64, ptr)
declare i32 @bn_rt_file_write_line(i64, ptr)
declare i32 @bn_rt_file_read_bytes(i64, ptr, i64, ptr)
declare i32 @bn_rt_file_write_bytes(i64, ptr, i64)
declare i32 @bn_rt_fs_exists(ptr, ptr)
declare i32 @bn_rt_fs_delete_file(ptr)
declare i32 @bn_rt_dataframe_read_csv(i64, i8, ptr, ptr)
declare i32 @bn_rt_dataframe_write_csv(i64, i64, i8, ptr)
declare i64 @bn_rt_log_fields_create()
declare i64 @bn_rt_log_logger_create()
declare i32 @bn_rt_log_fields_set_string(i64, ptr, ptr)
declare i32 @bn_rt_log_logger_add_null(i64, i64)
declare i32 @bn_rt_log_logger_add_console(i64, i64)
declare i32 @bn_rt_log_logger_add_file(i64, ptr, i64)
declare i32 @bn_rt_log_logger_log(i64, i64, ptr, i64)
declare i32 @bn_rt_log_logger_flush(i64, i64)
declare i32 @bn_rt_log_logger_close(i64, i64)
declare i32 @bn_rt_log_fields_close(i64)
declare i32 @bn_rt_log_logger_delete(i64)
declare i64 @bn_rt_clock_now()
declare i64 @bn_rt_clock_timer()
declare i32 @bn_rt_random_seed(i64)
declare double @bn_rt_random_next()
declare i32 @bn_rt_console_cls()
declare i32 @bn_rt_console_beep()
declare i32 @bn_rt_console_print_at(i32, i32, ptr)
declare i32 @bn_rt_console_num_cols()
declare i32 @bn_rt_console_num_rows()
declare i64 @bn_rt_str_asc(ptr)
declare ptr @bn_rt_str_to_lower(ptr)
declare ptr @bn_rt_str_to_upper(ptr)
declare ptr @bn_rt_text_int(i64)
declare ptr @bn_rt_text_uint(i64)
declare ptr @bn_rt_text_float(double)
declare ptr @bn_rt_text_float32(double)
declare ptr @bn_rt_crypto_sha256(ptr)
declare ptr @bn_rt_crypto_sha512(ptr)
declare i64 @bn_rt_crypto_bytes_from_text(ptr)
declare i32 @bn_rt_crypto_bytes_from_hex(ptr, ptr)
declare i64 @bn_rt_crypto_bytes_length(i64)
declare ptr @bn_rt_crypto_bytes_to_hex(i64)
declare i32 @bn_rt_crypto_bytes_release(i64)
declare i32 @bn_rt_crypto_seal(i32, i64, i64, i64, i64, ptr)
declare i32 @bn_rt_crypto_open(i32, i64, i64, i64, i64, ptr)
declare i32 @bn_rt_crypto_hmac(i64, i64, ptr)
declare i32 @bn_rt_crypto_hmac_verify(i64, i64, i64)
declare i32 @bn_rt_crypto_argon2id(i64, i64, i64, i64, i64, ptr)
declare i32 @bn_rt_crypto_public_key(i32, i64, ptr)
declare i32 @bn_rt_crypto_sign(i32, i64, i64, ptr)
declare i32 @bn_rt_crypto_verify(i32, i64, i64, i64)
declare i32 @bn_rt_crypto_slice(i64, i64, i64, ptr)
declare i32 @bn_rt_crypto_kem_keypair(i64, ptr)
declare i32 @bn_rt_crypto_kem_encapsulate(i64, ptr)
declare i32 @bn_rt_crypto_kem_decapsulate(i64, i64, ptr)
declare i32 @bn_rt_crypto_dsa_keypair(i64, ptr)
declare i32 @bn_rt_crypto_dsa_sign(i64, i64, ptr)
declare i32 @bn_rt_crypto_dsa_verify(i64, i64, i64)
declare i32 @bn_rt_sqlite_open(ptr, ptr)
declare i32 @bn_rt_sqlite_open_read_only(ptr, ptr)
declare i32 @bn_rt_sqlite_open_existing(ptr, ptr)
declare i32 @bn_rt_sqlite_close(i64)
declare i32 @bn_rt_sqlite_exec(i64, ptr)
declare i32 @bn_rt_sqlite_query(i64, ptr, ptr)
declare i32 @bn_rt_sqlite_begin(i64)
declare i32 @bn_rt_sqlite_commit(i64)
declare i32 @bn_rt_sqlite_rollback(i64)
declare i32 @bn_rt_sqlite_changes(i64)
declare i64 @bn_rt_sqlite_last_insert_rowid(i64)
declare i64 @bn_rt_json_object()
declare i32 @bn_rt_json_parse(ptr, ptr)
declare ptr @bn_rt_json_stringify(i64, ptr)
declare i32 @bn_rt_json_set_string(i64, ptr, ptr)
declare ptr @bn_rt_json_get_string(i64, ptr, ptr)
declare i32 @bn_rt_json_release(i64)
declare i64 @bn_rt_json_array()
declare ptr @bn_rt_json_kind(i64)
declare i32 @bn_rt_json_has(i64, ptr)
declare i64 @bn_rt_json_length(i64)
declare i32 @bn_rt_json_set_integer(i64, ptr, i64)
declare i32 @bn_rt_json_set_boolean(i64, ptr, i32)
declare i32 @bn_rt_json_set_null(i64, ptr)
declare i64 @bn_rt_json_get_integer(i64, ptr, ptr)
declare i32 @bn_rt_json_get_boolean(i64, ptr, ptr)
declare i32 @bn_rt_json_set_float(i64, ptr, double)
declare double @bn_rt_json_get_float(i64, ptr, ptr)
declare i32 @bn_rt_json_set_json(i64, ptr, i64)
declare i32 @bn_rt_json_get_json(i64, ptr, ptr)
declare i32 @bn_rt_json_clone(i64, ptr)
declare i32 @bn_rt_json_append_string(i64, ptr)
declare i32 @bn_rt_json_append_integer(i64, i64)
declare i32 @bn_rt_json_append_float(i64, double)
declare i32 @bn_rt_json_append_boolean(i64, i32)
declare i32 @bn_rt_json_append_null(i64)
declare i32 @bn_rt_json_append_json(i64, i64)
declare ptr @bn_rt_json_get_string_at(i64, i64, ptr)
declare i64 @bn_rt_json_get_integer_at(i64, i64, ptr)
declare double @bn_rt_json_get_float_at(i64, i64, ptr)
declare i32 @bn_rt_json_get_boolean_at(i64, i64, ptr)
declare i32 @bn_rt_json_get_json_at(i64, i64, ptr)
declare i32 @bn_rt_json_set_string_at(i64, i64, ptr)
declare i32 @bn_rt_json_set_integer_at(i64, i64, i64)
declare i32 @bn_rt_json_set_float_at(i64, i64, double)
declare i32 @bn_rt_json_set_boolean_at(i64, i64, i32)
declare i32 @bn_rt_json_set_null_at(i64, i64)
declare i32 @bn_rt_json_set_json_at(i64, i64, i64)
declare i64 @bn_rt_str_char_utf8(i64)
declare i32 @bn_rt_net_address_parse(ptr, ptr)
declare i32 @bn_rt_net_ping(ptr, i32, ptr, ptr)
declare i32 @bn_rt_net_reverse(ptr, i32, ptr)
declare i32 @bn_rt_net_neighbor(ptr, ptr)
declare i32 @bn_rt_net_resolve(ptr, i32, ptr)
declare i32 @bn_rt_net_addresses_count(ptr)
declare i32 @bn_rt_net_addresses_get(ptr, i32, ptr)
declare void @bn_rt_net_addresses_free(ptr)
declare i32 @bn_rt_net_udp_bind(ptr, i32, ptr)
declare i32 @bn_rt_net_tcp_connect(ptr, i32, i32, ptr)
declare i32 @bn_rt_net_tcp_listen_with_backlog(ptr, i32, i32, ptr)
declare i32 @bn_rt_net_tcp_accept(i64, i32, ptr)
declare i32 @bn_rt_net_tcp_listener_local_endpoint(i64, ptr, ptr)
declare i32 @bn_rt_net_udp_local_endpoint(i64, ptr, ptr)
declare i32 @bn_rt_net_tcp_stream_local_endpoint(i64, ptr, ptr)
declare i32 @bn_rt_net_tcp_stream_remote_endpoint(i64, ptr, ptr)
declare i32 @bn_rt_net_tcp_write(i64, ptr, i32, ptr)
declare i32 @bn_rt_net_tcp_read(i64, ptr, i32, ptr)
declare i32 @bn_rt_net_handle_close(i64)
declare i32 @bn_rt_net_udp_send_to(i64, ptr, i32, ptr, i32, ptr)
declare i32 @bn_rt_net_udp_receive_handle(i64, i32, i32, ptr)
declare i32 @bn_rt_net_udp_packet_size(i64)
declare i32 @bn_rt_net_udp_packet_truncated(i64)
declare i32 @bn_rt_net_udp_packet_copy_to(i64, ptr, i32, ptr)
declare i32 @bn_rt_net_udp_packet_source(i64, ptr, ptr)
declare i32 @bn_rt_dispatch_queue_create(i64, ptr)
declare i32 @bn_rt_dispatch_queue_create_auto(ptr)
declare i32 @bn_rt_dispatch_submit(i64, ptr, ptr, ptr, i32, ptr)
declare i32 @bn_rt_dispatch_await(i64, i64, ptr, ptr)
declare i32 @bn_rt_dispatch_cancel(i64)
declare i32 @bn_rt_dispatch_ticket_close(i64)
declare i32 @bn_rt_dispatch_ticket_cancel(i64, ptr)
declare i64 @bn_rt_dispatch_ticket_id(i64)
declare i32 @bn_rt_dispatch_ticket_status(i64)
declare i32 @bn_rt_dispatch_ticket_is_done(i64)
declare i32 @bn_rt_dispatch_ticket_error(i64, ptr, ptr)
declare i32 @bn_rt_dispatch_queue_join(i64, i64)
declare i32 @bn_rt_dispatch_queue_close(i64, i64)
declare i32 @bn_rt_dispatch_group_create(ptr)
declare i32 @bn_rt_dispatch_group_enter(i64)
declare i32 @bn_rt_dispatch_group_leave(i64)
declare i32 @bn_rt_dispatch_group_wait(i64, i64)
declare i32 @bn_rt_dispatch_group_close(i64)
declare i32 @bn_rt_dispatch_barrier_create(i64, ptr)
declare i32 @bn_rt_dispatch_barrier_wait(i64, i64, ptr)
declare i32 @bn_rt_dispatch_barrier_close(i64)
declare i32 @bn_rt_dispatch_semaphore_create(i64, ptr)
declare i32 @bn_rt_dispatch_semaphore_acquire(i64, i64)
declare i32 @bn_rt_dispatch_semaphore_release(i64)
declare i32 @bn_rt_dispatch_semaphore_close(i64)
declare i32 @bn_rt_dispatch_mutex_create(ptr)
declare i32 @bn_rt_dispatch_mutex_lock(i64, i64)
declare i32 @bn_rt_dispatch_mutex_unlock(i64)
declare i32 @bn_rt_dispatch_mutex_close(i64)
declare i32 @bn_rt_host_num_procs(ptr)
";

pub(crate) fn is_bn_rt_host_call(name: &str) -> bool {
    FS_CALLS.contains(&name)
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
                | "HOST.Net.UDPPacket.Truncated"
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
    // Provider-backed stubs have no lowered body, so the constructor callee
    // is identified by its documented name shape (`bn_ir::names`); the
    // remaining members are library names (host/library contract).
    if bn_ir::names::classify(name) == Some(bn_ir::names::EmittedNameKind::Constructor) {
        return match rest.split_once('.')?.0 {
            "Fields" => Some("fields_constructor"),
            "Logger" => Some("logger_constructor"),
            _ => None,
        };
    }
    match rest {
        "Fields.SetString" => Some("fields_set_string"),
        "Logger.AddNull" => Some("logger_add_null"),
        "Logger.AddConsole" => Some("logger_add_console"),
        "Logger.AddFile" => Some("logger_add_file"),
        "Logger.Log" => Some("logger_log"),
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
    match name {
        "HOST.NumProcs"
        | "HOST.Clock.Now"
        | "HOST.Clock.Timer"
        | "HOST.Console.Cls"
        | "HOST.Console.Beep"
        | "HOST.Console.NumCols"
        | "HOST.Console.NumRows" => arguments.is_empty(),
        "HOST.Exec.Run" => {
            arguments.len() == 2
                && values.get(&arguments[0]) == Some(&Type::String)
                && values.get(&arguments[1]).and_then(llvm_type) == Some("{ ptr, i32 }")
        }
        "HOST.Exec.Result.ReturnCode" | "HOST.Exec.Result.Stdout" | "HOST.Exec.Result.Stderr" => {
            arguments.len() == 1
                && values.get(&arguments[0]).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
        }
        "HOST.Exec.Result.Close" => {
            arguments.len() == 1
                && values.get(&arguments[0]).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
        }
        "HOST.Console.PrintAt" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .and_then(llvm_type)
                        .is_some_and(integer_llvm)
                })
                && arguments.get(1).is_some_and(|value| {
                    values
                        .get(value)
                        .and_then(llvm_type)
                        .is_some_and(integer_llvm)
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
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.Endpoint.Port" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    matches!(
                        values.get(value).and_then(llvm_type),
                        Some("{ i1, ptr, i32 }" | "{ ptr, i32 }")
                    )
                })
        }
        "HOST.Net.Endpoint.Address" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    matches!(
                        values.get(value).and_then(llvm_type),
                        Some("{ i1, ptr, i32 }" | "{ ptr, i32 }")
                    )
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
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.TCPListener.Accept" => {
            arguments.len() == 2
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ i1, ptr, i64 }")
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.TCPListener.LocalEndpoint" | "HOST.Net.UDPSocket.LocalEndpoint" => {
            arguments.len() == 1
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ i1, ptr, i64 }")
        }
        "HOST.Net.TCPStream.Write" => {
            arguments.len() == 3
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ i1, ptr, i64 }")
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ ptr, i32 }")
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.TCPStream.Read" => {
            arguments.len() == 3
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ i1, ptr, i64 }")
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ ptr, i32 }")
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.TCPStream.LocalEndpoint" | "HOST.Net.TCPStream.RemoteEndpoint" => {
            arguments.len() == 1
                && arguments
                    .first()
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    == Some("{ i1, ptr, i64 }")
        }
        "HOST.Net.TCPStream.Close" | "HOST.Net.TCPListener.Close" | "HOST.Net.UDPSocket.Close" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
        }
        "HOST.Net.UDPSocket.SendTo" => {
            arguments.len() == 4
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
                && arguments.get(1).is_some_and(|value| {
                    matches!(
                        values.get(value).and_then(llvm_type),
                        Some("{ ptr, i32 }" | "{ i1, ptr, i32 }")
                    )
                })
                && arguments.get(2).is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ ptr, i32 }")
                })
                && arguments.get(3).is_some_and(|value| {
                    matches!(values.get(value).and_then(llvm_type), Some("i32" | "i64"))
                })
        }
        "HOST.Net.UDPSocket.Receive" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.UDPPacket.Size"
        | "HOST.Net.UDPPacket.Truncated"
        | "HOST.Net.UDPPacket.WasTruncated" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
        }
        "HOST.Net.UDPPacket.CopyTo" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
                && arguments.get(1).is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ ptr, i32 }")
                })
                && arguments
                    .get(2)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.UDPPacket.Source" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
        }
        "HOST.Net.Ping" | "HOST.Net.Reverse" => {
            arguments.len() == 2
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .is_some_and(|ty| llvm_type(ty) == Some("{ i1, ptr, i64 }"))
                })
                && arguments.get(1).is_some_and(|value| {
                    values
                        .get(value)
                        .and_then(llvm_type)
                        .is_some_and(integer_llvm)
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
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.Addresses.Count" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr }")
                })
        }
        "HOST.Net.Addresses.Get" => {
            arguments.len() == 2
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr }")
                })
                && arguments
                    .get(1)
                    .and_then(|value| values.get(value))
                    .and_then(llvm_type)
                    .is_some_and(integer_llvm)
        }
        "HOST.Net.Neighbor"
        | "HOST.Net.PingReply.RoundTripMicroseconds"
        | "HOST.Net.PingReply.Address" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values
                        .get(value)
                        .is_some_and(|ty| llvm_type(ty) == Some("{ i1, ptr, i64 }"))
                })
        }
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_bn_rt_call(
    text: &mut String,
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
            matches!(
                values.get(value).and_then(llvm_type),
                Some("{ ptr, i32 }" | "{ i1, ptr, i32 }")
            )
        })
}

/// A `bn_rt` call whose status 0 is success; otherwise the failure it
/// recorded is printed with this site's texts for `failures`, and the
/// program leaves through `trap_bn_rt`.
pub(crate) fn emit_checked_i32_eq_zero(
    text: &mut String,
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
    text: &mut String,
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
        &format!("%bnrtfail{dest}"),
        ok,
        failures,
    );
}

pub(crate) fn extend_to_i32(text: &mut String, value: ValueId, ty: &Type) -> String {
    let llvm_ty = llvm_type(ty).unwrap_or("i32");
    match llvm_ty {
        "i32" => format!("%v{}", value.0),
        llvm_ty => {
            let op = match llvm_ty {
                "i64" => CastOp::Trunc,
                _ if is_unsigned(ty) => CastOp::ZExt,
                _ => CastOp::SExt,
            };
            let from = crate::layout::typed_llvm(llvm_ty);
            let temp = format!("bnrti32{}", value.0);
            text.assign(
                temp.clone(),
                LlvmInst::cast(op, from, value_reg(value), LlvmType::I32),
            );
            format!("%{temp}")
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
