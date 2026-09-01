#![allow(clippy::wildcard_imports, clippy::too_many_lines, clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss, clippy::unused_self)]
use super::*;

impl Executor<'_, '_> {
pub(crate) fn host_net_address_call(&mut self, name: &str, arguments: &[Value], span: Span) -> Result<Value, Diagnostic> {
        match name {


            "HOST.Net.Address.Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Address.Parse expects STRING",
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Address.ToString expects Address",
                        span,
                    ));
                };
                let Some(Value::String(value)) = fields.get("value") else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid Address value",
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
                    "IsLoopback" => address.as_std().is_loopback(),
                    "IsPrivate" => match address.as_std() {
                        std::net::IpAddr::V4(value) => value.is_private(),
                        std::net::IpAddr::V6(value) => value.is_unique_local(),
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Endpoint.Create expects Address",
                        span,
                    ));
                };
                if type_name != "HOST.Net.Address" {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid Address value",
                        span,
                    ));
                }
                let (port, _) = integer(&arguments[1], span)?;
                let port = u16::try_from(port).map_err(|_| {
                    runtime_error("INVALID_INPUT", "port is outside 0..65535", span)
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Address expects Endpoint",
                        span,
                    ));
                };
                if type_name != "HOST.Net.Endpoint" {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid Endpoint value",
                        span,
                    ));
                }
                fields
                    .get("address")
                    .cloned()
                    .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid Endpoint value", span))
            }
            "HOST.Net.Endpoint.Port" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Port expects Endpoint",
                        span,
                    ));
                };
                if type_name != "HOST.Net.Endpoint" {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid Endpoint value",
                        span,
                    ));
                }
                fields
                    .get("port")
                    .cloned()
                    .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid Endpoint value", span))
            }
            "HOST.Net.CIDR.Parse" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "CIDR.Parse expects STRING",
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
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "CIDR.Contains expects CIDR",
                        span,
                    ));
                };
                if type_name != "HOST.Net.CIDR" {
                    return Err(runtime_error("TYPE_MISMATCH", "invalid CIDR value", span));
                }
                let Some(Value::String(network)) = fields.get("network") else {
                    return Err(runtime_error("TYPE_MISMATCH", "invalid CIDR value", span));
                };
                let Some(Value::Integer(prefix, _)) = fields.get("prefix") else {
                    return Err(runtime_error("TYPE_MISMATCH", "invalid CIDR value", span));
                };
                let Value::Record {
                    type_name: address_type,
                    fields: address_fields,
                } = &arguments[1]
                else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "Contains expects Address",
                        span,
                    ));
                };
                if address_type != "HOST.Net.Address" {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid Address value",
                        span,
                    ));
                }
                let Some(Value::String(address)) = address_fields.get("value") else {
                    return Err(runtime_error(
                        "TYPE_MISMATCH",
                        "invalid Address value",
                        span,
                    ));
                };
                let cidr = crate::net::Cidr::parse(&format!("{network}/{prefix}"))
                    .map_err(|message| runtime_error("INVALID_VALUE", message, span))?;
                let address = crate::net::Address::parse(address)
                    .map_err(|_| runtime_error("INVALID_VALUE", "invalid Address value", span))?;
                Ok(Value::Boolean(cidr.contains(address)))
            }
            "HOST.Net.CIDR.Network" | "HOST.Net.CIDR.PrefixLength" => {
                require_arity(name, arguments, 1, span)?;
                let Value::Record { type_name, fields } = &arguments[0] else {
                    return Err(runtime_error("TYPE_MISMATCH", "expected CIDR", span));
                };
                if type_name != "HOST.Net.CIDR" {
                    return Err(runtime_error("TYPE_MISMATCH", "invalid CIDR value", span));
                }
                if name.ends_with(".Network") {
                    let Some(Value::String(network)) = fields.get("network") else {
                        return Err(runtime_error("TYPE_MISMATCH", "invalid CIDR value", span));
                    };
                    Ok(Value::Record {
                        type_name: "HOST.Net.Address".into(),
                        fields: HashMap::from([("value".into(), Value::String(network.clone()))]),
                    })
                } else {
                    fields
                        .get("prefix")
                        .cloned()
                        .ok_or_else(|| runtime_error("TYPE_MISMATCH", "invalid CIDR value", span))
                }
            }
            "HOST.Net.Ping" => {
                require_arity(name, arguments, 2, span)?;
                let _address = net_address(&arguments[0], span)?;
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "ping timeout is outside 1..60000 ms".into(),
                    });
                }
                Ok(Value::Error {
                    code: 1,
                    message: "ICMP Echo provider unavailable".into(),
                })
            }
            "HOST.Net.Neighbor" => {
                require_arity(name, arguments, 1, span)?;
                let _address = net_address(&arguments[0], span)?;
                Ok(Value::Error {
                    code: 1,
                    message: "direct-neighbor provider unavailable".into(),
                })
            }
            "HOST.Net.Reverse" => {
                require_arity(name, arguments, 2, span)?;
                let _address = net_address(&arguments[0], span)?;
                let (timeout, _) = integer(&arguments[1], span)?;
                if !(1..=60_000).contains(&timeout) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "reverse timeout is outside 1..60000 ms".into(),
                    });
                }
                Ok(Value::Error {
                    code: 1,
                    message: "reverse resolver provider unavailable".into(),
                })
            }

            _ => Err(runtime_error("HOST_CAPABILITY_UNAVAILABLE", format!("host function '{name}' is not available"), span)),
        }
    }
}
