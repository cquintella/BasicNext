#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn host_net_tcp_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        match name {

            "HOST.Net.TCPListen" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Vector(endpoints) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "TCPListen expects Endpoint[]",
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
                // ponytail: std::net does not expose backlog; the OS default is used.
                let mut listeners = Vec::with_capacity(endpoints.len());
                for endpoint in endpoints {
                    match crate::net::TcpListener::bind(net_endpoint(endpoint, span)?) {
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Resolve expects STRING host",
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
                    64,
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Close expects TCPStream",
                        span,
                    ));
                };
                self.tcp_streams.remove(&id);
                Ok(Value::Null)
            }
            "HOST.Net.TCPStream.Read" => {
                require_arity(name, arguments, 3, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Read expects TCPStream",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Read expects BYTE buffer",
                        span,
                    ));
                };
                let (maximum, _) = integer(&arguments[2], span)?;
                let capacity = self.memory.len(handle, span)?;
                let maximum = usize::try_from(maximum)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| runtime_error("LIMIT", "read exceeds buffer or 1 MiB", span))?;
                let mut bytes = vec![0; maximum];
                let count = self
                    .tcp_streams
                    .get_mut(&id)
                    .ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "TCP stream is invalid", span)
                    })?
                    .read_bounded(&mut bytes)
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                if count == 0 {
                    return Ok(Value::EndOfFile);
                }
                for (index, byte) in bytes.into_iter().take(count).enumerate() {
                    *self.memory.get_mut(handle, index, span)? =
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Write expects TCPStream",
                        span,
                    ));
                };
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Write expects BYTE buffer",
                        span,
                    ));
                };
                let (count, _) = integer(&arguments[2], span)?;
                let capacity = self.memory.len(handle, span)?;
                let count = usize::try_from(count)
                    .ok()
                    .filter(|value| *value <= capacity && *value <= 1_048_576)
                    .ok_or_else(|| runtime_error("LIMIT", "write exceeds buffer or 1 MiB", span))?;
                let bytes = (0..count)
                    .map(|index| {
                        let value = self.memory.get(handle, index, span)?;
                        let (value, _) = integer(value, span)?;
                        u8::try_from(value)
                            .map_err(|_| runtime_error("TYPE_MISMATCH", "buffer is not BYTE", span))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let written = self
                    .tcp_streams
                    .get_mut(&id)
                    .ok_or_else(|| {
                        runtime_error("USE_AFTER_DELETE", "TCP stream is invalid", span)
                    })?
                    .write_bounded(&bytes)
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                Ok(Value::Integer(
                    i128::try_from(written).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "HOST.Net.TCPStream.LocalEndpoint" | "HOST.Net.TCPStream.RemoteEndpoint" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "endpoint inspection expects TCPStream",
                        span,
                    ));
                };
                let stream = self.tcp_streams.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "TCP stream is invalid", span)
                })?;
                let endpoint = if name.ends_with("LocalEndpoint") {
                    stream.local_endpoint()
                } else {
                    stream.remote_endpoint()
                }
                .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                Ok(endpoint_value(endpoint))
            }
            "HOST.Net.TCPStream.SetTimeouts" => {
                require_arity(name, arguments, 3, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "SetTimeouts expects TCPStream",
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
                        runtime_error("USE_AFTER_DELETE", "TCP stream is invalid", span)
                    })?
                    .set_timeouts(
                        Some(std::time::Duration::from_millis(read_ms as u64)),
                        Some(std::time::Duration::from_millis(write_ms as u64)),
                    )
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Net.TCPListener.LocalEndpoint" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpListener(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "endpoint inspection expects TCPListener",
                        span,
                    ));
                };
                let listener = self.tcp_listeners.get(&id).ok_or_else(|| {
                    runtime_error("USE_AFTER_DELETE", "TCP listener is invalid", span)
                })?;
                let endpoint = listener
                    .first()
                    .ok_or_else(|| runtime_error("IO", "listener has no endpoints", span))?
                    .local_endpoint()
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                Ok(endpoint_value(endpoint))
            }
            "HOST.Net.TCPStream.ShutdownRead" | "HOST.Net.TCPStream.ShutdownWrite" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpStream(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "shutdown expects TCPStream",
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
                        runtime_error("USE_AFTER_DELETE", "TCP stream is invalid", span)
                    })?
                    .shutdown(direction)
                    .map_err(|error| runtime_error("IO", error.to_string(), span))?;
                Ok(Value::Null)
            }
            "HOST.Net.TCPListener.Accept" => {
                require_arity(name, arguments, 2, span)?;
                let Value::TcpListener(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Accept expects TCPListener",
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
                        runtime_error("USE_AFTER_DELETE", "TCP listener is invalid", span)
                    })?
                    .as_slice();
                let deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(timeout as u64);
                let mut stream = None;
                while std::time::Instant::now() < deadline && stream.is_none() {
                    for listener in listeners {
                        if let Some(accepted) = listener
                            .accept_timeout(std::time::Duration::from_millis(1))
                            .map_err(|error| runtime_error("IO", error.to_string(), span))?
                        {
                            stream = Some(accepted);
                            break;
                        }
                    }
                }
                let Some(stream) = stream else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "accept timeout".into(),
                    });
                };
                let stream_id = self.next_tcp_stream;
                self.next_tcp_stream += 1;
                self.tcp_streams.insert(stream_id, stream);
                Ok(Value::TcpStream(stream_id))
            }
            "HOST.Net.TCPListener.Close" => {
                require_arity(name, arguments, 1, span)?;
                let Value::TcpListener(id) = arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Close expects TCPListener",
                        span,
                    ));
                };
                self.tcp_listeners.remove(&id);
                Ok(Value::Null)
            }

            _ => Err(runtime_error("HOST_CAPABILITY_UNAVAILABLE", format!("host function '{name}' is not available"), span)),
        }
    }
}
