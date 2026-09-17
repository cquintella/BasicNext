// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Net` — the language's network capability, served through the
//! provider seam. Owns TCP streams, listeners and UDP sockets; byte buffers
//! are read and written through the core's pointer regions.

#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::unused_self
)] // Moved verbatim from the core (bucket 0.5.1d); one arm per HOST.Net member.

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{
    address_value, endpoint_value, index_out_of_bounds_pub as index_out_of_bounds,
    integer_from_i128_count_pub as integer_from_i128_count, integer_pub as integer, net_address,
    net_addresses, net_endpoint, ping_reply_value, require_arity_pub as require_arity,
    runtime_error_pub as runtime_error, type_mismatch,
};
use crate::types::IntegerType;

pub const NAME: &str = "Net";

pub struct NetProvider {
    tcp_streams: HashMap<u64, crate::net::TcpStream>,
    next_tcp_stream: u64,
    tcp_listeners: HashMap<u64, Vec<crate::net::TcpListener>>,
    next_tcp_listener: u64,
    udp_sockets: HashMap<u64, crate::net::UdpSocket>,
    next_udp_socket: u64,
}

impl Default for NetProvider {
    fn default() -> Self {
        Self {
            tcp_streams: HashMap::new(),
            next_tcp_stream: 1,
            tcp_listeners: HashMap::new(),
            next_tcp_listener: 1,
            udp_sockets: HashMap::new(),
            next_udp_socket: 1,
        }
    }
}

impl Provider for NetProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("HOST.Net.{member}");
        self.host_net_call(core, &name, &arguments, span)
    }
}

impl NetProvider {
    fn host_net_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if name.starts_with("HOST.Net.Address.")
            || name.starts_with("HOST.Net.Endpoint.")
            || name.starts_with("HOST.Net.CIDR.")
            || name.starts_with("HOST.Net.PingReply.")
            || matches!(
                name,
                "HOST.Net.Ping" | "HOST.Net.Neighbor" | "HOST.Net.Reverse"
            )
        {
            return self.host_net_address_call(core, name, arguments, span);
        }
        if name.starts_with("HOST.Net.TCP") || name == "HOST.Net.Resolve" {
            return self.host_net_tcp_call(core, name, arguments, span);
        }
        match name {
            "HOST.Net.UDPBind" => {
                require_arity(name, arguments, 1, span)?;
                let endpoint = net_endpoint(&arguments[0], span)?;
                if self.tcp_streams.len()
                    + self.udp_sockets.len()
                    + self.tcp_listeners.values().map(Vec::len).sum::<usize>()
                    >= crate::config::web_limits().socket_handles_max
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "socket handle quota exceeded".into(),
                    });
                }
                match crate::net::UdpSocket::bind(endpoint) {
                    Ok(socket) => {
                        let id = self.next_udp_socket;
                        self.next_udp_socket += 1;
                        self.udp_sockets.insert(id, socket);
                        Ok(Value::UdpSocket(id))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.Net.UDPSocket.SendTo" => {
                require_arity(name, arguments, 4, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "UDPSocket",
                        "non-UDPSocket value",
                        "HOST.Net.UDPSocket.SendTo",
                        span,
                    ));
                };
                let endpoint = net_endpoint(&arguments[1], span)?;
                let Value::Pointer { handle } = arguments[2] else {
                    return Err(type_mismatch(
                        "BYTE buffer",
                        "non-pointer value",
                        "HOST.Net.UDPSocket.SendTo buffer",
                        span,
                    ));
                };
                let (count, _) = integer(&arguments[3], span)?;
                let capacity = core.memory().len(handle, span)?;
                let count = usize::try_from(count)
                    .ok()
                    .filter(|value| {
                        *value <= capacity
                            && *value <= crate::config::web_limits().datagram_max_bytes
                    })
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::LIMIT,
                            "datagram exceeds buffer or configured limit",
                            span,
                        )
                    })?;
                let bytes = (0..count)
                    .map(|index| {
                        let (value, _) = integer(core.memory().get(handle, index, span)?, span)?;
                        u8::try_from(value).map_err(|_| {
                            type_mismatch(
                                "BYTE",
                                "non-BYTE value",
                                "HOST.Net.UDPSocket.SendTo buffer element",
                                span,
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let sent = self
                    .udp_sockets
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "UDP socket is invalid",
                            span,
                        )
                    })?
                    .send_to(endpoint, &bytes)
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                Ok(Value::Integer(
                    i128::try_from(sent).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.UDPSocket.Receive" => {
                require_arity(name, arguments, 3, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "UDPSocket",
                        "non-UDPSocket value",
                        "HOST.Net.UDPSocket.Receive",
                        span,
                    ));
                };
                let (maximum, _) = integer(&arguments[1], span)?;
                let (timeout, _) = integer(&arguments[2], span)?;
                let maximum = usize::try_from(maximum)
                    .ok()
                    .filter(|value| *value <= crate::config::web_limits().datagram_max_bytes)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::LIMIT,
                            "receive exceeds configured limit",
                            span,
                        )
                    })?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "receive timeout is outside 1..60000 ms".into(),
                    });
                }
                let socket = self.udp_sockets.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                        "UDP socket is invalid",
                        span,
                    )
                })?;
                socket
                    .set_read_timeout(Some(std::time::Duration::from_millis(timeout as u64)))
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                match socket.receive(maximum) {
                    Ok(packet) => Ok(Value::Record {
                        type_name: "HOST.Net.UDPPacket".into(),
                        fields: HashMap::from([
                            ("source".into(), endpoint_value(packet.source())),
                            (
                                "bytes".into(),
                                Value::Vector(
                                    packet
                                        .bytes()
                                        .iter()
                                        .map(|byte| {
                                            Value::Integer(i128::from(*byte), IntegerType::Byte)
                                        })
                                        .collect(),
                                ),
                            ),
                            ("truncated".into(), Value::Boolean(packet.truncated())),
                        ]),
                    }),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.Net.UDPPacket.Source" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "UDPPacket",
                        "non-UDPPacket value",
                        "HOST.Net.UDPPacket.Source",
                        span,
                    ));
                };
                if type_name != "HOST.Net.UDPPacket" {
                    return Err(type_mismatch(
                        "UDPPacket",
                        type_name,
                        "HOST.Net.UDPPacket.Source",
                        span,
                    ));
                }
                fields.get("source").cloned().ok_or_else(|| {
                    type_mismatch(
                        "UDPPacket.source field",
                        "missing field",
                        "HOST.Net.UDPPacket.Source",
                        span,
                    )
                })
            }
            "HOST.Net.UDPPacket.Size" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { fields, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "UDPPacket",
                        "non-UDPPacket value",
                        "HOST.Net.UDPPacket.Size",
                        span,
                    ));
                };
                let Some(Value::Vector(bytes)) = fields.get("bytes") else {
                    return Err(type_mismatch(
                        "UDPPacket.bytes vector",
                        "missing or invalid field",
                        "HOST.Net.UDPPacket.Size",
                        span,
                    ));
                };
                integer_from_i128_count(bytes.len() as i128, span)
            }
            "HOST.Net.UDPPacket.Truncated" | "HOST.Net.UDPPacket.WasTruncated" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { fields, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "UDPPacket",
                        "non-UDPPacket value",
                        "HOST.Net.UDPPacket.Truncated",
                        span,
                    ));
                };
                fields.get("truncated").cloned().ok_or_else(|| {
                    type_mismatch(
                        "UDPPacket.truncated field",
                        "missing field",
                        "HOST.Net.UDPPacket.Truncated",
                        span,
                    )
                })
            }
            "HOST.Net.UDPPacket.CopyTo" => {
                require_arity(name, arguments, 3, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "UDPPacket",
                        "non-UDPPacket value",
                        "HOST.Net.UDPPacket.CopyTo",
                        span,
                    ));
                };
                if type_name != "HOST.Net.UDPPacket" {
                    return Err(type_mismatch(
                        "UDPPacket",
                        type_name,
                        "HOST.Net.UDPPacket.CopyTo",
                        span,
                    ));
                }
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(type_mismatch(
                        "BYTE buffer",
                        "non-pointer value",
                        "HOST.Net.UDPPacket.CopyTo buffer",
                        span,
                    ));
                };
                let (maximum, _) = integer(&arguments[2], span)?;
                let Some(Value::Vector(bytes)) = fields.get("bytes") else {
                    return Err(type_mismatch(
                        "UDPPacket.bytes vector",
                        "missing or invalid field",
                        "HOST.Net.UDPPacket.CopyTo",
                        span,
                    ));
                };
                let capacity = core.memory().len(handle, span)?;
                let maximum = usize::try_from(maximum)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::LIMIT,
                            "copy exceeds buffer or 1 MiB",
                            span,
                        )
                    })?;
                let count = bytes.len().min(maximum);
                for (index, byte) in bytes.iter().take(count).enumerate() {
                    *core.memory_mut().get_mut(handle, index, span)? = byte.clone();
                }
                Ok(Value::Integer(
                    i128::try_from(count).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.UDPSocket.Close" => {
                require_arity(name, arguments, 1, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "UDPSocket",
                        "non-UDPSocket value",
                        "HOST.Net.UDPSocket.Close",
                        span,
                    ));
                };
                self.udp_sockets.remove(&id);
                Ok(Value::Null)
            }
            "HOST.Net.UDPSocket.LocalEndpoint" => {
                require_arity(name, arguments, 1, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "UDPSocket",
                        "non-UDPSocket value",
                        "HOST.Net.UDPSocket.LocalEndpoint",
                        span,
                    ));
                };
                let endpoint = self
                    .udp_sockets
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "UDP socket is invalid",
                            span,
                        )
                    })?
                    .local_endpoint()
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                Ok(endpoint_value(endpoint))
            }
            "HOST.Net.Addresses.Count" => {
                require_arity(name, arguments, 1, span)?;
                let values = net_addresses(&arguments[0], span)?;
                integer_from_i128_count(values.len() as i128, span)
            }
            "HOST.Net.Addresses.Get" => {
                require_arity(name, arguments, 2, span)?;
                let values = net_addresses(&arguments[0], span)?;
                let (index, _) = integer(&arguments[1], span)?;
                let index = usize::try_from(index).map_err(|_| {
                    index_out_of_bounds("negative", "0", "HOST.Net.Addresses", span)
                })?;
                values.get(index).cloned().ok_or_else(|| {
                    index_out_of_bounds(index, values.len(), "HOST.Net.Addresses", span)
                })
            }

            _ => Err(runtime_error(
                crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }

    fn host_net_address_call(
        &mut self,
        _core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name {
            "HOST.Net.Address.Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.Net.Address.Parse",
                        span,
                    ));
                };
                match crate::net::Address::parse(text) {
                    Ok(address) => Ok(Value::Record {
                        type_name: "HOST.Net.Address".into(),
                        fields: HashMap::from([(
                            "value".into(),
                            Value::String(address.to_string()),
                        )]),
                    }),
                    Err(_) => Ok(Value::Error {
                        code: 1,
                        message: "invalid IP address".into(),
                    }),
                }
            }
            "HOST.Net.Address.ToString" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { fields, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.Address",
                        "non-address value",
                        "HOST.Net.Address.ToString",
                        span,
                    ));
                };
                let Some(Value::String(value)) = fields.get("value") else {
                    return Err(type_mismatch(
                        "Address.value STRING",
                        "missing or invalid field",
                        "HOST.Net.Address.ToString",
                        span,
                    ));
                };
                Ok(Value::String(value.clone()))
            }
            "HOST.Net.Address.IsIPv4"
            | "HOST.Net.Address.IsIPv6"
            | "HOST.Net.Address.IsLoopback"
            | "HOST.Net.Address.IsPrivate"
            | "HOST.Net.Address.IsLinkLocal"
            | "HOST.Net.Address.IsMulticast" => {
                require_arity(name, arguments, 1, span)?;
                let address = net_address(&arguments[0], span)?;
                let value = match name.rsplit('.').next().unwrap_or_default() {
                    "IsIPv4" => address.as_std().is_ipv4(),
                    "IsIPv6" => address.as_std().is_ipv6(),
                    "IsLoopback" => match address.as_std() {
                        std::net::IpAddr::V4(value) => value.is_loopback(),
                        std::net::IpAddr::V6(value) => {
                            value.is_loopback()
                                || value
                                    .to_ipv4_mapped()
                                    .is_some_and(|mapped| mapped.is_loopback())
                        }
                    },
                    "IsPrivate" => match address.as_std() {
                        std::net::IpAddr::V4(value) => value.is_private(),
                        std::net::IpAddr::V6(value) => {
                            value.is_unique_local()
                                || value
                                    .to_ipv4_mapped()
                                    .is_some_and(|mapped| mapped.is_private())
                        }
                    },
                    "IsLinkLocal" => match address.as_std() {
                        std::net::IpAddr::V4(value) => value.is_link_local(),
                        std::net::IpAddr::V6(value) => value.is_unicast_link_local(),
                    },
                    "IsMulticast" => address.as_std().is_multicast(),
                    _ => unreachable!("matched address predicate"),
                };
                Ok(Value::Boolean(value))
            }
            "HOST.Net.Endpoint.Create" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Record { type_name, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.Address",
                        "non-address value",
                        "HOST.Net.Endpoint.Create",
                        span,
                    ));
                };
                if type_name != "HOST.Net.Address" {
                    return Err(type_mismatch(
                        "HOST.Net.Address",
                        type_name,
                        "HOST.Net.Endpoint.Create",
                        span,
                    ));
                }
                let (port, _) = integer(&arguments[1], span)?;
                let port = u16::try_from(port).map_err(|_| {
                    runtime_error(
                        crate::diagnostic::DiagId::INVALID_INPUT,
                        "port is outside 0..65535",
                        span,
                    )
                })?;
                Ok(Value::Record {
                    type_name: "HOST.Net.Endpoint".into(),
                    fields: HashMap::from([
                        ("address".into(), arguments[0].clone()),
                        (
                            "port".into(),
                            Value::Integer(i128::from(port), IntegerType::UInt16),
                        ),
                    ]),
                })
            }
            "HOST.Net.Endpoint.Address" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.Endpoint",
                        "non-endpoint value",
                        "HOST.Net.Endpoint.Address",
                        span,
                    ));
                };
                if type_name != "HOST.Net.Endpoint" {
                    return Err(type_mismatch(
                        "HOST.Net.Endpoint",
                        type_name,
                        "HOST.Net.Endpoint.Address",
                        span,
                    ));
                }
                fields.get("address").cloned().ok_or_else(|| {
                    type_mismatch(
                        "Endpoint.address field",
                        "missing field",
                        "HOST.Net.Endpoint.Address",
                        span,
                    )
                })
            }
            "HOST.Net.Endpoint.Port" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.Endpoint",
                        "non-endpoint value",
                        "HOST.Net.Endpoint.Port",
                        span,
                    ));
                };
                if type_name != "HOST.Net.Endpoint" {
                    return Err(type_mismatch(
                        "HOST.Net.Endpoint",
                        type_name,
                        "HOST.Net.Endpoint.Port",
                        span,
                    ));
                }
                fields.get("port").cloned().ok_or_else(|| {
                    type_mismatch(
                        "Endpoint.port field",
                        "missing field",
                        "HOST.Net.Endpoint.Port",
                        span,
                    )
                })
            }
            "HOST.Net.CIDR.Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.Net.CIDR.Parse",
                        span,
                    ));
                };
                match crate::net::Cidr::parse(text) {
                    Ok(cidr) => Ok(Value::Record {
                        type_name: "HOST.Net.CIDR".into(),
                        fields: HashMap::from([
                            ("network".into(), Value::String(cidr.network().to_string())),
                            (
                                "prefix".into(),
                                Value::Integer(
                                    i128::from(cidr.prefix_length()),
                                    IntegerType::Int32,
                                ),
                            ),
                        ]),
                    }),
                    Err(message) => Ok(Value::Error {
                        code: 1,
                        message: message.into(),
                    }),
                }
            }
            "HOST.Net.CIDR.Contains" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.CIDR",
                        "non-CIDR value",
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                };
                if type_name != "HOST.Net.CIDR" {
                    return Err(type_mismatch(
                        "HOST.Net.CIDR",
                        type_name,
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                }
                let Some(Value::String(network)) = fields.get("network") else {
                    return Err(type_mismatch(
                        "CIDR.network STRING",
                        "missing or invalid field",
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                };
                let Some(Value::Integer(prefix, _)) = fields.get("prefix") else {
                    return Err(type_mismatch(
                        "CIDR.prefix INTEGER",
                        "missing or invalid field",
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                };
                let Value::Record {
                    type_name: address_type,
                    fields: address_fields,
                } = &arguments[1]
                else {
                    return Err(type_mismatch(
                        "HOST.Net.Address",
                        "non-address value",
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                };
                if address_type != "HOST.Net.Address" {
                    return Err(type_mismatch(
                        "HOST.Net.Address",
                        address_type,
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                }
                let Some(Value::String(address)) = address_fields.get("value") else {
                    return Err(type_mismatch(
                        "Address.value STRING",
                        "missing or invalid field",
                        "HOST.Net.CIDR.Contains",
                        span,
                    ));
                };
                let cidr =
                    crate::net::Cidr::parse(&format!("{network}/{prefix}")).map_err(|message| {
                        runtime_error(crate::diagnostic::DiagId::INVALID_VALUE, message, span)
                    })?;
                let address = crate::net::Address::parse(address).map_err(|_| {
                    runtime_error(
                        crate::diagnostic::DiagId::INVALID_VALUE,
                        "invalid Address value",
                        span,
                    )
                })?;
                Ok(Value::Boolean(cidr.contains(address)))
            }
            "HOST.Net.CIDR.Network" | "HOST.Net.CIDR.PrefixLength" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.CIDR",
                        "non-CIDR value",
                        "HOST.Net.CIDR accessor",
                        span,
                    ));
                };
                if type_name != "HOST.Net.CIDR" {
                    return Err(type_mismatch(
                        "HOST.Net.CIDR",
                        type_name,
                        "HOST.Net.CIDR accessor",
                        span,
                    ));
                }
                if name.ends_with(".Network") {
                    let Some(Value::String(network)) = fields.get("network") else {
                        return Err(type_mismatch(
                            "CIDR.network STRING",
                            "missing or invalid field",
                            "HOST.Net.CIDR.Network",
                            span,
                        ));
                    };
                    Ok(Value::Record {
                        type_name: "HOST.Net.Address".into(),
                        fields: HashMap::from([("value".into(), Value::String(network.clone()))]),
                    })
                } else {
                    fields.get("prefix").cloned().ok_or_else(|| {
                        type_mismatch(
                            "CIDR.prefix INTEGER",
                            "missing or invalid field",
                            "HOST.Net.CIDR.PrefixLength",
                            span,
                        )
                    })
                }
            }
            "HOST.Net.Ping" => {
                require_arity(name, arguments, 2, span)?;
                let address = net_address(&arguments[0], span)?;
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "ping timeout is outside 1..60000 ms".into(),
                    });
                }
                match crate::net::ping(address, std::time::Duration::from_millis(timeout as u64)) {
                    Ok(reply) => Ok(ping_reply_value(reply)),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.message(),
                    }),
                }
            }
            "HOST.Net.Neighbor" => {
                require_arity(name, arguments, 1, span)?;
                let address = net_address(&arguments[0], span)?;
                match crate::net::neighbor(address) {
                    Ok(neighbor) => Ok(address_value(neighbor.as_std())),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.message(),
                    }),
                }
            }
            "HOST.Net.Reverse" => {
                require_arity(name, arguments, 2, span)?;
                let address = net_address(&arguments[0], span)?;
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "reverse timeout is outside 1..60000 ms".into(),
                    });
                }
                match crate::net::reverse_timeout(
                    address,
                    std::time::Duration::from_millis(timeout as u64),
                ) {
                    Ok(name) => Ok(Value::String(name)),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.message(),
                    }),
                }
            }
            "HOST.Net.PingReply.Address" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.PingReply",
                        "non-PingReply value",
                        "HOST.Net.PingReply.Address",
                        span,
                    ));
                };
                if type_name != "HOST.Net.PingReply" {
                    return Err(type_mismatch(
                        "HOST.Net.PingReply",
                        type_name,
                        "HOST.Net.PingReply.Address",
                        span,
                    ));
                }
                fields.get("address").cloned().ok_or_else(|| {
                    type_mismatch(
                        "PingReply.address field",
                        "missing field",
                        "HOST.Net.PingReply.Address",
                        span,
                    )
                })
            }
            "HOST.Net.PingReply.RoundTripMicroseconds" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(type_mismatch(
                        "HOST.Net.PingReply",
                        "non-PingReply value",
                        "HOST.Net.PingReply.RoundTripMicroseconds",
                        span,
                    ));
                };
                if type_name != "HOST.Net.PingReply" {
                    return Err(type_mismatch(
                        "HOST.Net.PingReply",
                        type_name,
                        "HOST.Net.PingReply.RoundTripMicroseconds",
                        span,
                    ));
                }
                fields.get("roundTripMicroseconds").cloned().ok_or_else(|| {
                    type_mismatch(
                        "PingReply.roundTripMicroseconds field",
                        "missing field",
                        "HOST.Net.PingReply.RoundTripMicroseconds",
                        span,
                    )
                })
            }

            _ => Err(runtime_error(
                crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }

    fn host_net_tcp_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match name {
            "HOST.Net.TCPListen" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Vector(endpoints) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Endpoint[]",
                        "non-vector value",
                        "TCPListen endpoints",
                        span,
                    ));
                };
                let (backlog, _) = integer(&arguments[1], span)?;
                if !(1..=128).contains(&backlog) || endpoints.is_empty() || endpoints.len() > 16 {
                    return Ok(Value::Error {
                        code: 1,
                        message: "invalid listener endpoints or backlog".into(),
                    });
                }
                let mut listeners = Vec::with_capacity(endpoints.len());
                for endpoint in endpoints {
                    if self.tcp_listeners.values().map(Vec::len).sum::<usize>() + listeners.len()
                        >= crate::config::web_limits().socket_handles_max
                    {
                        return Ok(Value::Error {
                            code: 1,
                            message: "socket handle quota exceeded".into(),
                        });
                    }
                    match crate::net::TcpListener::bind_with_backlog(
                        net_endpoint(endpoint, span)?,
                        usize::try_from(backlog).expect("validated backlog is positive"),
                    ) {
                        Ok(listener) => listeners.push(listener),
                        Err(error) => {
                            return Ok(Value::Error {
                                code: 1,
                                message: error.to_string(),
                            });
                        }
                    }
                }
                let id = self.next_tcp_listener;
                self.next_tcp_listener += 1;
                self.tcp_listeners.insert(id, listeners);
                Ok(Value::TcpListener(id))
            }
            "HOST.Net.Resolve" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(host) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "Net.Resolve host",
                        span,
                    ));
                };
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "resolver timeout is outside 1..60000 ms".into(),
                    });
                }
                match crate::net::resolve_timeout(
                    host,
                    0,
                    crate::config::web_limits().resolved_addresses_max,
                    std::time::Duration::from_millis(timeout as u64),
                ) {
                    Ok(Some(addresses)) => Ok(Value::Record {
                        type_name: "HOST.Net.Addresses".into(),
                        fields: HashMap::from([(
                            "values".into(),
                            Value::Vector(
                                addresses
                                    .into_iter()
                                    .map(|address| Value::Record {
                                        type_name: "HOST.Net.Address".into(),
                                        fields: HashMap::from([(
                                            "value".into(),
                                            Value::String(address.to_string()),
                                        )]),
                                    })
                                    .collect(),
                            ),
                        )]),
                    }),
                    Ok(None) => Ok(Value::Error {
                        code: 1,
                        message: "resolver timeout".into(),
                    }),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.Net.TCPConnect" => {
                require_arity(name, arguments, 2, span)?;
                let endpoint = net_endpoint(&arguments[0], span)?;
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "connect timeout is outside 1..60000 ms".into(),
                    });
                }
                match crate::net::TcpStream::connect(
                    endpoint,
                    std::time::Duration::from_millis(timeout as u64),
                ) {
                    Ok(stream) => {
                        if self.tcp_streams.len()
                            + self.udp_sockets.len()
                            + self.tcp_listeners.values().map(Vec::len).sum::<usize>()
                            >= crate::config::web_limits().socket_handles_max
                        {
                            return Ok(Value::Error {
                                code: 1,
                                message: "socket handle quota exceeded".into(),
                            });
                        }
                        stream
                            .set_timeouts(
                                Some(std::time::Duration::from_millis(timeout as u64)),
                                Some(std::time::Duration::from_millis(timeout as u64)),
                            )
                            .map_err(|error| {
                                runtime_error(
                                    crate::diagnostic::DiagId::IO,
                                    error.to_string(),
                                    span,
                                )
                            })?;
                        let id = self.next_tcp_stream;
                        self.next_tcp_stream += 1;
                        self.tcp_streams.insert(id, stream);
                        Ok(Value::TcpStream(id))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: error.to_string(),
                    }),
                }
            }
            "HOST.Net.TCPStream.Close" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPStream",
                        "non-TCPStream value",
                        "TCPStream.Close",
                        span,
                    ));
                };
                self.tcp_streams.remove(&id);
                Ok(Value::Null)
            }
            "HOST.Net.TCPStream.Read" => {
                require_arity(name, arguments, 3, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPStream",
                        "non-TCPStream value",
                        "TCPStream.Read",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(type_mismatch(
                        "BYTE buffer pointer",
                        "non-pointer value",
                        "TCPStream.Read",
                        span,
                    ));
                };
                let (maximum, _) = integer(&arguments[2], span)?;
                let capacity = core.memory().len(handle, span)?;
                let maximum = usize::try_from(maximum)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::LIMIT,
                            "read exceeds buffer or 1 MiB",
                            span,
                        )
                    })?;
                let mut bytes = vec![0; maximum];
                let count = self
                    .tcp_streams
                    .get_mut(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "TCP stream is invalid",
                            span,
                        )
                    })?
                    .read_bounded(&mut bytes)
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                if count == 0 {
                    return Ok(Value::EndOfFile);
                }
                for (index, byte) in bytes.into_iter().take(count).enumerate() {
                    *core.memory_mut().get_mut(handle, index, span)? =
                        Value::Integer(i128::from(byte), IntegerType::Byte);
                }
                Ok(Value::Integer(
                    i128::try_from(count).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.TCPStream.Write" => {
                require_arity(name, arguments, 3, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPStream",
                        "non-TCPStream value",
                        "TCPStream.Write receiver",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(type_mismatch(
                        "BYTE buffer pointer",
                        "non-pointer value",
                        "TCPStream.Write buffer",
                        span,
                    ));
                };
                let (count, _) = integer(&arguments[2], span)?;
                let capacity = core.memory().len(handle, span)?;
                let count = usize::try_from(count)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::LIMIT,
                            "write exceeds buffer or 1 MiB",
                            span,
                        )
                    })?;
                let bytes = (0..count)
                    .map(|index| {
                        let value = core.memory().get(handle, index, span)?;
                        let (value, _) = integer(value, span)?;
                        u8::try_from(value).map_err(|_| {
                            type_mismatch("BYTE", "non-BYTE value", "TCPStream.Write buffer", span)
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let written = self
                    .tcp_streams
                    .get_mut(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "TCP stream is invalid",
                            span,
                        )
                    })?
                    .write_bounded(&bytes)
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                Ok(Value::Integer(
                    i128::try_from(written).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.TCPStream.LocalEndpoint" | "HOST.Net.TCPStream.RemoteEndpoint" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPStream",
                        "non-TCPStream value",
                        "TCPStream endpoint",
                        span,
                    ));
                };
                let stream = self.tcp_streams.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                        "TCP stream is invalid",
                        span,
                    )
                })?;
                let endpoint = if name.ends_with("LocalEndpoint") {
                    stream.local_endpoint()
                } else {
                    stream.remote_endpoint()
                }
                .map_err(|error| {
                    runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                })?;
                Ok(endpoint_value(endpoint))
            }
            "HOST.Net.TCPStream.SetTimeouts" => {
                require_arity(name, arguments, 3, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPStream",
                        "non-TCPStream value",
                        "TCPStream.SetTimeouts",
                        span,
                    ));
                };
                let (read_ms, _) = integer(&arguments[1], span)?;
                let (write_ms, _) = integer(&arguments[2], span)?;
                if !(1..=60_000).contains(&read_ms) || !(1..=60_000).contains(&write_ms) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "socket timeout is outside 1..60000 ms".into(),
                    });
                }
                self.tcp_streams
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "TCP stream is invalid",
                            span,
                        )
                    })?
                    .set_timeouts(
                        Some(std::time::Duration::from_millis(read_ms as u64)),
                        Some(std::time::Duration::from_millis(write_ms as u64)),
                    )
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                Ok(Value::Null)
            }
            "HOST.Net.TCPListener.LocalEndpoint" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpListener(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPListener",
                        "non-TCPListener value",
                        "TCPListener.LocalEndpoint",
                        span,
                    ));
                };
                let listener = self.tcp_listeners.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                        "TCP listener is invalid",
                        span,
                    )
                })?;
                let endpoint = listener
                    .first()
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::IO,
                            "listener has no endpoints",
                            span,
                        )
                    })?
                    .local_endpoint()
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                Ok(endpoint_value(endpoint))
            }
            "HOST.Net.TCPStream.ShutdownRead" | "HOST.Net.TCPStream.ShutdownWrite" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPStream",
                        "non-TCPStream value",
                        "TCPStream.Shutdown",
                        span,
                    ));
                };
                let direction = if name.ends_with("ShutdownRead") {
                    std::net::Shutdown::Read
                } else {
                    std::net::Shutdown::Write
                };
                self.tcp_streams
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "TCP stream is invalid",
                            span,
                        )
                    })?
                    .shutdown(direction)
                    .map_err(|error| {
                        runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                    })?;
                Ok(Value::Null)
            }
            "HOST.Net.TCPListener.Accept" => {
                require_arity(name, arguments, 2, span)?;
                let Value::TcpListener(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPListener",
                        "non-TCPListener value",
                        "TCPListener.Accept",
                        span,
                    ));
                };
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "accept timeout is outside 1..60000 ms".into(),
                    });
                }
                let listeners = self
                    .tcp_listeners
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::USE_AFTER_RELEASE,
                            "TCP listener is invalid",
                            span,
                        )
                    })?
                    .as_slice();
                let mut stream = None;
                let accept_timeout = std::time::Duration::from_millis(timeout as u64);
                for listener in listeners {
                    if let Some(accepted) =
                        listener.accept_timeout(accept_timeout).map_err(|error| {
                            runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                        })?
                    {
                        stream = Some(accepted);
                        break;
                    }
                }
                let Some(stream) = stream else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "accept timeout".into(),
                    });
                };
                if self.tcp_streams.len()
                    + self.udp_sockets.len()
                    + self.tcp_listeners.values().map(Vec::len).sum::<usize>()
                    >= crate::config::web_limits().socket_handles_max
                {
                    return Ok(Value::Error {
                        code: 1,
                        message: "socket handle quota exceeded".into(),
                    });
                }
                let _ = stream.set_timeouts(Some(accept_timeout), Some(accept_timeout));
                let stream_id = self.next_tcp_stream;
                self.next_tcp_stream += 1;
                self.tcp_streams.insert(stream_id, stream);
                Ok(Value::TcpStream(stream_id))
            }
            "HOST.Net.TCPListener.Close" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpListener(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "TCPListener",
                        "non-TCPListener value",
                        "TCPListener.Close",
                        span,
                    ));
                };
                self.tcp_listeners.remove(&id);
                Ok(Value::Null)
            }

            _ => Err(runtime_error(
                crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
