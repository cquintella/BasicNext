// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::ast::{DeclarationKind, FunctionSignature, TypeReference, VectorDimension};

use super::types::{float_type, integer_type, parse_integer, type_from_name};
use super::{PointerLength, Type, default_span};

pub(super) fn declaration_type(kind: DeclarationKind, name: &str) -> Type {
    match kind {
        DeclarationKind::Function => Type::Unknown,
        _ => Type::TypeName(name.into()),
    }
}

pub(super) fn function_type(signature: &FunctionSignature) -> Type {
    Type::Function {
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| type_from_reference(&parameter.type_ref))
            .collect(),
        return_type: Box::new(type_from_reference(&signature.return_type)),
    }
}

pub(super) fn type_from_reference(reference: &TypeReference) -> Type {
    let alternatives = reference
        .alternatives
        .iter()
        .map(type_from_atom)
        .collect::<Vec<_>>();
    match alternatives.as_slice() {
        [] => Type::Unknown,
        [ty] => ty.clone(),
        _ => Type::Alternative(alternatives),
    }
}

pub(super) fn type_from_atom(atom: &crate::ast::TypeAtom) -> Type {
    if atom.name == "FUNCTION" {
        return function_type_from_parts(&atom.parts);
    }
    if atom.name == "POINTER" {
        return Type::Pointer {
            element: Box::new(pointer_element_type(&atom.parts)),
            length: pointer_length(&atom.parts),
        };
    }
    let name = qualified_type_name(atom);
    let base = match name.as_str() {
        "BOOLEAN" => Type::Boolean,
        "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64"
        | "INTEGER" | "TIMESTAMP" => {
            Type::Integer(integer_type(atom.name.as_str()).expect("numeric type"))
        }
        "FLOAT32" | "FLOAT64" | "FLOAT" => {
            Type::Float(float_type(atom.name.as_str()).expect("float type"))
        }
        "STRING" => Type::String,
        "NULL" => Type::Null,
        "NA" => Type::NotAvailable,
        "EOF" => Type::EndOfFile,
        "POINTER" => Type::Unknown,
        "SYSTEM" => Type::System,
        name => Type::Named(name.into()),
    };
    let dimensions = if atom.dimensions.is_empty() {
        atom.parts
            .windows(3)
            .filter(|parts| parts[0] == "LeftBracket" && parts[2] == "RightBracket")
            .filter_map(|parts| {
                parse_integer(&parts[1]).and_then(|value| u64::try_from(value).ok())
            })
            .collect::<Vec<_>>()
    } else {
        atom.dimensions
            .iter()
            .map(|dimension| match dimension {
                VectorDimension::Literal { value, .. } => parse_integer(value)
                    .and_then(|value| u64::try_from(value).ok())
                    .unwrap_or(u64::MAX),
                VectorDimension::Expression(_) => u64::MAX,
            })
            .collect::<Vec<_>>()
    };
    let has_vector_brackets = atom.parts.iter().any(|part| part == "LeftBracket");
    if dimensions.is_empty() && !has_vector_brackets {
        base
    } else {
        Type::Vector {
            element: Box::new(base),
            dimensions: if dimensions.is_empty() {
                vec![u64::MAX]
            } else {
                dimensions
            },
        }
    }
}

fn function_type_from_parts(parts: &[String]) -> Type {
    let Some(close) = matching_right_paren(parts) else {
        return Type::Unknown;
    };
    let parameter_tokens = &parts[1..close];
    let mut parameters = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=parameter_tokens.len() {
        let separator =
            index == parameter_tokens.len() || (parameter_tokens[index] == "Comma" && depth == 0);
        if separator {
            if start < index {
                let parameter = &parameter_tokens[start..index];
                let type_tokens = parameter
                    .iter()
                    .position(|token| token == "AS")
                    .map_or(parameter, |as_index| &parameter[as_index + 1..]);
                parameters.push(type_from_tokens(type_tokens));
            }
            start = index + 1;
            continue;
        }
        match parameter_tokens[index].as_str() {
            "LeftParen" | "LeftBracket" => depth += 1,
            "RightParen" | "RightBracket" => depth -= 1,
            _ => {}
        }
    }
    let return_type = parts
        .get(close + 1)
        .filter(|part| part.as_str() == "AS")
        .map_or(Type::Unknown, |_| type_from_tokens(&parts[close + 2..]));
    Type::Function {
        parameters,
        return_type: Box::new(return_type),
    }
}

fn matching_right_paren(parts: &[String]) -> Option<usize> {
    if parts.first()?.as_str() != "LeftParen" {
        return None;
    }
    let mut depth = 0;
    for (index, part) in parts.iter().enumerate() {
        match part.as_str() {
            "LeftParen" => depth += 1,
            "RightParen" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn type_from_tokens(tokens: &[String]) -> Type {
    let mut alternatives = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=tokens.len() {
        let separator = index == tokens.len() || (tokens[index] == "OR" && depth == 0);
        if separator {
            if start < index {
                alternatives.push(type_from_atom(&crate::ast::TypeAtom {
                    name: tokens[start].clone(),
                    parts: tokens[start + 1..index].to_vec(),
                    dimensions: Vec::new(),
                    span: default_span(),
                }));
            }
            start = index + 1;
            continue;
        }
        match tokens[index].as_str() {
            "LeftParen" | "LeftBracket" => depth += 1,
            "RightParen" | "RightBracket" => depth -= 1,
            _ => {}
        }
    }
    match alternatives.as_slice() {
        [ty] => ty.clone(),
        _ => Type::Alternative(alternatives),
    }
}

pub(super) fn qualified_type_name(atom: &crate::ast::TypeAtom) -> String {
    let mut name = atom.name.clone();
    let mut parts = atom.parts.iter();
    while matches!(parts.next(), Some(part) if part == "Dot") {
        let Some(part) = parts.next() else { break };
        name.push('.');
        name.push_str(part);
    }
    name
}

pub(super) fn pointer_element_type(parts: &[String]) -> Type {
    let Some(start) = parts
        .iter()
        .position(|part| part == "TO")
        .map(|index| index + 1)
    else {
        return Type::Unknown;
    };
    let end = parts[start..]
        .iter()
        .position(|part| part == "LeftBracket")
        .map_or(parts.len(), |index| start + index);
    let element = &parts[start..end];
    let Some(first) = element.first() else {
        return Type::Unknown;
    };
    if element.len() == 1 {
        return type_from_name(first);
    }
    let mut name = first.clone();
    let mut suffix = element[1..].chunks_exact(2);
    for pair in &mut suffix {
        if pair[0] != "Dot" {
            return Type::Unknown;
        }
        name.push('.');
        name.push_str(&pair[1]);
    }
    if !suffix.remainder().is_empty() {
        return Type::Unknown;
    }
    type_from_name(&name)
}

fn pointer_length(parts: &[String]) -> PointerLength {
    let Some(open) = parts.iter().position(|part| part == "LeftBracket") else {
        return PointerLength::One;
    };
    parts
        .get(open + 1)
        .filter(|part| part.as_str() != "RightBracket")
        .and_then(|part| parse_integer(part))
        .and_then(|part| u64::try_from(part).ok())
        .map_or(PointerLength::Dynamic, PointerLength::Fixed)
}
