// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared language type identities used by semantic analysis and BN IR.

pub mod error_codes;
pub mod text;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ModuleId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Boolean,
    Integer(IntegerType),
    IntegerLiteral(String),
    Float(FloatType),
    FloatLiteral,
    String,
    Null,
    NotAvailable,
    EndOfFile,
    System,
    HostClock,
    HostRandom,
    HostConsole,
    HostFileSystem,
    HostNet,
    HostExec,
    HostArgs,
    Named(String),
    TypeName(String),
    ImportedNamed {
        module: ModuleId,
        name: String,
    },
    ImportedTypeName {
        module: ModuleId,
        name: String,
    },
    Module(ModuleId),
    Function {
        parameters: Vec<Type>,
        return_type: Box<Type>,
    },
    Vector {
        element: Box<Type>,
        dimensions: Vec<u64>,
    },
    Pointer {
        element: Box<Type>,
        length: PointerLength,
    },
    Alternative(Vec<Type>),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegerType {
    Byte,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
}

/// The scalar type an `IS` test name denotes (`x IS INTEGER`), for both
/// backends: `INTEGER` is `INT32` and `TIMESTAMP` is `INT64`.
#[must_use]
pub fn scalar_test_type(test: &str) -> Option<Type> {
    Some(match test {
        "INTEGER" | "INT32" => Type::Integer(IntegerType::Int32),
        "BYTE" => Type::Integer(IntegerType::Byte),
        "INT8" => Type::Integer(IntegerType::Int8),
        "INT16" => Type::Integer(IntegerType::Int16),
        "INT64" | "TIMESTAMP" => Type::Integer(IntegerType::Int64),
        "UINT16" => Type::Integer(IntegerType::UInt16),
        "UINT32" => Type::Integer(IntegerType::UInt32),
        "UINT64" => Type::Integer(IntegerType::UInt64),
        "FLOAT" | "FLOAT64" => Type::Float(FloatType::Float64),
        "FLOAT32" => Type::Float(FloatType::Float32),
        "BOOLEAN" => Type::Boolean,
        "STRING" => Type::String,
        _ => return None,
    })
}

#[must_use]
pub const fn integer_byte_size(kind: IntegerType) -> u64 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 1,
        IntegerType::Int16 | IntegerType::UInt16 => 2,
        IntegerType::Int32 | IntegerType::UInt32 => 4,
        IntegerType::Int64 | IntegerType::UInt64 => 8,
    }
}

#[must_use]
pub fn dimension_product(dimensions: &[u64]) -> Option<u64> {
    if dimensions.contains(&u64::MAX) {
        return None;
    }
    dimensions
        .iter()
        .try_fold(1u64, |product, dimension| product.checked_mul(*dimension))
}

#[must_use]
pub fn static_size_of(ty: &Type) -> Option<u64> {
    match ty {
        Type::Boolean => Some(1),
        Type::Integer(kind) => Some(integer_byte_size(*kind)),
        Type::IntegerLiteral(_) | Type::Float(FloatType::Float32) => Some(4),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some(8),
        Type::Named(name) if name == "DATE" || name == "TIME" => Some(4),
        Type::Vector {
            element,
            dimensions,
        } => static_size_of(element)
            .and_then(|element| dimension_product(dimensions)?.checked_mul(element)),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatType {
    Float32,
    Float64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerLength {
    One,
    Fixed(u64),
    Dynamic,
}
