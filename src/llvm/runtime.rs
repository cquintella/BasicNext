#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) const BN_RT_DECLS: &str = "\
declare i64 @bn_rt_clock_timestamp()
declare i64 @bn_rt_clock_monotonic()
declare i32 @bn_rt_console_cls()
declare i32 @bn_rt_console_beep()
declare i32 @bn_rt_console_print_at(i32, i32, ptr)
declare i32 @bn_rt_console_num_cols()
declare i32 @bn_rt_console_num_rows()
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
declare i32 @bn_rt_net_tcp_listen(ptr, i32, ptr)
declare i32 @bn_rt_net_tcp_listen_with_backlog(ptr, i32, i32, ptr)
declare i32 @bn_rt_net_tcp_accept(i64, i32, ptr)
declare i32 @bn_rt_net_tcp_listener_local_endpoint(i64, ptr, ptr)
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
";

pub(crate) fn is_bn_rt_host_call(name: &str) -> bool {
    matches!(
        name,
        "HOST.Clock.Timestamp"
            | "HOST.Clock.Monotonic"
            | "HOST.Console.Cls"
            | "HOST.Console.Beep"
            | "HOST.Console.PrintAt"
            | "HOST.Console.NumCols"
            | "HOST.Console.NumRows"
            | "HOST.Net.Address.Parse"
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

pub(crate) fn bn_rt_call_supported(
    name: &str,
    arguments: &[ValueId],
    values: &HashMap<ValueId, Type>,
) -> bool {
    match name {
        "HOST.Clock.Timestamp"
        | "HOST.Clock.Monotonic"
        | "HOST.Console.Cls"
        | "HOST.Console.Beep"
        | "HOST.Console.NumCols"
        | "HOST.Console.NumRows" => arguments.is_empty(),
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
            arguments.len() == 2
                && arguments.first().and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ ptr, i32 }")
                && arguments.get(1).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
        }
        "HOST.Net.TCPListener.Accept" => {
            arguments.len() == 2
                && arguments.first().and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                && arguments.get(1).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
        }
        "HOST.Net.TCPListener.LocalEndpoint" => {
            arguments.len() == 1
                && arguments.first().and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
        }
        "HOST.Net.TCPStream.Write" => {
            arguments.len() == 3
                && arguments.first().and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                && arguments.get(1).and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ ptr, i32 }")
                && arguments.get(2).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
        }
        "HOST.Net.TCPStream.Read" => {
            arguments.len() == 3
                && arguments.first().and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                && arguments.get(1).and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ ptr, i32 }")
                && arguments.get(2).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
        }
        "HOST.Net.TCPStream.LocalEndpoint" | "HOST.Net.TCPStream.RemoteEndpoint" => {
            arguments.len() == 1
                && arguments.first().and_then(|value| values.get(value)).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
        }
        "HOST.Net.TCPStream.Close" | "HOST.Net.TCPListener.Close" | "HOST.Net.UDPSocket.Close" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| {
                    values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }")
                })
        }
        "HOST.Net.UDPSocket.SendTo" => {
            arguments.len() == 4
                && arguments.first().is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }"))
                && arguments.get(1).is_some_and(|value| matches!(values.get(value).and_then(llvm_type), Some("{ ptr, i32 }" | "{ i1, ptr, i32 }")))
                && arguments.get(2).is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ ptr, i32 }"))
                && arguments.get(3).is_some_and(|value| matches!(values.get(value).and_then(llvm_type), Some("i32" | "i64")))
        }
        "HOST.Net.UDPSocket.Receive" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }"))
                && arguments.get(1).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
                && arguments.get(2).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
        }
        "HOST.Net.UDPPacket.Size" | "HOST.Net.UDPPacket.Truncated" | "HOST.Net.UDPPacket.WasTruncated" => {
            arguments.len() == 1 && arguments.first().is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }"))
        }
        "HOST.Net.UDPPacket.CopyTo" => {
            arguments.len() == 3
                && arguments.first().is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }"))
                && arguments.get(1).is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ ptr, i32 }"))
                && arguments.get(2).and_then(|value| values.get(value)).and_then(llvm_type).is_some_and(integer_llvm)
        }
        "HOST.Net.UDPPacket.Source" => {
            arguments.len() == 1
                && arguments.first().is_some_and(|value| values.get(value).and_then(llvm_type) == Some("{ i1, ptr, i64 }"))
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
    match name {
        "HOST.Clock.Timestamp" => {
            let _ = writeln!(
                text,
                "  %v{} = call i64 @bn_rt_clock_timestamp()",
                destination.0
            );
        }
        "HOST.Net.UDPSocket.SendTo" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let (address, port) = endpoint_parts(text, arguments[1], analysis);
            let byte_ty = analysis.values.get(&arguments[2]).and_then(llvm_type).unwrap_or("{ ptr, i32 }");
            let bytes = format!("%netbytes{dest}");
            let length = format!("%netlen{dest}");
            if byte_ty == "{ ptr, i32 }" {
                let _ = writeln!(text, "  {bytes} = extractvalue {{ ptr, i32 }} %v{}, 0", arguments[2].0);
            }
            let len = extend_to_i32(text, arguments[3], analysis.values.get(&arguments[3]).expect("validated length"));
            let _ = writeln!(text, "  {length} = add i32 {len}, 0");
            let _ = writeln!(text, "  %netwritten{dest} = alloca i32");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_udp_send_to(i64 %nethandle{dest}, ptr {address}, i32 {port}, ptr {bytes}, i32 {length}, ptr %netwritten{dest})");
            let _ = writeln!(text, "  %netwrittenv{dest} = load i32, ptr %netwritten{dest}");
            let _ = writeln!(text, "  %netwritten64{dest} = sext i32 %netwrittenv{dest} to i64");
            emit_handle_result(text, destination, format!("%netrc{dest}"), format!("%netwritten64{dest}"));
        }
        "HOST.Net.TCPStream.Read" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let _ = writeln!(text, "  %netbuffer{dest} = extractvalue {{ ptr, i32 }} %v{}, 0", arguments[1].0);
            let length = extend_to_i32(text, arguments[2], analysis.values.get(&arguments[2]).expect("validated length"));
            let _ = writeln!(text, "  %netout{dest} = alloca i32");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_tcp_read(i64 %nethandle{dest}, ptr %netbuffer{dest}, i32 {length}, ptr %netout{dest})");
            let _ = writeln!(text, "  %netread{dest} = load i32, ptr %netout{dest}");
            let _ = writeln!(text, "  %netread64{dest} = sext i32 %netread{dest} to i64");
            let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 %netrc{dest}, 0");
            let _ = writeln!(text, "  %neteof{dest} = icmp eq i32 %netread{dest}, 0");
            let _ = writeln!(text, "  %neteofptr{dest} = getelementptr [4 x i8], ptr @.bn_eof, i64 0, i64 0");
            let _ = writeln!(text, "  %nettagptr{dest} = select i1 %neteof{dest}, ptr %neteofptr{dest}, ptr null");
            let _ = writeln!(text, "  %netagg{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %neterr{dest}, 0");
            let _ = writeln!(text, "  %netaggp{dest} = insertvalue {{ i1, ptr, i64 }} %netagg{dest}, ptr %nettagptr{dest}, 1");
            let _ = writeln!(text, "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netaggp{dest}, i64 %netread64{dest}, 2");
        }
        "HOST.Net.TCPStream.LocalEndpoint" | "HOST.Net.TCPStream.RemoteEndpoint" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let _ = writeln!(text, "  %netaddress{dest} = alloca ptr");
            let _ = writeln!(text, "  %netport{dest} = alloca i32");
            let function = if name.ends_with("LocalEndpoint") { "bn_rt_net_tcp_stream_local_endpoint" } else { "bn_rt_net_tcp_stream_remote_endpoint" };
            let _ = writeln!(text, "  %netrc{dest} = call i32 @{function}(i64 %nethandle{dest}, ptr %netaddress{dest}, ptr %netport{dest})");
            let _ = writeln!(text, "  %netaddrv{dest} = load ptr, ptr %netaddress{dest}");
            let _ = writeln!(text, "  %netportv{dest} = load i32, ptr %netport{dest}");
            let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 %netrc{dest}, 0");
            let _ = writeln!(text, "  %netep0{dest} = insertvalue {{ i1, ptr, i32 }} undef, i1 %neterr{dest}, 0");
            let _ = writeln!(text, "  %netep1{dest} = insertvalue {{ i1, ptr, i32 }} %netep0{dest}, ptr %netaddrv{dest}, 1");
            let _ = writeln!(text, "  %v{dest} = insertvalue {{ i1, ptr, i32 }} %netep1{dest}, i32 %netportv{dest}, 2");
        }
        "HOST.Net.UDPSocket.Receive" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let maximum = extend_to_i32(text, arguments[1], analysis.values.get(&arguments[1]).expect("validated maximum"));
            let timeout = extend_to_i32(text, arguments[2], analysis.values.get(&arguments[2]).expect("validated timeout"));
            let _ = writeln!(text, "  %netout{dest} = alloca i64");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_udp_receive_handle(i64 %nethandle{dest}, i32 {maximum}, i32 {timeout}, ptr %netout{dest})");
            let _ = writeln!(text, "  %netdata{dest} = load i64, ptr %netout{dest}");
            emit_handle_result(text, destination, format!("%netrc{dest}"), format!("%netdata{dest}"));
        }
        "HOST.Net.UDPPacket.Size" | "HOST.Net.UDPPacket.Truncated" | "HOST.Net.UDPPacket.WasTruncated" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let function = if name.ends_with("Size") { "bn_rt_net_udp_packet_size" } else { "bn_rt_net_udp_packet_truncated" };
            let _ = writeln!(text, "  %netpacket{dest} = call i32 @{function}(i64 %nethandle{dest})");
            if name.ends_with("Size") {
                let _ = writeln!(text, "  %v{dest} = add i32 %netpacket{dest}, 0");
            } else {
                let _ = writeln!(text, "  %v{dest} = icmp ne i32 %netpacket{dest}, 0");
            }
        }
        "HOST.Net.UDPPacket.CopyTo" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let _ = writeln!(text, "  %netbuffer{dest} = extractvalue {{ ptr, i32 }} %v{}, 0", arguments[1].0);
            let maximum = extend_to_i32(text, arguments[2], analysis.values.get(&arguments[2]).expect("validated maximum"));
            let _ = writeln!(text, "  %netcopied{dest} = alloca i32");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_udp_packet_copy_to(i64 %nethandle{dest}, ptr %netbuffer{dest}, i32 {maximum}, ptr %netcopied{dest})");
            let _ = writeln!(text, "  %netcopiedv{dest} = load i32, ptr %netcopied{dest}");
            let _ = writeln!(text, "  %netcopied64{dest} = sext i32 %netcopiedv{dest} to i64");
            emit_handle_result(text, destination, format!("%netrc{dest}"), format!("%netcopied64{dest}"));
        }
        "HOST.Net.UDPPacket.Source" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let _ = writeln!(text, "  %netaddr{dest} = alloca ptr");
            let _ = writeln!(text, "  %netport{dest} = alloca i32");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_udp_packet_source(i64 %nethandle{dest}, ptr %netaddr{dest}, ptr %netport{dest})");
            let _ = writeln!(text, "  %netaddrv{dest} = load ptr, ptr %netaddr{dest}");
            let _ = writeln!(text, "  %netportv{dest} = load i32, ptr %netport{dest}");
            let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 %netrc{dest}, 0");
            let _ = writeln!(text, "  %netep0{dest} = insertvalue {{ i1, ptr, i32 }} undef, i1 %neterr{dest}, 0");
            let _ = writeln!(text, "  %netep1{dest} = insertvalue {{ i1, ptr, i32 }} %netep0{dest}, ptr %netaddrv{dest}, 1");
            let _ = writeln!(text, "  %v{dest} = insertvalue {{ i1, ptr, i32 }} %netep1{dest}, i32 %netportv{dest}, 2");
        }
        "HOST.Net.TCPStream.Write" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let _ = writeln!(text, "  %netbuffer{dest} = extractvalue {{ ptr, i32 }} %v{}, 0", arguments[1].0);
            let length = extend_to_i32(text, arguments[2], analysis.values.get(&arguments[2]).expect("validated length"));
            let _ = writeln!(text, "  %netout{dest} = alloca i32");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_tcp_write(i64 %nethandle{dest}, ptr %netbuffer{dest}, i32 {length}, ptr %netout{dest})");
            let _ = writeln!(text, "  %netwritten{dest} = load i32, ptr %netout{dest}");
            let _ = writeln!(text, "  %netwritten64{dest} = sext i32 %netwritten{dest} to i64");
            emit_handle_result(text, destination, format!("%netrc{dest}"), format!("%netwritten64{dest}"));
        }
        "HOST.Clock.Monotonic" => {
            let _ = writeln!(
                text,
                "  %v{} = call i64 @bn_rt_clock_monotonic()",
                destination.0
            );
        }
        "HOST.Console.Cls" => {
            emit_checked_i32_eq_zero(
                text,
                block_id,
                destination,
                "call i32 @bn_rt_console_cls()",
                state,
            );
        }
        "HOST.Console.Beep" => {
            emit_checked_i32_eq_zero(
                text,
                block_id,
                destination,
                "call i32 @bn_rt_console_beep()",
                state,
            );
        }
        "HOST.Console.PrintAt" => {
            let column = extend_to_i32(
                text,
                arguments[0],
                analysis
                    .values
                    .get(&arguments[0])
                    .expect("validated column"),
            );
            let row = extend_to_i32(
                text,
                arguments[1],
                analysis.values.get(&arguments[1]).expect("validated row"),
            );
            let call = format!(
                "call i32 @bn_rt_console_print_at(i32 {column}, i32 {row}, ptr %v{})",
                arguments[2].0
            );
            emit_checked_i32_eq_zero(text, block_id, destination, &call, state);
        }
        "HOST.Console.NumCols" => {
            emit_checked_i32_sge_zero(
                text,
                block_id,
                destination,
                "call i32 @bn_rt_console_num_cols()",
                state,
            );
        }
        "HOST.Console.NumRows" => {
            emit_checked_i32_sge_zero(
                text,
                block_id,
                destination,
                "call i32 @bn_rt_console_num_rows()",
                state,
            );
        }
        "HOST.Net.Address.Parse" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %netout{dest} = alloca ptr");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_address_parse(ptr %v{}, ptr %netout{dest})",
                arguments[0].0
            );
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                "0",
            );
        }
        "HOST.Net.Endpoint.Create" => {
            let dest = destination.0;
            let address = net_payload_ptr(text, arguments[0]);
            let port = extend_to_i32(
                text,
                arguments[1],
                analysis.values.get(&arguments[1]).expect("validated port"),
            );
            let _ = writeln!(
                text,
                "  %netep0{dest} = insertvalue {{ ptr, i32 }} undef, ptr {address}, 0"
            );
            let _ = writeln!(
                text,
                "  %netep1{dest} = insertvalue {{ ptr, i32 }} %netep0{dest}, i32 {port}, 1"
            );
            let _ = writeln!(
                text,
                "  %netagg{dest} = insertvalue {{ i1, ptr, i32 }} undef, i1 false, 0"
            );
            let _ = writeln!(
                text,
                "  %netaggp{dest} = insertvalue {{ i1, ptr, i32 }} %netagg{dest}, ptr {address}, 1"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr, i32 }} %netaggp{dest}, i32 {port}, 2"
            );
        }
        "HOST.Net.UDPBind" => {
            let dest = destination.0;
            let (address, port) = endpoint_parts(text, arguments[0], analysis);
            let _ = writeln!(text, "  %netout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_udp_bind(ptr {address}, i32 {port}, ptr %netout{dest})"
            );
            let _ = writeln!(text, "  %netdata{dest} = load i64, ptr %netout{dest}");
            emit_handle_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netdata{dest}"),
            );
        }
        "HOST.Net.TCPConnect" => {
            let dest = destination.0;
            let (address, port) = endpoint_parts(text, arguments[0], analysis);
            let timeout = extend_to_i32(
                text,
                arguments[1],
                analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated timeout"),
            );
            let _ = writeln!(text, "  %netout{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_tcp_connect(ptr {address}, i32 {port}, i32 {timeout}, ptr %netout{dest})"
            );
            let _ = writeln!(text, "  %netdata{dest} = load i64, ptr %netout{dest}");
            emit_handle_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netdata{dest}"),
            );
        }
        "HOST.Net.TCPListen" => {
            let dest = destination.0;
            let backlog = extend_to_i32(text, arguments[1], analysis.values.get(&arguments[1]).expect("validated backlog"));
            let _ = writeln!(text, "  %netvec{dest} = extractvalue {{ ptr, i32 }} %v{}, 0", arguments[0].0);
            let _ = writeln!(text, "  %netaddress{dest} = load ptr, ptr %netvec{dest}");
            let _ = writeln!(text, "  %netportptr{dest} = getelementptr i8, ptr %netvec{dest}, i64 8");
            let _ = writeln!(text, "  %netport{dest} = load i32, ptr %netportptr{dest}");
            let _ = writeln!(text, "  %netout{dest} = alloca i64");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_tcp_listen_with_backlog(ptr %netaddress{dest}, i32 %netport{dest}, i32 {backlog}, ptr %netout{dest})");
            let _ = writeln!(text, "  %netdata{dest} = load i64, ptr %netout{dest}");
            emit_handle_result(text, destination, format!("%netrc{dest}"), format!("%netdata{dest}"));
        }
        "HOST.Net.TCPListener.Accept" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let timeout = extend_to_i32(text, arguments[1], analysis.values.get(&arguments[1]).expect("validated timeout"));
            let _ = writeln!(text, "  %netout{dest} = alloca i64");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_tcp_accept(i64 %nethandle{dest}, i32 {timeout}, ptr %netout{dest})");
            let _ = writeln!(text, "  %netdata{dest} = load i64, ptr %netout{dest}");
            emit_handle_result(text, destination, format!("%netrc{dest}"), format!("%netdata{dest}"));
        }
        "HOST.Net.TCPListener.LocalEndpoint" => {
            let dest = destination.0;
            let _ = writeln!(text, "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2", arguments[0].0);
            let _ = writeln!(text, "  %netaddress{dest} = alloca ptr");
            let _ = writeln!(text, "  %netport{dest} = alloca i32");
            let _ = writeln!(text, "  %netrc{dest} = call i32 @bn_rt_net_tcp_listener_local_endpoint(i64 %nethandle{dest}, ptr %netaddress{dest}, ptr %netport{dest})");
            let _ = writeln!(text, "  %netaddrv{dest} = load ptr, ptr %netaddress{dest}");
            let _ = writeln!(text, "  %netportv{dest} = load i32, ptr %netport{dest}");
            let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 %netrc{dest}, 0");
            let _ = writeln!(text, "  %netep0{dest} = insertvalue {{ i1, ptr, i32 }} undef, i1 %neterr{dest}, 0");
            let _ = writeln!(text, "  %netep1{dest} = insertvalue {{ i1, ptr, i32 }} %netep0{dest}, ptr %netaddrv{dest}, 1");
            let _ = writeln!(text, "  %v{dest} = insertvalue {{ i1, ptr, i32 }} %netep1{dest}, i32 %netportv{dest}, 2");
        }
        "HOST.Net.TCPStream.Close" | "HOST.Net.TCPListener.Close" | "HOST.Net.UDPSocket.Close" => {
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %nethandle{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                arguments[0].0
            );
            emit_void_result(
                text,
                destination,
                format!("call i32 @bn_rt_net_handle_close(i64 %nethandle{dest})"),
            );
        }
        "HOST.Net.Endpoint.Port" => {
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or("{ i1, ptr, i32 }");
            let index = if ty == "{ ptr, i32 }" { 1 } else { 2 };
            let _ = writeln!(
                text,
                "  %netport{} = extractvalue {ty} %v{}, {index}",
                destination.0, arguments[0].0
            );
            if llvm_type(
                analysis
                    .values
                    .get(&destination)
                    .expect("validated port result"),
            ) == Some("i16")
            {
                let _ = writeln!(
                    text,
                    "  %v{} = trunc i32 %netport{} to i16",
                    destination.0, destination.0
                );
            } else {
                let _ = writeln!(
                    text,
                    "  %v{} = add i32 %netport{}, 0",
                    destination.0, destination.0
                );
            }
        }
        "HOST.Net.Endpoint.Address" => {
            let dest = destination.0;
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or("{ i1, ptr, i32 }");
            let index = if ty == "{ ptr, i32 }" { 0 } else { 1 };
            let _ = writeln!(
                text,
                "  %netaddr{dest} = extractvalue {ty} %v{}, {index}",
                arguments[0].0
            );
            let _ = writeln!(
                text,
                "  %netfat0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 false, 0"
            );
            let _ = writeln!(
                text,
                "  %netfat1{dest} = insertvalue {{ i1, ptr, i64 }} %netfat0{dest}, ptr %netaddr{dest}, 1"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netfat1{dest}, i64 0, 2"
            );
        }
        "HOST.Net.Ping" => {
            let dest = destination.0;
            let addr = net_payload_ptr(text, arguments[0]);
            let timeout = extend_to_i32(
                text,
                arguments[1],
                analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated timeout"),
            );
            let _ = writeln!(text, "  %netout{dest} = alloca ptr");
            let _ = writeln!(text, "  %netrtt{dest} = alloca i64");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_ping(ptr {addr}, i32 {timeout}, ptr %netout{dest}, ptr %netrtt{dest})"
            );
            let _ = writeln!(text, "  %netrttv{dest} = load i64, ptr %netrtt{dest}");
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                format!("%netrttv{dest}"),
            );
        }
        "HOST.Net.Reverse" => {
            let dest = destination.0;
            let addr = net_payload_ptr(text, arguments[0]);
            let timeout = extend_to_i32(
                text,
                arguments[1],
                analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated timeout"),
            );
            let _ = writeln!(text, "  %netout{dest} = alloca ptr");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_reverse(ptr {addr}, i32 {timeout}, ptr %netout{dest})"
            );
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                "0",
            );
        }
        "HOST.Net.Resolve" => {
            let dest = destination.0;
            let timeout = extend_to_i32(
                text,
                arguments[1],
                analysis
                    .values
                    .get(&arguments[1])
                    .expect("validated timeout"),
            );
            let _ = writeln!(text, "  %netout{dest} = alloca ptr");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_resolve(ptr %v{}, i32 {timeout}, ptr %netout{dest})",
                arguments[0].0
            );
            let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 %netrc{dest}, 0");
            let _ = writeln!(text, "  %netdata{dest} = load ptr, ptr %netout{dest}");
            let _ = writeln!(
                text,
                "  %netagg{dest} = insertvalue {{ i1, ptr }} undef, i1 %neterr{dest}, 0"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr }} %netagg{dest}, ptr %netdata{dest}, 1"
            );
        }
        "HOST.Net.Addresses.Count" => {
            let _ = writeln!(
                text,
                "  %netaddr{0} = extractvalue {{ i1, ptr }} %v{1}, 1",
                destination.0, arguments[0].0
            );
            let _ = writeln!(
                text,
                "  %v{} = call i32 @bn_rt_net_addresses_count(ptr %netaddr{})",
                destination.0, destination.0
            );
        }
        "HOST.Net.Addresses.Get" => {
            let dest = destination.0;
            let index = extend_to_i32(
                text,
                arguments[1],
                analysis.values.get(&arguments[1]).expect("validated index"),
            );
            let _ = writeln!(
                text,
                "  %netaddr{dest} = extractvalue {{ i1, ptr }} %v{}, 1",
                arguments[0].0
            );
            let _ = writeln!(text, "  %netout{dest} = alloca ptr");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_addresses_get(ptr %netaddr{dest}, i32 {index}, ptr %netout{dest})"
            );
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                "0",
            );
        }
        "HOST.Net.Neighbor" => {
            let dest = destination.0;
            let addr = net_payload_ptr(text, arguments[0]);
            let _ = writeln!(text, "  %netout{dest} = alloca ptr");
            let _ = writeln!(
                text,
                "  %netrc{dest} = call i32 @bn_rt_net_neighbor(ptr {addr}, ptr %netout{dest})"
            );
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                "0",
            );
        }
        "HOST.Net.PingReply.RoundTripMicroseconds" => {
            let _ = writeln!(
                text,
                "  %v{} = extractvalue {{ i1, ptr, i64 }} %v{}, 2",
                destination.0, arguments[0].0
            );
        }
        "HOST.Net.PingReply.Address" => {
            let dest = destination.0;
            let _ = writeln!(
                text,
                "  %netaddr{dest} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
                arguments[0].0
            );
            let _ = writeln!(
                text,
                "  %netfat0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 false, 0"
            );
            let _ = writeln!(
                text,
                "  %netfat1{dest} = insertvalue {{ i1, ptr, i64 }} %netfat0{dest}, ptr %netaddr{dest}, 1"
            );
            let _ = writeln!(
                text,
                "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netfat1{dest}, i64 0, 2"
            );
        }
        _ => unreachable!("validated bn_rt host call"),
    }
}

fn net_payload_ptr(text: &mut String, value: ValueId) -> String {
    let temp = format!("netpay{}", value.0);
    let _ = writeln!(
        text,
        "  %{temp} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
        value.0
    );
    format!("%{temp}")
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

fn endpoint_parts(
    text: &mut String,
    value: ValueId,
    analysis: &LoweringAnalysis<'_>,
) -> (String, String) {
    let ty = analysis
        .values
        .get(&value)
        .and_then(llvm_type)
        .unwrap_or("{ ptr, i32 }");
    if ty == "{ ptr, i32 }" {
        let address = format!("%netepaddr{}", value.0);
        let port = format!("%netepport{}", value.0);
        let _ = writeln!(text, "  {address} = extractvalue {ty} %v{}, 0", value.0);
        let _ = writeln!(text, "  {port} = extractvalue {ty} %v{}, 1", value.0);
        (address, port)
    } else {
        let address = format!("%netepaddr{}", value.0);
        let port = format!("%netepport{}", value.0);
        let _ = writeln!(text, "  {address} = extractvalue {ty} %v{}, 1", value.0);
        let _ = writeln!(text, "  {port} = extractvalue {ty} %v{}, 2", value.0);
        (address, port)
    }
}

fn emit_handle_result(
    text: &mut String,
    destination: ValueId,
    rc: impl AsRef<str>,
    handle: impl AsRef<str>,
) {
    let dest = destination.0;
    let rc = rc.as_ref();
    let handle = handle.as_ref();
    let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 {rc}, 0");
    let _ = writeln!(
        text,
        "  %netagg{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %neterr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %netaggp{dest} = insertvalue {{ i1, ptr, i64 }} %netagg{dest}, ptr null, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netaggp{dest}, i64 {handle}, 2"
    );
}

fn emit_void_result(text: &mut String, destination: ValueId, rc: impl AsRef<str>) {
    let dest = destination.0;
    let _ = writeln!(text, "  %netrc{dest} = {}", rc.as_ref());
    let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 %netrc{dest}, 0");
    let _ = writeln!(
        text,
        "  %netagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %neterr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %netagg1{dest} = insertvalue {{ i1, ptr, i64 }} %netagg0{dest}, ptr null, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netagg1{dest}, i64 0, 2"
    );
}

fn emit_net_result(
    text: &mut String,
    destination: ValueId,
    rc: impl AsRef<str>,
    out_slot: impl AsRef<str>,
    rtt: impl AsRef<str>,
) {
    let dest = destination.0;
    let rc = rc.as_ref();
    let out_slot = out_slot.as_ref();
    let rtt = rtt.as_ref();
    let _ = writeln!(text, "  %neterr{dest} = icmp ne i32 {rc}, 0");
    let _ = writeln!(text, "  %netdata{dest} = load ptr, ptr {out_slot}");
    let _ = writeln!(
        text,
        "  %netagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 %neterr{dest}, 0"
    );
    let _ = writeln!(
        text,
        "  %netagg1{dest} = insertvalue {{ i1, ptr, i64 }} %netagg0{dest}, ptr %netdata{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %netagg1{dest}, i64 {rtt}, 2"
    );
}

fn emit_checked_i32_eq_zero(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    call: &str,
    state: &mut EmissionState,
) {
    let ok = take_continuation(block_id, state);
    let _ = writeln!(text, "  %bnrtrc{} = {call}", destination.0);
    let _ = writeln!(
        text,
        "  %bnrtok{} = icmp eq i32 %bnrtrc{}, 0",
        destination.0, destination.0
    );
    let _ = writeln!(
        text,
        "  br i1 %bnrtok{}, label %{ok}, label %trap_bn_rt",
        destination.0
    );
    let _ = writeln!(text, "{ok}:");
    state.needs_bn_rt_trap = true;
}

fn emit_checked_i32_sge_zero(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    call: &str,
    state: &mut EmissionState,
) {
    let ok = take_continuation(block_id, state);
    let _ = writeln!(text, "  %v{} = {call}", destination.0);
    let _ = writeln!(
        text,
        "  %bnrtok{} = icmp sge i32 %v{}, 0",
        destination.0, destination.0
    );
    let _ = writeln!(
        text,
        "  br i1 %bnrtok{}, label %{ok}, label %trap_bn_rt",
        destination.0
    );
    let _ = writeln!(text, "{ok}:");
    state.needs_bn_rt_trap = true;
}

fn extend_to_i32(text: &mut String, value: ValueId, ty: &Type) -> String {
    match llvm_type(ty).expect("validated integer extension type") {
        "i32" => format!("%v{}", value.0),
        "i64" => {
            let temp = format!("bnrti32{}", value.0);
            let _ = writeln!(text, "  %{temp} = trunc i64 %v{} to i32", value.0);
            format!("%{temp}")
        }
        llvm_ty => {
            let opcode = if is_unsigned(ty) { "zext" } else { "sext" };
            let temp = format!("bnrti32{}", value.0);
            let _ = writeln!(text, "  %{temp} = {opcode} {llvm_ty} %v{} to i32", value.0);
            format!("%{temp}")
        }
    }
}

fn take_continuation(block_id: BlockId, state: &mut EmissionState) -> String {
    let name = format!("b{}.cont{}", block_id.0, state.continuation_count);
    state.continuation_count += 1;
    name
}
