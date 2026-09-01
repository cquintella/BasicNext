#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn host_net_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        if name.starts_with("HOST.Net.Address.") || name.starts_with("HOST.Net.Endpoint.") || name.starts_with("HOST.Net.CIDR.") || matches!(name, "HOST.Net.Ping" | "HOST.Net.Neighbor" | "HOST.Net.Reverse") {
            return self.host_net_address_call(name, arguments, span);
        }
        if name.starts_with("HOST.Net.TCP") || name == "HOST.Net.Resolve" {
            return self.host_net_tcp_call(name, arguments, span);
        }
        match name {
            "HOST.Net.UDPBind" => {
                require_arity(name, arguments, 1, span)?;
                let endpoint = net_endpoint(&arguments[0], span)?;
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "SendTo expects UDPSocket",
                        span,
                    ));
                };
                let endpoint = net_endpoint(&arguments[1], span)?;
                let Value::Pointer { handle } = arguments[2] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "SendTo expects BYTE buffer",
                        span,
                    ));
                };
                let (count, _) = integer(&arguments[3], span)?;
                let capacity = self.memory.len(handle, span)?;
                let count = usize::try_from(count)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| {
                        runtime_error("LIMIT", "datagram exceeds buffer or 1 MiB", span)
                    })?;
                let bytes = (0..count)
                    .map(|index| {
                        let (value, _) = integer(self.memory.get(handle, index, span)?, span)?;
                        u8::try_from(value)
                            .map_err(|_| runtime_error("TYPE_MISMATCH", "buffer is not BYTE", span))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let sent = self
                    .udp_sockets
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "UDP socket is invalid", span)
                    })?
                    .send_to(endpoint, &bytes)
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                Ok(Value::Integer(
                    i128::try_from(sent).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.UDPSocket.Receive" => {
                require_arity(name, arguments, 3, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Receive expects UDPSocket",
                        span,
                    ));
                };
                let (maximum, _) = integer(&arguments[1], span)?;
                let (timeout, _) = integer(&arguments[2], span)?;
                let maximum = usize::try_from(maximum)
                    .ok()
                    .filter(|value| *value <= 1_048_576)
                    .ok_or_else(|| runtime_error("LIMIT", "receive exceeds 1 MiB", span))?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "receive timeout is outside 1..60000 ms".into(),
                    });
                }
                let socket = self.udp_sockets.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "UDP socket is invalid", span)
                })?;
                socket
                    .set_read_timeout(Some(std::time::Duration::from_millis(timeout as u64)))
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Source expects UDPPacket",
                        span,
                    ));
                };
                if type_name != "HOST.Net.UDPPacket" {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid UDPPacket value",
                        span,
                    ));
                }
                fields
                    .get("source")
                    .cloned()
                    .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid UDPPacket value", span))
            }
            "HOST.Net.UDPPacket.Size" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { fields, .. } = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Size expects UDPPacket",
                        span,
                    ));
                };
                let Some(Value::Vector(bytes)) = fields.get("bytes") else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid UDPPacket value",
                        span,
                    ));
                };
                integer_from_i128_count(bytes.len() as i128, span)
            }
            "HOST.Net.UDPPacket.Truncated" | "HOST.Net.UDPPacket.WasTruncated" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { fields, .. } = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Truncated expects UDPPacket",
                        span,
                    ));
                };
                fields
                    .get("truncated")
                    .cloned()
                    .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid UDPPacket value", span))
            }
            "HOST.Net.UDPPacket.CopyTo" => {
                require_arity(name, arguments, 3, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "CopyTo expects UDPPacket",
                        span,
                    ));
                };
                if type_name != "HOST.Net.UDPPacket" {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid UDPPacket value",
                        span,
                    ));
                }
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "CopyTo expects BYTE buffer",
                        span,
                    ));
                };
                let (maximum, _) = integer(&arguments[2], span)?;
                let Some(Value::Vector(bytes)) = fields.get("bytes") else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid UDPPacket value",
                        span,
                    ));
                };
                let capacity = self.memory.len(handle, span)?;
                let maximum = usize::try_from(maximum)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| runtime_error("LIMIT", "copy exceeds buffer or 1 MiB", span))?;
                let count = bytes.len().min(maximum);
                for (index, byte) in bytes.iter().take(count).enumerate() {
                    *self.memory.get_mut(handle, index, span)? = byte.clone();
                }
                Ok(Value::Integer(
                    i128::try_from(count).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.UDPSocket.Close" => {
                require_arity(name, arguments, 1, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Close expects UDPSocket",
                        span,
                    ));
                };
                self.udp_sockets.remove(&id);
                Ok(Value::Null)
            }
            "HOST.Net.UDPSocket.LocalEndpoint" => {
                require_arity(name, arguments, 1, span)?;
                let Value::UdpSocket(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "LocalEndpoint expects UDPSocket",
                        span,
                    ));
                };
                let endpoint = self
                    .udp_sockets
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "UDP socket is invalid", span)
                    })?
                    .local_endpoint()
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
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
                    runtime_error("INDEX_OUT_OF_BOUNDS", "address index is negative", span)
                })?;
                values.get(index).cloned().ok_or_else(|| {
                    runtime_error(
                        "INDEX_OUT_OF_BOUNDS",
                        "address index is outside the result",
                        span,
                    )
                })
            }

            _ => Err(runtime_error(
                "HOST_CAPABILITY_UNAVAILABLE",
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }
}
