// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::semantic::{FloatType, IntegerType, Type};

use super::Value;

#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::match_same_arms
)]
pub(super) fn equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Integer(left, _), Value::Integer(right, _)) => left == right,
        (Value::Float(left, _), Value::Float(right, _)) => left == right,
        (Value::Integer(left, _), Value::Float(right, _)) => *left as f64 == *right,
        (Value::Float(left, _), Value::Integer(right, _)) => *left == *right as f64,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Null, Value::Null)
        | (Value::NotAvailable, Value::NotAvailable)
        | (Value::EndOfFile, Value::EndOfFile)
        | (Value::HostConsole, Value::HostConsole) => true,
        (Value::Handle { type_name: left }, Value::Handle { type_name: right }) => left == right,
        (Value::File(left), Value::File(right)) => left == right,
        (Value::DataFrame(left), Value::DataFrame(right)) => left == right,
        (
            Value::Error {
                code: left_code,
                message: left_message,
            },
            Value::Error {
                code: right_code,
                message: right_message,
            },
        ) => left_code == right_code && left_message == right_message,
        (Value::Object { handle: left, .. }, Value::Object { handle: right, .. }) => left == right,
        (Value::Pointer { handle: left }, Value::Pointer { handle: right }) => left == right,
        (Value::Date(left), Value::Date(right)) => left == right,
        (Value::Time(left), Value::Time(right)) => left == right,
        (Value::TimeZone(left), Value::TimeZone(right)) => left == right,
        (
            Value::Record {
                type_name: left_name,
                fields: left,
            },
            Value::Record {
                type_name: right_name,
                fields: right,
            },
        ) => {
            left_name == right_name
                && left.len() == right.len()
                && left
                    .iter()
                    .all(|(name, value)| right.get(name).is_some_and(|other| equals(value, other)))
        }
        (Value::Vector(left), Value::Vector(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equals(left, right))
        }
        _ => false,
    }
}

pub(super) fn is_value(value: &Value, test: &str) -> bool {
    match test {
        "INTEGER" | "INT32" => matches!(value, Value::Integer(_, IntegerType::Int32)),
        "BYTE" => matches!(value, Value::Integer(_, IntegerType::Byte)),
        "INT8" => matches!(value, Value::Integer(_, IntegerType::Int8)),
        "INT16" => matches!(value, Value::Integer(_, IntegerType::Int16)),
        "INT64" | "TIMESTAMP" => matches!(value, Value::Integer(_, IntegerType::Int64)),
        "UINT16" => matches!(value, Value::Integer(_, IntegerType::UInt16)),
        "UINT32" => matches!(value, Value::Integer(_, IntegerType::UInt32)),
        "UINT64" => matches!(value, Value::Integer(_, IntegerType::UInt64)),
        "FLOAT" | "FLOAT64" => matches!(value, Value::Float(_, FloatType::Float64)),
        "FLOAT32" => matches!(value, Value::Float(_, FloatType::Float32)),
        "BOOLEAN" => matches!(value, Value::Boolean(_)),
        "STRING" => matches!(value, Value::String(_)),
        "NAN" => matches!(value, Value::Float(value, _) if value.is_nan()),
        "INF" => matches!(value, Value::Float(value, _) if *value == f64::INFINITY),
        "-INF" => matches!(value, Value::Float(value, _) if *value == f64::NEG_INFINITY),
        "NULL" => matches!(value, Value::Null),
        "NA" => matches!(value, Value::NotAvailable),
        "EOF" => matches!(value, Value::EndOfFile),
        "Error" => matches!(value, Value::Error { .. }),
        "DATE" => matches!(value, Value::Date(_)),
        "TIME" => matches!(value, Value::Time(_)),
        "TIMEZONE" => matches!(value, Value::TimeZone(_)),
        test if test.starts_with("POINTER TO ") => matches!(value, Value::Pointer { .. }),
        _ => match value {
            Value::File(_) => is_host_file_type(test),
            Value::DataFrame(_) => {
                test == "DataFrame" || (test.ends_with(".DataFrame") && !test.starts_with('#'))
            }
            Value::LogFields(_) => test == "BNLog.Fields" || test == "Fields",
            Value::LogEntry(_) => test == "BNLog.Entry" || test == "Entry",
            Value::LogLogger(_) => test == "BNLog.Logger" || test == "Logger",
            Value::Json(_) => test == "BNJson.Json" || test == "Json",
            Value::DispatchQueue(_) => test == "BNDispatch.Queue" || test == "Queue",
            Value::DispatchTicket(_) => test == "BNDispatch.Ticket" || test == "Ticket",
            Value::DispatchGroup(_) => test == "BNDispatch.Group" || test == "Group",
            Value::DispatchBarrier(_) => test == "BNDispatch.Barrier" || test == "Barrier",
            Value::DispatchSemaphore(_) => test == "BNDispatch.Semaphore" || test == "Semaphore",
            Value::DispatchMutex(_) => test == "BNDispatch.Mutex" || test == "Mutex",
            Value::Object { class, .. } => class == test || class.rsplit('.').next() == Some(test),
            Value::Record { type_name, .. } => type_name == test,
            _ => false,
        },
    }
}

pub(super) fn value_matches_type(value: &Value, ty: &Type) -> bool {
    match (value, ty) {
        (Value::Integer(_, _), Type::Integer(_))
        | (Value::Float(_, _), Type::Float(_))
        | (Value::Boolean(_), Type::Boolean)
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile)
        | (Value::Object { .. }, Type::Named(_) | Type::TypeName(_) | Type::ImportedNamed { .. } | Type::ImportedTypeName { .. })
        | (Value::Pointer { .. }, Type::Pointer { .. }) => true,
        (Value::Date(_), Type::Named(name)) if name == "DATE" => true,
        (Value::Time(_), Type::Named(name)) if name == "TIME" => true,
        (Value::TimeZone(_), Type::Named(name)) if name == "TIMEZONE" => true,
        (Value::Error { .. }, Type::Named(name)) => name == "Error",
        (Value::TcpStream(_), Type::Named(name)) => name == "HOST.Net.TCPStream",
        (Value::TcpListener(_), Type::Named(name)) => name == "HOST.Net.TCPListener",
        (Value::UdpSocket(_), Type::Named(name)) => name == "HOST.Net.UDPSocket",
        (Value::File(_), Type::Named(name) | Type::TypeName(name)) => is_host_file_type(name),
        (
            Value::DataFrame(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "DataFrame",
        (Value::Null, Type::Named(name)) => name == "VOID",
        (
            Value::Record { type_name, .. },
            Type::Named(name) | Type::TypeName(name) | Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. },
        ) => {
            type_name == name
                || type_name.rsplit('.').next() == Some(name.as_str())
                || (name.starts_with("Net.") && type_name == &format!("HOST.{name}"))
        }
        (
            Value::LogFields(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNLog.Fields" || name == "Fields" || name.ends_with(".Fields"),
        (
            Value::LogEntry(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNLog.Entry" || name == "Entry" || name.ends_with(".Entry"),
        (
            Value::LogLogger(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNLog.Logger" || name == "Logger" || name.ends_with(".Logger"),
        (
            Value::Json(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNJson.Json" || name == "Json" || name.ends_with(".Json"),
        (
            Value::DispatchQueue(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNDispatch.Queue" || name == "Queue" || name.ends_with(".Queue"),
        (
            Value::DispatchTicket(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNDispatch.Ticket" || name == "Ticket" || name.ends_with(".Ticket"),
        (Value::DispatchGroup(_), Type::Named(name) | Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. }) => name == "BNDispatch.Group" || name == "Group" || name.ends_with(".Group"),
        (Value::DispatchBarrier(_), Type::Named(name) | Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. }) => name == "BNDispatch.Barrier" || name == "Barrier" || name.ends_with(".Barrier"),
        (Value::DispatchSemaphore(_), Type::Named(name) | Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. }) => name == "BNDispatch.Semaphore" || name == "Semaphore" || name.ends_with(".Semaphore"),
        (Value::DispatchMutex(_), Type::Named(name) | Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. }) => name == "BNDispatch.Mutex" || name == "Mutex" || name.ends_with(".Mutex"),
        _ => false,
    }
}

pub(super) fn is_host_file_type(name: &str) -> bool {
    name == "FS.File" || (name.ends_with(".File") && !name.starts_with('#'))
}

pub(super) fn is_host_file_method(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(owner, method)| {
        is_host_file_type(owner)
            && matches!(
                method,
                "Close"
                    | "ReadLine"
                    | "ReadAll"
                    | "Write"
                    | "ReadBytes"
                    | "WriteBytes"
                    | "WriteLine"
            )
    })
}
