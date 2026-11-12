// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_types::{Type, class_model::ClassModel};

use super::Value;

#[allow(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::match_same_arms
)]
#[must_use]
pub fn equals(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Integer(left, _), Value::Integer(right, _)) => left == right,
        (Value::Float(left, _), Value::Float(right, _)) => left == right,
        (Value::Integer(left, _), Value::Float(right, _)) => *left as f64 == *right,
        (Value::Float(left, _), Value::Integer(right, _)) => *left == *right as f64,
        (Value::Boolean(left), Value::Boolean(right)) => left == right,
        (Value::String(left), Value::String(right)) => left == right,
        (Value::Function(left), Value::Function(right)) => left == right,
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
                ..
            },
            Value::Error {
                code: right_code,
                message: right_message,
                ..
            },
        ) => left_code == right_code && left_message == right_message,
        (Value::Object { handle: left, .. }, Value::Object { handle: right, .. }) => left == right,
        (Value::Pointer { handle: left, .. }, Value::Pointer { handle: right, .. }) => {
            left == right
        }
        (Value::Date(left), Value::Date(right)) => left == right,
        (Value::Time(left), Value::Time(right)) => left == right,
        (Value::TimeZone(left), Value::TimeZone(right)) => left == right,
        (Value::Record { record: left }, Value::Record { record: right }) => {
            left.type_name() == right.type_name()
                && left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| equals(left, right))
        }
        (Value::Vector(left), Value::Vector(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| equals(left, right))
        }
        _ => false,
    }
}

/// `value IS test`. An object holds a class or interface alternative when its
/// class is that type or reaches it through `classes` (a subclass, or a
/// class implementing the interface: `0.6.md`, "IS" on declared
/// alternatives).
pub(super) fn is_value(
    value: &Value,
    test: &str,
    classes: &ClassModel,
    pointer_len: Option<u64>,
) -> bool {
    if let Some(ty) = bn_types::scalar_test_type(test) {
        let actual = match value {
            Value::Integer(_, kind) => Type::Integer(*kind),
            Value::Float(_, kind) => Type::Float(*kind),
            Value::Boolean(_) => Type::Boolean,
            Value::String(_) => Type::String,
            _ => return false,
        };
        return actual == ty;
    }
    match test {
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
        test if test.starts_with(bn_catalog::text("frontend", "msg.pointer_to_prefix")) => {
            match value {
                Value::Pointer { element, .. } => {
                    bn_types::pointers::pointer_is_holds(*element, pointer_len.unwrap_or(0), test)
                        .unwrap_or(false)
                }
                _ => false,
            }
        }
        // `ELEMENT[n][m]`: a fixed vector of exactly `n` elements, each an
        // `ELEMENT[m]`.
        test if test.ends_with(']')
            && let Some(open) = test.find('[') =>
        {
            let close = open + test[open..].find(']').expect("closing bracket");
            let element = format!("{}{}", &test[..open], &test[close + 1..]);
            matches!(value, Value::Vector(items)
                if test[open + 1..close].parse::<usize>() == Ok(items.len())
                    && items.iter().all(|item| is_value(item, &element, classes, None)))
        }
        _ => is_opaque_or_object_value(value, test, classes),
    }
}

fn is_opaque_or_object_value(value: &Value, test: &str, classes: &ClassModel) -> bool {
    match value {
        Value::File(_) => is_host_file_type(test),
        Value::DataFrame(_) => {
            test == "DataFrame" || (test.ends_with(".DataFrame") && !test.starts_with('#'))
        }
        Value::LogFields(_) => test == "BNLog.Fields" || test == "Fields",
        Value::LogEntry(_) => test == "BNLog.Entry" || test == "Entry",
        Value::LogLogger(_) => test == "BNLog.Logger" || test == "Logger",
        Value::Json(_) => test == "BNJson.Json" || test == "Json",
        Value::CryptoBytes(_) => {
            test == bn_catalog::text("modules", "type.crypto_bytes")
                || test == "Bytes"
                || test == bn_catalog::text("modules", "type.compress_bytes")
        }
        Value::CryptoKey(_) => {
            test == bn_catalog::text("modules", "type.crypto_key")
                || test == "Key"
                || test.ends_with(".Key")
                || test == "PublicKey"
                || test.ends_with(".PublicKey")
                || test == "PrivateKey"
                || test.ends_with(".PrivateKey")
        }
        Value::CryptoSecret(_) => {
            test == bn_catalog::text("modules", "type.crypto_secret")
                || test == "Secret"
                || test.ends_with(".Secret")
        }
        Value::CryptoSecretKey(_) => {
            test == bn_catalog::text("modules", "type.crypto_secret_key")
                || test == "SecretKey"
                || test.ends_with(".SecretKey")
        }
        Value::CompressBytes(_) => {
            test == bn_catalog::text("modules", "type.compress_bytes")
                || test == "Bytes"
                || test == bn_catalog::text("modules", "type.crypto_bytes")
        }
        Value::CompressZip(_) => {
            test == bn_catalog::text("modules", "type.compress_zip_archive") || test == "ZipArchive"
        }
        Value::DispatchQueue(_) => test == "BNDispatch.Queue" || test == "Queue",
        Value::DispatchTicket(_) => test == "BNDispatch.Ticket" || test == "Ticket",
        Value::DispatchGroup(_) => test == "BNDispatch.Group" || test == "Group",
        Value::DispatchBarrier(_) => test == "BNDispatch.Barrier" || test == "Barrier",
        Value::DispatchSemaphore(_) => test == "BNDispatch.Semaphore" || test == "Semaphore",
        Value::DispatchMutex(_) => test == "BNDispatch.Mutex" || test == "Mutex",
        // HOST type tests arrive under their canonical names (lowering).
        Value::TcpStream(_) => test == "HOST.Net.TCPStream",
        Value::TcpListener(_) => test == "HOST.Net.TCPListener",
        Value::UdpSocket(_) => test == "HOST.Net.UDPSocket",
        Value::Object { class, .. } => {
            class.as_ref() == test
                || class.rsplit('.').next() == Some(test)
                || classes.is_upcast(class, test)
        }
        Value::Record { record } => {
            record.type_name().as_ref() == test
                || record.type_name().rsplit('.').next() == Some(test)
                || record.type_name().rsplit('.').next() == test.rsplit('.').next()
        }
        _ => false,
    }
}

#[allow(clippy::too_many_lines)] // One arm per value/type pair.
pub(super) fn value_matches_type(value: &Value, ty: &Type) -> bool {
    match (value, ty) {
        (Value::Integer(_, _), Type::Integer(_))
        | (Value::Float(_, _), Type::Float(_))
        | (Value::Boolean(_), Type::Boolean)
        | (Value::Function(_), Type::Function { .. })
        | (Value::String(_), Type::String)
        | (Value::Vector(_), Type::Vector { .. })
        | (Value::Null, Type::Null)
        | (Value::NotAvailable, Type::NotAvailable)
        | (Value::EndOfFile, Type::EndOfFile)
        | (
            Value::Object { .. },
            Type::Named(_)
            | Type::TypeName(_)
            | Type::ImportedNamed { .. }
            | Type::ImportedTypeName { .. },
        )
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
            | Type::TypeName(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "DataFrame" || name.ends_with(".DataFrame"),
        (Value::Null, Type::Named(name)) => name == "VOID",
        (
            Value::Record { record },
            Type::Named(name)
            | Type::TypeName(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            record.type_name().as_ref() == name
                || record.type_name().rsplit('.').next() == Some(name.as_str())
                || record.type_name().rsplit('.').next() == name.rsplit('.').next()
                || (name.starts_with("Net.")
                    && record.type_name().as_ref() == format!("HOST.{name}"))
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
            Value::CryptoBytes(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            name == bn_catalog::text("modules", "type.crypto_bytes")
                || name == "Bytes"
                || name.ends_with(".Bytes")
                || name == bn_catalog::text("modules", "type.compress_bytes")
        }
        (
            Value::CryptoKey(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            name == bn_catalog::text("modules", "type.crypto_key")
                || name == "Key"
                || name.ends_with(".Key")
                || name == "PublicKey"
                || name.ends_with(".PublicKey")
                || name == "PrivateKey"
                || name.ends_with(".PrivateKey")
        }
        (
            Value::CryptoSecret(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            name == bn_catalog::text("modules", "type.crypto_secret")
                || name == "Secret"
                || name.ends_with(".Secret")
        }
        (
            Value::CryptoSecretKey(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            name == bn_catalog::text("modules", "type.crypto_secret_key")
                || name == "SecretKey"
                || name.ends_with(".SecretKey")
        }
        (
            Value::CompressBytes(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            name == bn_catalog::text("modules", "type.compress_bytes")
                || name == "Bytes"
                || name.ends_with(".Bytes")
                || name == bn_catalog::text("modules", "type.crypto_bytes")
        }
        (
            Value::CompressZip(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => {
            name == bn_catalog::text("modules", "type.compress_zip_archive")
                || name == "ZipArchive"
                || name.ends_with(".ZipArchive")
        }
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
        (
            Value::DispatchGroup(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNDispatch.Group" || name == "Group" || name.ends_with(".Group"),
        (
            Value::DispatchBarrier(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNDispatch.Barrier" || name == "Barrier" || name.ends_with(".Barrier"),
        (
            Value::DispatchSemaphore(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNDispatch.Semaphore" || name == "Semaphore" || name.ends_with(".Semaphore"),
        (
            Value::DispatchMutex(_),
            Type::Named(name)
            | Type::ImportedNamed { name, .. }
            | Type::ImportedTypeName { name, .. },
        ) => name == "BNDispatch.Mutex" || name == "Mutex" || name.ends_with(".Mutex"),
        _ => false,
    }
}

pub(super) fn is_host_file_type(name: &str) -> bool {
    name == "FS.File" || (name.ends_with(".File") && !name.starts_with('#'))
}

/// `File.<method>` for a `<alias>.File.<method>` callee on a `Value::File`.
pub(super) fn host_file_member(name: &str) -> Option<String> {
    let (_, method) = name.rsplit_once('.')?;
    is_host_file_method(name).then(|| format!("File.{method}"))
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
