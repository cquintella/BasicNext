// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::HashMap;

use crate::{
    diagnostic::Diagnostic,
    ir::{BasicBlock, BlockId, Constant, Function, ValueId},
    semantic::{IntegerType, Type},
    source::Span,
    temporal,
};

use super::{
    Value, float_kind, float_value, integer_kind, parse_float, parse_integer, runtime_error,
};

pub(super) fn find_block(function: &Function, id: BlockId) -> Result<&BasicBlock, Diagnostic> {
    function
        .blocks
        .get(id.0 as usize)
        .filter(|block| block.id == id)
        .ok_or_else(|| runtime_error("INVALID_IR", "basic block does not exist", function.span))
}

pub(super) fn set(values: &mut HashMap<ValueId, Value>, destination: ValueId, value: Value) {
    values.insert(destination, value);
}

pub(super) fn value(
    values: &HashMap<ValueId, Value>,
    id: ValueId,
    span: Span,
) -> Result<&Value, Diagnostic> {
    values
        .get(&id)
        .ok_or_else(|| runtime_error("INVALID_IR", format!("value %{} is undefined", id.0), span))
}

pub(super) fn constant_value(
    constant: &Constant,
    ty: &Type,
    span: Span,
) -> Result<Value, Diagnostic> {
    match constant {
        Constant::Integer(value) => Ok(Value::Integer(
            parse_integer(value)
                .ok_or_else(|| runtime_error("INVALID_IR", "invalid integer constant", span))?,
            integer_kind(ty).unwrap_or(IntegerType::Int32),
        )),
        Constant::Float(value) => Ok(float_value(parse_float(value), float_kind(ty))),
        Constant::String(value) => Ok(Value::String(value.clone())),
        Constant::Boolean(value) => Ok(Value::Boolean(*value)),
        Constant::Null => Ok(Value::Null),
        Constant::NotAvailable => Ok(Value::NotAvailable),
        Constant::EndOfFile => Ok(Value::EndOfFile),
        Constant::Function(value) => Ok(Value::Function(value.clone())),
        Constant::Type(value) => Ok(Value::Type(value.clone())),
        Constant::HostConsole => Ok(Value::HostConsole),
        Constant::HostArgs => Ok(Value::HostArgs),
    }
}

pub(super) fn empty_named(name: &str) -> Value {
    match name {
        "DATE" => Value::Date(temporal::default_date()),
        "TIME" => Value::Time(temporal::default_time()),
        "TIMEZONE" => Value::TimeZone("UTC".into()),
        "VOID" => Value::Handle {
            type_name: name.into(),
        },
        "Error" => Value::Error {
            code: 0,
            message: String::new(),
        },
        _ => Value::Record {
            type_name: name.into(),
            fields: HashMap::new(),
        },
    }
}

pub(super) fn require_arity(
    name: &str,
    arguments: &[Value],
    expected: usize,
    span: Span,
) -> Result<(), Diagnostic> {
    if arguments.len() == expected {
        Ok(())
    } else {
        Err(runtime_error(
            "TYPE_MISMATCH",
            format!("{name} expects {expected} argument(s)"),
            span,
        ))
    }
}

pub(super) fn default_function_owner(ty: &Type) -> Option<String> {
    match ty {
        Type::Named(name) | Type::TypeName(name) => Some(name.clone()),
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            Some(format!("#{}.{name}", module.0))
        }
        _ => None,
    }
}
