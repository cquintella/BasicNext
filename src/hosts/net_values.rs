// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use crate::{diagnostic::Diagnostic, source::Span, types::IntegerType};

use bn_value::Value;

use crate::runtime::{runtime_error_pub as runtime_error, type_mismatch};

pub(crate) fn net_addresses(value: &Value, span: Span) -> Result<&Vec<Value>, Diagnostic> {
    let Value::Record { type_name, fields } = value else {
        return Err(type_mismatch(
            "HOST.Net.Addresses",
            "non-record value",
            "HOST.Net",
            span,
        ));
    };
    if type_name != "HOST.Net.Addresses" {
        return Err(type_mismatch(
            "HOST.Net.Addresses",
            type_name,
            "HOST.Net",
            span,
        ));
    }
    let Some(Value::Vector(values)) = fields.get("values") else {
        return Err(type_mismatch(
            "values: vector",
            "missing or non-vector field",
            "HOST.Net.Addresses",
            span,
        ));
    };
    Ok(values)
}

pub(crate) fn net_address(value: &Value, span: Span) -> Result<crate::net::Address, Diagnostic> {
    let Value::Record { type_name, fields } = value else {
        return Err(type_mismatch(
            "HOST.Net.Address",
            "non-record value",
            "HOST.Net",
            span,
        ));
    };
    if type_name != "HOST.Net.Address" {
        return Err(type_mismatch(
            "HOST.Net.Address",
            type_name,
            "HOST.Net",
            span,
        ));
    }
    let Some(Value::String(address)) = fields.get("value") else {
        return Err(type_mismatch(
            "value: STRING",
            "missing or non-string field",
            "HOST.Net.Address",
            span,
        ));
    };
    crate::net::Address::parse(address).map_err(|_| {
        runtime_error(
            crate::diagnostic::DiagId::INVALID_INPUT,
            "invalid Net.Address value",
            span,
        )
    })
}

pub(crate) fn net_endpoint(value: &Value, span: Span) -> Result<crate::net::Endpoint, Diagnostic> {
    let Value::Record { type_name, fields } = value else {
        return Err(type_mismatch(
            "HOST.Net.Endpoint",
            "non-record value",
            "HOST.Net",
            span,
        ));
    };
    if type_name != "HOST.Net.Endpoint" {
        return Err(type_mismatch(
            "HOST.Net.Endpoint",
            type_name,
            "HOST.Net",
            span,
        ));
    }
    let Some(Value::Record {
        type_name: address_type,
        fields: address_fields,
    }) = fields.get("address")
    else {
        return Err(type_mismatch(
            "address: HOST.Net.Address",
            "missing or incompatible field",
            "HOST.Net.Endpoint",
            span,
        ));
    };
    if address_type != "HOST.Net.Address" {
        return Err(type_mismatch(
            "HOST.Net.Address",
            address_type,
            "HOST.Net.Endpoint.address",
            span,
        ));
    }
    let Some(Value::String(address)) = address_fields.get("value") else {
        return Err(type_mismatch(
            "value: STRING",
            "missing or non-string field",
            "HOST.Net.Address",
            span,
        ));
    };
    let Some(Value::Integer(port, _)) = fields.get("port") else {
        return Err(type_mismatch(
            "port: INTEGER",
            "missing or non-integer field",
            "HOST.Net.Endpoint",
            span,
        ));
    };
    let port = u16::try_from(*port).map_err(|_| {
        runtime_error(
            crate::diagnostic::DiagId::INVALID_INPUT,
            "port is outside 0..65535",
            span,
        )
    })?;
    let address = crate::net::Address::parse(address).map_err(|_| {
        runtime_error(
            crate::diagnostic::DiagId::INVALID_INPUT,
            "invalid Net.Address value",
            span,
        )
    })?;
    Ok(crate::net::Endpoint::new(address, port))
}

pub(crate) fn endpoint_value(endpoint: crate::net::Endpoint) -> Value {
    Value::Record {
        type_name: "HOST.Net.Endpoint".into(),
        fields: HashMap::from([
            (
                "address".into(),
                Value::Record {
                    type_name: "HOST.Net.Address".into(),
                    fields: HashMap::from([(
                        "value".into(),
                        Value::String(endpoint.address().to_string()),
                    )]),
                },
            ),
            (
                "port".into(),
                Value::Integer(i128::from(endpoint.port()), IntegerType::UInt16),
            ),
        ]),
    }
}

pub(crate) fn address_value(address: std::net::IpAddr) -> Value {
    Value::Record {
        type_name: "HOST.Net.Address".into(),
        fields: HashMap::from([("value".into(), Value::String(address.to_string()))]),
    }
}

pub(crate) fn ping_reply_value(reply: crate::net::PingReply) -> Value {
    Value::Record {
        type_name: "HOST.Net.PingReply".into(),
        fields: HashMap::from([
            ("address".into(), address_value(reply.address.as_std())),
            (
                "roundTripMicroseconds".into(),
                Value::Integer(
                    i128::from(reply.round_trip_microseconds),
                    IntegerType::Int64,
                ),
            ),
        ]),
    }
}
