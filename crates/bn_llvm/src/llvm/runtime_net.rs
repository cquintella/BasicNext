// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// HOST.Net lowering: addresses, endpoints, TCP, UDP, ping and name
// resolution calls into `bn_rt`.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{self, I1, I8, I16, I32, I64, Ptr},
};
use crate::layout::{handle_result_ty, typed_llvm, vector_ty};
use runtime_abi::{
    ERROR_TAKE, NET_ADDRESS_PARSE, NET_ADDRESSES_COUNT, NET_ADDRESSES_GET, NET_HANDLE_CLOSE,
    NET_NEIGHBOR, NET_PING, NET_RESOLVE, NET_REVERSE, NET_TCP_ACCEPT, NET_TCP_CONNECT,
    NET_TCP_LISTEN_WITH_BACKLOG, NET_TCP_LISTENER_LOCAL_ENDPOINT, NET_TCP_READ,
    NET_TCP_STREAM_LOCAL_ENDPOINT, NET_TCP_STREAM_REMOTE_ENDPOINT, NET_TCP_WRITE, NET_UDP_BIND,
    NET_UDP_LOCAL_ENDPOINT, NET_UDP_PACKET_COPY_TO, NET_UDP_PACKET_SIZE, NET_UDP_PACKET_SOURCE,
    NET_UDP_PACKET_TRUNCATED, NET_UDP_RECEIVE_HANDLE, NET_UDP_SEND_TO,
};

pub(crate) fn lower_net_call(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let raw = LlvmOperand::raw;
    let i32_argument = |text: &mut String, index: usize, what: &str| {
        extend_to_i32(
            text,
            arguments[index],
            analysis.values.get(&arguments[index]).expect(what),
        )
    };
    // `%nethandle<dest>`: the runtime handle inside a handle result.
    let net_handle = |text: &mut String| {
        text.assign(
            format!("nethandle{dest}"),
            LlvmInst::extract(handle_result_ty(), value_reg(arguments[0]), 2),
        );
        reg(format!("nethandle{dest}"))
    };
    match name {
        "HOST.Net.UDPSocket.SendTo" => {
            let handle = net_handle(text);
            let (address, port) = endpoint_parts(text, arguments[1], analysis);
            // `bn_rt_call_supported` admits only a `{ ptr, i32 }` buffer.
            text.assign(
                format!("netbytes{dest}"),
                LlvmInst::extract(vector_ty(), value_reg(arguments[2]), 0),
            );
            let len = i32_argument(text, 3, "validated length");
            let length = format!("%netlen{dest}");
            text.assign(
                format!("netlen{dest}"),
                LlvmInst::binary(BinaryOp::Add, I32, raw(len), LlvmOperand::int(0)),
            );
            text.assign(format!("netwritten{dest}"), LlvmInst::alloca(I32));
            emit_buffer_bound(
                text,
                block_id,
                destination,
                arguments[2],
                &length,
                "datagram exceeds the buffer",
                state,
            );
            text.assign(
                format!("netrc{dest}"),
                NET_UDP_SEND_TO.call([
                    handle,
                    raw(address),
                    raw(port),
                    reg(format!("netbytes{dest}")),
                    raw(length),
                    reg(format!("netwritten{dest}")),
                ]),
            );
            text.assign(
                format!("netwrittenv{dest}"),
                LlvmInst::load(I32, reg(format!("netwritten{dest}"))),
            );
            text.assign(
                format!("netwritten64{dest}"),
                LlvmInst::cast(CastOp::SExt, I32, reg(format!("netwrittenv{dest}")), I64),
            );
            emit_handle_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netwritten64{dest}"),
            );
        }
        "HOST.Net.TCPStream.Read" | "HOST.Net.TCPStream.Write" | "HOST.Net.UDPPacket.CopyTo" => {
            let handle = net_handle(text);
            text.assign(
                format!("netbuffer{dest}"),
                LlvmInst::extract(vector_ty(), value_reg(arguments[1]), 0),
            );
            let (what, out, bound) = match name {
                "HOST.Net.TCPStream.Read" => {
                    ("validated length", "netout", "read exceeds the buffer")
                }
                "HOST.Net.TCPStream.Write" => {
                    ("validated length", "netout", "write exceeds the buffer")
                }
                _ => ("validated maximum", "netcopied", "copy exceeds the buffer"),
            };
            let length = i32_argument(text, 2, what);
            text.assign(format!("{out}{dest}"), LlvmInst::alloca(I32));
            emit_buffer_bound(
                text,
                block_id,
                destination,
                arguments[1],
                &length,
                bound,
                state,
            );
            let operands = [
                handle,
                reg(format!("netbuffer{dest}")),
                raw(length),
                reg(format!("{out}{dest}")),
            ];
            let call = match name {
                "HOST.Net.TCPStream.Read" => NET_TCP_READ.call(operands),
                "HOST.Net.TCPStream.Write" => NET_TCP_WRITE.call(operands),
                _ => NET_UDP_PACKET_COPY_TO.call(operands),
            };
            text.assign(format!("netrc{dest}"), call);
            if name == "HOST.Net.TCPStream.Read" {
                lower_tcp_read_result(text, destination);
            } else {
                let count = if out == "netout" {
                    "netwritten"
                } else {
                    "netcopiedv"
                };
                let wide = if out == "netout" {
                    "netwritten64"
                } else {
                    "netcopied64"
                };
                text.assign(
                    format!("{count}{dest}"),
                    LlvmInst::load(I32, reg(format!("{out}{dest}"))),
                );
                text.assign(
                    format!("{wide}{dest}"),
                    LlvmInst::cast(CastOp::SExt, I32, reg(format!("{count}{dest}")), I64),
                );
                emit_handle_result(
                    text,
                    destination,
                    format!("%netrc{dest}"),
                    format!("%{wide}{dest}"),
                );
            }
        }
        "HOST.Net.TCPStream.LocalEndpoint"
        | "HOST.Net.TCPStream.RemoteEndpoint"
        | "HOST.Net.TCPListener.LocalEndpoint"
        | "HOST.Net.UDPSocket.LocalEndpoint"
        | "HOST.Net.UDPPacket.Source" => {
            let handle = net_handle(text);
            let address = if name == "HOST.Net.UDPPacket.Source" {
                "netaddr"
            } else {
                "netaddress"
            };
            text.assign(format!("{address}{dest}"), LlvmInst::alloca(Ptr));
            text.assign(format!("netport{dest}"), LlvmInst::alloca(I32));
            let operands = [
                handle,
                reg(format!("{address}{dest}")),
                reg(format!("netport{dest}")),
            ];
            let call = match name {
                "HOST.Net.TCPStream.LocalEndpoint" => NET_TCP_STREAM_LOCAL_ENDPOINT.call(operands),
                "HOST.Net.TCPStream.RemoteEndpoint" => {
                    NET_TCP_STREAM_REMOTE_ENDPOINT.call(operands)
                }
                "HOST.Net.TCPListener.LocalEndpoint" => {
                    NET_TCP_LISTENER_LOCAL_ENDPOINT.call(operands)
                }
                "HOST.Net.UDPSocket.LocalEndpoint" => NET_UDP_LOCAL_ENDPOINT.call(operands),
                _ => NET_UDP_PACKET_SOURCE.call(operands),
            };
            text.assign(format!("netrc{dest}"), call);
            text.assign(
                format!("netaddrv{dest}"),
                LlvmInst::load(Ptr, reg(format!("{address}{dest}"))),
            );
            text.assign(
                format!("netportv{dest}"),
                LlvmInst::load(I32, reg(format!("netport{dest}"))),
            );
            emit_endpoint_result(
                text,
                destination,
                &format!("%netrc{dest}"),
                &format!("%netaddrv{dest}"),
                &format!("%netportv{dest}"),
            );
        }
        "HOST.Net.UDPSocket.Receive" => {
            let handle = net_handle(text);
            let maximum = i32_argument(text, 1, "validated maximum");
            let timeout = i32_argument(text, 2, "validated timeout");
            text.assign(format!("netout{dest}"), LlvmInst::alloca(I64));
            text.assign(
                format!("netrc{dest}"),
                NET_UDP_RECEIVE_HANDLE.call([
                    handle,
                    raw(maximum),
                    raw(timeout),
                    reg(format!("netout{dest}")),
                ]),
            );
            load_handle_result(text, destination);
        }
        "HOST.Net.UDPPacket.Size"
        | "HOST.Net.UDPPacket.Truncated"
        | "HOST.Net.UDPPacket.WasTruncated" => {
            let handle = net_handle(text);
            let size = name.ends_with("Size");
            let call = if size {
                NET_UDP_PACKET_SIZE.call([handle])
            } else {
                NET_UDP_PACKET_TRUNCATED.call([handle])
            };
            text.assign(format!("netpacket{dest}"), call);
            let packet = reg(format!("netpacket{dest}"));
            text.assign(
                format!("v{dest}"),
                if size {
                    LlvmInst::binary(BinaryOp::Add, I32, packet, LlvmOperand::int(0))
                } else {
                    LlvmInst::icmp(ICmpCond::Ne, I32, packet, LlvmOperand::int(0))
                },
            );
        }
        "HOST.Net.Address.Parse" => {
            text.assign(format!("netout{dest}"), LlvmInst::alloca(Ptr));
            text.assign(
                format!("netrc{dest}"),
                NET_ADDRESS_PARSE.call([value_reg(arguments[0]), reg(format!("netout{dest}"))]),
            );
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                "0",
            );
        }
        "HOST.Net.Address.ToString" => text.assign(
            format!("v{dest}"),
            LlvmInst::extract(handle_result_ty(), value_reg(arguments[0]), 1),
        ),
        "HOST.Net.Endpoint.Create" => {
            let address = net_payload_ptr(text, arguments[0]);
            let port = i32_argument(text, 1, "validated port");
            let endpoint = LlvmType::struct_of([I1, Ptr, I32]);
            text.assign(
                format!("netep0{dest}"),
                LlvmInst::insert(
                    vector_ty(),
                    LlvmOperand::undef(),
                    Ptr,
                    raw(address.clone()),
                    0,
                ),
            );
            text.assign(
                format!("netep1{dest}"),
                LlvmInst::insert(
                    vector_ty(),
                    reg(format!("netep0{dest}")),
                    I32,
                    raw(port.clone()),
                    1,
                ),
            );
            text.assign(
                format!("netagg{dest}"),
                LlvmInst::insert(
                    endpoint.clone(),
                    LlvmOperand::undef(),
                    I1,
                    LlvmOperand::bool(false),
                    0,
                ),
            );
            text.assign(
                format!("netaggp{dest}"),
                LlvmInst::insert(
                    endpoint.clone(),
                    reg(format!("netagg{dest}")),
                    Ptr,
                    raw(address),
                    1,
                ),
            );
            text.assign(
                format!("v{dest}"),
                LlvmInst::insert(endpoint, reg(format!("netaggp{dest}")), I32, raw(port), 2),
            );
        }
        "HOST.Net.UDPBind" | "HOST.Net.TCPConnect" => {
            let (address, port) = endpoint_parts(text, arguments[0], analysis);
            let call = if name == "HOST.Net.UDPBind" {
                text.assign(format!("netout{dest}"), LlvmInst::alloca(I64));
                NET_UDP_BIND.call([raw(address), raw(port), reg(format!("netout{dest}"))])
            } else {
                let timeout = i32_argument(text, 1, "validated timeout");
                text.assign(format!("netout{dest}"), LlvmInst::alloca(I64));
                NET_TCP_CONNECT.call([
                    raw(address),
                    raw(port),
                    raw(timeout),
                    reg(format!("netout{dest}")),
                ])
            };
            text.assign(format!("netrc{dest}"), call);
            load_handle_result(text, destination);
        }
        "HOST.Net.TCPListen" => {
            let backlog = i32_argument(text, 1, "validated backlog");
            text.assign(
                format!("netvec{dest}"),
                LlvmInst::extract(vector_ty(), value_reg(arguments[0]), 0),
            );
            text.assign(
                format!("netaddress{dest}"),
                LlvmInst::load(Ptr, reg(format!("netvec{dest}"))),
            );
            text.assign(
                format!("netportptr{dest}"),
                LlvmInst::GetElementPtr {
                    inbounds: false,
                    elem_ty: I8,
                    ptr: reg(format!("netvec{dest}")),
                    indices: vec![(I64, LlvmOperand::int(8))],
                },
            );
            text.assign(
                format!("netport{dest}"),
                LlvmInst::load(I32, reg(format!("netportptr{dest}"))),
            );
            text.assign(format!("netout{dest}"), LlvmInst::alloca(I64));
            text.assign(
                format!("netrc{dest}"),
                NET_TCP_LISTEN_WITH_BACKLOG.call([
                    reg(format!("netaddress{dest}")),
                    reg(format!("netport{dest}")),
                    raw(backlog),
                    reg(format!("netout{dest}")),
                ]),
            );
            load_handle_result(text, destination);
        }
        "HOST.Net.TCPListener.Accept" => {
            let handle = net_handle(text);
            let timeout = i32_argument(text, 1, "validated timeout");
            text.assign(format!("netout{dest}"), LlvmInst::alloca(I64));
            text.assign(
                format!("netrc{dest}"),
                NET_TCP_ACCEPT.call([handle, raw(timeout), reg(format!("netout{dest}"))]),
            );
            load_handle_result(text, destination);
        }
        "HOST.Net.TCPStream.Close" | "HOST.Net.TCPListener.Close" | "HOST.Net.UDPSocket.Close" => {
            let handle = net_handle(text);
            emit_void_result(
                text,
                destination,
                NET_HANDLE_CLOSE.call([handle]).to_string(),
            );
        }
        "HOST.Net.Endpoint.Port" => {
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or("{ i1, ptr, i32 }");
            let index = if ty == "{ ptr, i32 }" { 1 } else { 2 };
            text.assign(
                format!("netport{dest}"),
                LlvmInst::extract(typed_llvm(ty), value_reg(arguments[0]), index),
            );
            let port = reg(format!("netport{dest}"));
            let narrow = llvm_type(
                analysis
                    .values
                    .get(&destination)
                    .expect("validated port result"),
            ) == Some("i16");
            text.assign(
                format!("v{dest}"),
                if narrow {
                    LlvmInst::cast(CastOp::Trunc, I32, port, I16)
                } else {
                    LlvmInst::binary(BinaryOp::Add, I32, port, LlvmOperand::int(0))
                },
            );
        }
        "HOST.Net.Endpoint.Address" => {
            let ty = analysis
                .values
                .get(&arguments[0])
                .and_then(llvm_type)
                .unwrap_or("{ i1, ptr, i32 }");
            let index = usize::from(ty != "{ ptr, i32 }");
            text.assign(
                format!("netaddr{dest}"),
                LlvmInst::extract(typed_llvm(ty), value_reg(arguments[0]), index),
            );
            lower_address_result(text, dest);
        }
        "HOST.Net.Ping" | "HOST.Net.Reverse" => {
            let address = net_payload_ptr(text, arguments[0]);
            let timeout = i32_argument(text, 1, "validated timeout");
            text.assign(format!("netout{dest}"), LlvmInst::alloca(Ptr));
            let payload = if name == "HOST.Net.Ping" {
                text.assign(format!("netrtt{dest}"), LlvmInst::alloca(I64));
                text.assign(
                    format!("netrc{dest}"),
                    NET_PING.call([
                        raw(address),
                        raw(timeout),
                        reg(format!("netout{dest}")),
                        reg(format!("netrtt{dest}")),
                    ]),
                );
                text.assign(
                    format!("netrttv{dest}"),
                    LlvmInst::load(I64, reg(format!("netrtt{dest}"))),
                );
                format!("%netrttv{dest}")
            } else {
                text.assign(
                    format!("netrc{dest}"),
                    NET_REVERSE.call([raw(address), raw(timeout), reg(format!("netout{dest}"))]),
                );
                "0".into()
            };
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                payload,
            );
        }
        "HOST.Net.Resolve" => {
            let timeout = i32_argument(text, 1, "validated timeout");
            lower_resolve(text, dest, value_reg(arguments[0]), raw(timeout));
        }
        "HOST.Net.Addresses.Count" => {
            text.assign(
                format!("netaddr{dest}"),
                LlvmInst::extract(LlvmType::struct_of([I1, Ptr]), value_reg(arguments[0]), 1),
            );
            text.assign(
                format!("v{dest}"),
                NET_ADDRESSES_COUNT.call([reg(format!("netaddr{dest}"))]),
            );
        }
        "HOST.Net.Addresses.Get" => {
            let index = i32_argument(text, 1, "validated index");
            text.assign(
                format!("netaddr{dest}"),
                LlvmInst::extract(LlvmType::struct_of([I1, Ptr]), value_reg(arguments[0]), 1),
            );
            text.assign(format!("netout{dest}"), LlvmInst::alloca(Ptr));
            text.assign(
                format!("netrc{dest}"),
                NET_ADDRESSES_GET.call([
                    reg(format!("netaddr{dest}")),
                    raw(index),
                    reg(format!("netout{dest}")),
                ]),
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
            let address = net_payload_ptr(text, arguments[0]);
            text.assign(format!("netout{dest}"), LlvmInst::alloca(Ptr));
            text.assign(
                format!("netrc{dest}"),
                NET_NEIGHBOR.call([raw(address), reg(format!("netout{dest}"))]),
            );
            emit_net_result(
                text,
                destination,
                format!("%netrc{dest}"),
                format!("%netout{dest}"),
                "0",
            );
        }
        "HOST.Net.PingReply.RoundTripMicroseconds" => text.assign(
            format!("v{dest}"),
            LlvmInst::extract(handle_result_ty(), value_reg(arguments[0]), 2),
        ),
        "HOST.Net.PingReply.Address" => {
            text.assign(
                format!("netaddr{dest}"),
                LlvmInst::extract(handle_result_ty(), value_reg(arguments[0]), 1),
            );
            lower_address_result(text, dest);
        }
        _ => unreachable!("validated HOST.Net call"),
    }
}

/// `TCPStream.Read`: zero bytes on success is EOF, status 4 for the shared
/// result builder.
fn lower_tcp_read_result(text: &mut String, destination: ValueId) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    text.assign(
        format!("netread{dest}"),
        LlvmInst::load(I32, reg(format!("netout{dest}"))),
    );
    text.assign(
        format!("netread64{dest}"),
        LlvmInst::cast(CastOp::SExt, I32, reg(format!("netread{dest}")), I64),
    );
    text.assign(
        format!("netok{dest}"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            I32,
            reg(format!("netrc{dest}")),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("netnone{dest}"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            I32,
            reg(format!("netread{dest}")),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("neteof{dest}"),
        LlvmInst::binary(
            BinaryOp::And,
            I1,
            reg(format!("netok{dest}")),
            reg(format!("netnone{dest}")),
        ),
    );
    text.assign(
        format!("netstatus{dest}"),
        LlvmInst::select(
            reg(format!("neteof{dest}")),
            I32,
            LlvmOperand::int(4),
            reg(format!("netrc{dest}")),
        ),
    );
    emit_status_result(
        text,
        destination,
        &format!("%netstatus{dest}"),
        Some(4),
        "null",
        &format!("%netread64{dest}"),
    );
}

/// Loads the handle `bn_rt` wrote to `%netout<dest>` and builds the result.
fn load_handle_result(text: &mut String, destination: ValueId) {
    let dest = destination.0;
    text.assign(
        format!("netdata{dest}"),
        LlvmInst::load(I64, LlvmOperand::reg(format!("netout{dest}"))),
    );
    emit_handle_result(
        text,
        destination,
        format!("%netrc{dest}"),
        format!("%netdata{dest}"),
    );
}

/// Wraps the address in `%netaddr<dest>` as a successful `Address` result.
fn lower_address_result(text: &mut String, dest: u32) {
    let reg = LlvmOperand::reg;
    text.assign(
        format!("netfat0{dest}"),
        LlvmInst::insert(
            handle_result_ty(),
            LlvmOperand::undef(),
            I1,
            LlvmOperand::bool(false),
            0,
        ),
    );
    text.assign(
        format!("netfat1{dest}"),
        LlvmInst::insert(
            handle_result_ty(),
            reg(format!("netfat0{dest}")),
            Ptr,
            reg(format!("netaddr{dest}")),
            1,
        ),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::insert(
            handle_result_ty(),
            reg(format!("netfat1{dest}")),
            I64,
            LlvmOperand::int(0),
            2,
        ),
    );
}

/// `Resolve`: the address list, or the error record the runtime kept
/// (code, message, cause).
fn lower_resolve(text: &mut String, dest: u32, host: LlvmOperand, timeout: LlvmOperand) {
    let reg = LlvmOperand::reg;
    let result = LlvmType::struct_of([I1, Ptr]);
    text.assign(format!("netout{dest}"), LlvmInst::alloca(Ptr));
    text.assign(
        format!("netrc{dest}"),
        NET_RESOLVE.call([host, timeout, reg(format!("netout{dest}"))]),
    );
    text.assign(
        format!("neterr{dest}"),
        LlvmInst::icmp(
            ICmpCond::Ne,
            I32,
            reg(format!("netrc{dest}")),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("netdata{dest}"),
        LlvmInst::load(Ptr, reg(format!("netout{dest}"))),
    );
    text.assign(
        format!("neterrint{dest}"),
        LlvmInst::cast(CastOp::ZExt, I1, reg(format!("neterr{dest}")), I32),
    );
    text.assign(
        format!("netrecord{dest}"),
        ERROR_TAKE.call([reg(format!("neterrint{dest}")), LlvmOperand::null()]),
    );
    text.assign(
        format!("netptr{dest}"),
        LlvmInst::select(
            reg(format!("neterr{dest}")),
            Ptr,
            reg(format!("netrecord{dest}")),
            reg(format!("netdata{dest}")),
        ),
    );
    text.assign(
        format!("netagg{dest}"),
        LlvmInst::insert(
            result.clone(),
            LlvmOperand::undef(),
            I1,
            reg(format!("neterr{dest}")),
            0,
        ),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::insert(
            result,
            reg(format!("netagg{dest}")),
            Ptr,
            reg(format!("netptr{dest}")),
            1,
        ),
    );
}

fn net_payload_ptr(text: &mut String, value: ValueId) -> String {
    let temp = format!("netpay{}", value.0);
    text.assign(
        temp.clone(),
        LlvmInst::extract(handle_result_ty(), value_reg(value), 1),
    );
    format!("%{temp}")
}

/// The address and port registers of an `Endpoint` (`{ ptr, i32 }`) or an
/// endpoint result (`{ i1, ptr, i32 }`).
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
    let first = usize::from(ty != "{ ptr, i32 }");
    let address = format!("netepaddr{}", value.0);
    let port = format!("netepport{}", value.0);
    text.assign(
        address.clone(),
        LlvmInst::extract(typed_llvm(ty), value_reg(value), first),
    );
    text.assign(
        port.clone(),
        LlvmInst::extract(typed_llvm(ty), value_reg(value), first + 1),
    );
    (format!("%{address}"), format!("%{port}"))
}

/// A `HOST.Net` result whose success value is in `out_slot`: the value, or
/// the `Error` the runtime recorded (`emit_status_result`); `payload` is the
/// success `i64` (a round-trip time).
fn emit_net_result(
    text: &mut String,
    destination: ValueId,
    rc: impl AsRef<str>,
    out_slot: impl AsRef<str>,
    payload: impl AsRef<str>,
) {
    let dest = destination.0;
    text.assign(
        format!("netdata{dest}"),
        LlvmInst::load(Ptr, LlvmOperand::raw(out_slot.as_ref())),
    );
    emit_status_result(
        text,
        destination,
        rc.as_ref(),
        None,
        &format!("%netdata{dest}"),
        payload.as_ref(),
    );
}
