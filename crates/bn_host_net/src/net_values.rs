// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_types::IntegerType;

use bn_value::{RecordValue, Value, shared_string};

use bn_interp::{runtime_error_pub as runtime_error, type_mismatch};

/// Canonical positional layouts shared by HOST.Net providers and `BNWeb`.
pub mod slots {
    pub const UDP_PACKET_SOURCE: usize = 0;
    pub const UDP_PACKET_BYTES: usize = 1;
    pub const UDP_PACKET_TRUNCATED: usize = 2;
    pub const UDP_PACKET_FIELDS: usize = 3;
    pub const ADDRESSES_VALUES: usize = 0;
    pub const ADDRESSES_FIELDS: usize = 1;
    pub const ADDRESS_VALUE: usize = 0;
    pub const ADDRESS_FIELDS: usize = 1;
    pub const ENDPOINT_ADDRESS: usize = 0;
    pub const ENDPOINT_PORT: usize = 1;
    pub const ENDPOINT_FIELDS: usize = 2;
    pub const CIDR_NETWORK: usize = 0;
    pub const CIDR_PREFIX: usize = 1;
    pub const CIDR_FIELDS: usize = 2;
    pub const PING_REPLY_ADDRESS: usize = 0;
    pub const PING_REPLY_ROUND_TRIP_MICROSECONDS: usize = 1;
    pub const PING_REPLY_FIELDS: usize = 2;

    #[must_use]
    pub fn field_count(type_name: &str) -> Option<usize> {
        match type_name {
            "HOST.Net.UDPPacket" => Some(UDP_PACKET_FIELDS),
            "HOST.Net.Addresses" => Some(ADDRESSES_FIELDS),
            "HOST.Net.Address" => Some(ADDRESS_FIELDS),
            "HOST.Net.Endpoint" => Some(ENDPOINT_FIELDS),
            "HOST.Net.CIDR" => Some(CIDR_FIELDS),
            "HOST.Net.PingReply" => Some(PING_REPLY_FIELDS),
            _ => None,
        }
    }
}

/// # Errors
///
/// Returns `TYPE_MISMATCH` when the value is not an address vector.
pub fn net_addresses(value: &Value, span: Span) -> Result<&Vec<Value>, Diagnostic> {
    let Value::Record { record } = value else {
        return Err(type_mismatch(
            "HOST.Net.Addresses",
            "non-record value",
            "HOST.Net",
            span,
        ));
    };
    if record.type_name().as_ref() != "HOST.Net.Addresses" {
        return Err(type_mismatch(
            "HOST.Net.Addresses",
            record.type_name(),
            "HOST.Net",
            span,
        ));
    }
    if record.len() != slots::ADDRESSES_FIELDS {
        return Err(type_mismatch(
            "HOST.Net.Addresses with one field",
            "malformed record shape",
            "HOST.Net",
            span,
        ));
    }
    let Some(Value::Vector(values)) = record.get(slots::ADDRESSES_VALUES) else {
        return Err(type_mismatch(
            "values: vector",
            "missing or non-vector field",
            "HOST.Net.Addresses",
            span,
        ));
    };
    Ok(values)
}

/// # Errors
///
/// Returns `TYPE_MISMATCH` when the value is not a `HOST.Net.Address`.
pub fn net_address(value: &Value, span: Span) -> Result<crate::net::Address, Diagnostic> {
    let Value::Record { record } = value else {
        return Err(type_mismatch(
            "HOST.Net.Address",
            "non-record value",
            "HOST.Net",
            span,
        ));
    };
    if record.type_name().as_ref() != "HOST.Net.Address" {
        return Err(type_mismatch(
            "HOST.Net.Address",
            record.type_name(),
            "HOST.Net",
            span,
        ));
    }
    if record.len() != slots::ADDRESS_FIELDS {
        return Err(type_mismatch(
            "HOST.Net.Address with one field",
            "malformed record shape",
            "HOST.Net",
            span,
        ));
    }
    let Some(Value::String(address)) = record.get(slots::ADDRESS_VALUE) else {
        return Err(type_mismatch(
            "value: STRING",
            "missing or non-string field",
            "HOST.Net.Address",
            span,
        ));
    };
    crate::net::Address::parse(address).map_err(|_| {
        runtime_error(
            bn_diag::DiagId::INVALID_INPUT,
            "invalid Net.Address value",
            span,
        )
    })
}

/// # Errors
///
/// Returns `TYPE_MISMATCH` when the value is not a `HOST.Net.Endpoint`.
pub fn net_endpoint(value: &Value, span: Span) -> Result<crate::net::Endpoint, Diagnostic> {
    let Value::Record { record } = value else {
        return Err(type_mismatch(
            "HOST.Net.Endpoint",
            "non-record value",
            "HOST.Net",
            span,
        ));
    };
    if record.type_name().as_ref() != "HOST.Net.Endpoint" {
        return Err(type_mismatch(
            "HOST.Net.Endpoint",
            record.type_name(),
            "HOST.Net",
            span,
        ));
    }
    if record.len() != slots::ENDPOINT_FIELDS {
        return Err(type_mismatch(
            "HOST.Net.Endpoint with two fields",
            "malformed record shape",
            "HOST.Net",
            span,
        ));
    }
    let Some(Value::Record {
        record: address_record,
    }) = record.get(slots::ENDPOINT_ADDRESS)
    else {
        return Err(type_mismatch(
            "address: HOST.Net.Address",
            "missing or incompatible field",
            "HOST.Net.Endpoint",
            span,
        ));
    };
    if address_record.type_name().as_ref() != "HOST.Net.Address" {
        return Err(type_mismatch(
            "HOST.Net.Address",
            address_record.type_name(),
            "HOST.Net.Endpoint.address",
            span,
        ));
    }
    if address_record.len() != slots::ADDRESS_FIELDS {
        return Err(type_mismatch(
            "HOST.Net.Address with one field",
            "malformed record shape",
            "HOST.Net.Endpoint.address",
            span,
        ));
    }
    let Some(Value::String(address)) = address_record.get(slots::ADDRESS_VALUE) else {
        return Err(type_mismatch(
            "value: STRING",
            "missing or non-string field",
            "HOST.Net.Address",
            span,
        ));
    };
    let Some(Value::Integer(port, _)) = record.get(slots::ENDPOINT_PORT) else {
        return Err(type_mismatch(
            "port: INTEGER",
            "missing or non-integer field",
            "HOST.Net.Endpoint",
            span,
        ));
    };
    let port = u16::try_from(*port).map_err(|_| {
        runtime_error(
            bn_diag::DiagId::INVALID_INPUT,
            "port is outside 0..65535",
            span,
        )
    })?;
    let address = crate::net::Address::parse(address).map_err(|_| {
        runtime_error(
            bn_diag::DiagId::INVALID_INPUT,
            "invalid Net.Address value",
            span,
        )
    })?;
    Ok(crate::net::Endpoint::new(address, port))
}

#[must_use]
pub fn endpoint_value(endpoint: crate::net::Endpoint) -> Value {
    Value::Record {
        record: RecordValue::new(
            "HOST.Net.Endpoint",
            vec![
                Value::Record {
                    record: RecordValue::new(
                        "HOST.Net.Address",
                        vec![Value::String(shared_string(endpoint.address().to_string()))],
                    ),
                },
                Value::Integer(i128::from(endpoint.port()), IntegerType::UInt16),
            ],
        ),
    }
}

#[must_use]
pub fn address_value(address: std::net::IpAddr) -> Value {
    Value::Record {
        record: RecordValue::new(
            "HOST.Net.Address",
            vec![Value::String(shared_string(address.to_string()))],
        ),
    }
}

#[must_use]
pub fn ping_reply_value(reply: crate::net::PingReply) -> Value {
    Value::Record {
        record: RecordValue::new(
            "HOST.Net.PingReply",
            vec![
                address_value(reply.address.as_std()),
                Value::Integer(
                    i128::from(reply.round_trip_microseconds),
                    IntegerType::Int64,
                ),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{net_address, slots};
    use bn_value::{RecordValue, Value};

    #[test]
    fn malformed_address_shape_returns_diagnostic() {
        let value = Value::Record {
            record: RecordValue::new(
                "HOST.Net.Address",
                vec![Value::String("127.0.0.1".into()), Value::Null],
            ),
        };
        let error = net_address(&value, bn_interp::default_span())
            .expect_err("extra address slot must be rejected");
        assert_eq!(error.code, "TYPE_MISMATCH");
    }

    #[test]
    fn provider_slot_contract_declares_exact_shapes() {
        assert_eq!(slots::field_count("HOST.Net.Address"), Some(1));
        assert_eq!(slots::field_count("HOST.Net.Endpoint"), Some(2));
        assert_eq!(slots::field_count("HOST.Net.CIDR"), Some(2));
        assert_eq!(slots::field_count("unrecognized"), None);
    }
}
