#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, clippy::unused_self)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn host_net_address_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        match name {


            "HOST.Net.Address.Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(super::type_mismatch("STRING", "non-STRING value", "HOST.Net.Address.Parse", span));
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
                    return Err(super::type_mismatch("HOST.Net.Address", "non-address value", "HOST.Net.Address.ToString", span));
                };
                let Some(Value::String(value)) = fields.get("value") else {
                    return Err(super::type_mismatch("Address.value STRING", "missing or invalid field", "HOST.Net.Address.ToString", span));
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
                                || value.to_ipv4_mapped().is_some_and(|mapped| mapped.is_loopback())
                        }
                    },
                    "IsPrivate" => match address.as_std() {
                        std::net::IpAddr::V4(value) => value.is_private(),
                        std::net::IpAddr::V6(value) => {
                            value.is_unique_local()
                                || value.to_ipv4_mapped().is_some_and(|mapped| mapped.is_private())
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
                    return Err(super::type_mismatch("HOST.Net.Address", "non-address value", "HOST.Net.Endpoint.Create", span));
                };
                if type_name != "HOST.Net.Address" {
                    return Err(super::type_mismatch("HOST.Net.Address", type_name, "HOST.Net.Endpoint.Create", span));
                }
                let (port, _) = integer(&arguments[1], span)?;
                let port = u16::try_from(port).map_err(|_| {
                    runtime_error(crate::diagnostic::DiagId::INVALID_INPUT, "port is outside 0..65535", span)
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
                    return Err(super::type_mismatch("HOST.Net.Endpoint", "non-endpoint value", "HOST.Net.Endpoint.Address", span));
                };
                if type_name != "HOST.Net.Endpoint" {
                    return Err(super::type_mismatch("HOST.Net.Endpoint", type_name, "HOST.Net.Endpoint.Address", span));
                }
                fields
                    .get("address")
                    .cloned()
                    .ok_or_else(|| super::type_mismatch("Endpoint.address field", "missing field", "HOST.Net.Endpoint.Address", span))
            }
            "HOST.Net.Endpoint.Port" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(super::type_mismatch("HOST.Net.Endpoint", "non-endpoint value", "HOST.Net.Endpoint.Port", span));
                };
                if type_name != "HOST.Net.Endpoint" {
                    return Err(super::type_mismatch("HOST.Net.Endpoint", type_name, "HOST.Net.Endpoint.Port", span));
                }
                fields
                    .get("port")
                    .cloned()
                    .ok_or_else(|| super::type_mismatch("Endpoint.port field", "missing field", "HOST.Net.Endpoint.Port", span))
            }
            "HOST.Net.CIDR.Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(super::type_mismatch("STRING", "non-STRING value", "HOST.Net.CIDR.Parse", span));
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
                    return Err(super::type_mismatch("HOST.Net.CIDR", "non-CIDR value", "HOST.Net.CIDR.Contains", span));
                };
                if type_name != "HOST.Net.CIDR" {
                    return Err(super::type_mismatch("HOST.Net.CIDR", type_name, "HOST.Net.CIDR.Contains", span));
                }
                let Some(Value::String(network)) = fields.get("network") else {
                    return Err(super::type_mismatch("CIDR.network STRING", "missing or invalid field", "HOST.Net.CIDR.Contains", span));
                };
                let Some(Value::Integer(prefix, _)) = fields.get("prefix") else {
                    return Err(super::type_mismatch("CIDR.prefix INTEGER", "missing or invalid field", "HOST.Net.CIDR.Contains", span));
                };
                let Value::Record {
                    type_name: address_type,
                    fields: address_fields,
                } = &arguments[1]
                else {
                    return Err(super::type_mismatch("HOST.Net.Address", "non-address value", "HOST.Net.CIDR.Contains", span));
                };
                if address_type != "HOST.Net.Address" {
                    return Err(super::type_mismatch("HOST.Net.Address", address_type, "HOST.Net.CIDR.Contains", span));
                }
                let Some(Value::String(address)) = address_fields.get("value") else {
                    return Err(super::type_mismatch("Address.value STRING", "missing or invalid field", "HOST.Net.CIDR.Contains", span));
                };
                let cidr = crate::net::Cidr::parse(&format!("{network}/{prefix}"))
                    .map_err(|message| runtime_error(crate::diagnostic::DiagId::INVALID_VALUE, message, span))?;
                let address = crate::net::Address::parse(address)
                    .map_err(|_| runtime_error(crate::diagnostic::DiagId::INVALID_VALUE, "invalid Address value", span))?;
                Ok(Value::Boolean(cidr.contains(address)))
            }
            "HOST.Net.CIDR.Network" | "HOST.Net.CIDR.PrefixLength" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(super::type_mismatch("HOST.Net.CIDR", "non-CIDR value", "HOST.Net.CIDR accessor", span));
                };
                if type_name != "HOST.Net.CIDR" {
                    return Err(super::type_mismatch("HOST.Net.CIDR", type_name, "HOST.Net.CIDR accessor", span));
                }
                if name.ends_with(".Network") {
                    let Some(Value::String(network)) = fields.get("network") else {
                        return Err(super::type_mismatch("CIDR.network STRING", "missing or invalid field", "HOST.Net.CIDR.Network", span));
                    };
                    Ok(Value::Record {
                        type_name: "HOST.Net.Address".into(),
                        fields: HashMap::from([("value".into(), Value::String(network.clone()))]),
                    })
                } else {
                    fields
                        .get("prefix")
                        .cloned()
                        .ok_or_else(|| super::type_mismatch("CIDR.prefix INTEGER", "missing or invalid field", "HOST.Net.CIDR.PrefixLength", span))
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
                match crate::net::ping(
                    address,
                    std::time::Duration::from_millis(timeout as u64),
                ) {
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
                    return Err(super::type_mismatch("HOST.Net.PingReply", "non-PingReply value", "HOST.Net.PingReply.Address", span));
                };
                if type_name != "HOST.Net.PingReply" {
                    return Err(super::type_mismatch("HOST.Net.PingReply", type_name, "HOST.Net.PingReply.Address", span));
                }
                fields.get("address").cloned().ok_or_else(|| super::type_mismatch("PingReply.address field", "missing field", "HOST.Net.PingReply.Address", span))
            }
            "HOST.Net.PingReply.RoundTripMicroseconds" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(super::type_mismatch("HOST.Net.PingReply", "non-PingReply value", "HOST.Net.PingReply.RoundTripMicroseconds", span));
                };
                if type_name != "HOST.Net.PingReply" {
                    return Err(super::type_mismatch("HOST.Net.PingReply", type_name, "HOST.Net.PingReply.RoundTripMicroseconds", span));
                }
                fields.get("roundTripMicroseconds").cloned().ok_or_else(|| super::type_mismatch("PingReply.roundTripMicroseconds field", "missing field", "HOST.Net.PingReply.RoundTripMicroseconds", span))
            }

            _ => Err(runtime_error(crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE, format!("host function '{name}' is not available"), span)),
        }
    }
}
