// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dependency-free LLVM textual backend. Unsupported IR is rejected explicitly.

#![allow(clippy::match_same_arms)]

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Write as _,
};

use crate::{
    ir::{BlockId, Constant, Function, Instruction, Module, Terminator, ValueId},
    semantic::{FloatType, IntegerType, SymbolId, Type},
};

#[derive(Clone, Debug)]
enum ConstantValue {
    Integer(i128, IntegerType),
    Float(f64),
    Boolean(bool),
    String(String),
}

#[allow(clippy::struct_excessive_bools)]
struct LoweringAnalysis<'a> {
    values: HashMap<ValueId, Type>,
    symbols: HashMap<SymbolId, Type>,
    functions: HashMap<ValueId, &'a str>,
    strings: Vec<(ValueId, String)>,
    input_count: usize,
    uses_random: bool,
    uses_string_concat: bool,
    uses_bn_rt: bool,
    uses_bn_rt_math: bool,
    uses_float_print: bool,
    uses_string_ops: bool,
    uses_temporal_print: bool,
    uses_heap: bool,
    multi_defs: HashSet<ValueId>,
    intrinsics: BTreeSet<&'static str>,
}

struct BlockState {
    constants: HashMap<ValueId, ConstantValue>,
    bindings: HashMap<SymbolId, ConstantValue>,
}

#[allow(clippy::struct_excessive_bools)]
struct EmissionState {
    print_count: usize,
    continuation_count: usize,
    md_temp: usize,
    needs_numeric_overflow_trap: bool,
    needs_bn_rt_trap: bool,
    is_start: bool,
    rng_global: bool,
    synchronize_prints: bool,
    return_llvm: &'static str,
}

/// Lower the supported typed BN IR subset to LLVM IR.
///
/// # Errors
///
/// Returns a diagnostic when the module contains unsupported IR or has no
/// executable entry point.
pub fn lower_module(module: &Module) -> Result<String, String> {
    // Keep the public IR-only helper byte-for-byte compatible with the
    // historical textual output; the CLI selects native/Wasm host interop.
    lower_module_for_target(module, true)
}

/// Lowers a module while selecting target-specific host interop behavior.
///
/// # Errors
///
/// Returns a diagnostic when the module has no supported entry point or
/// contains an instruction outside the target lowering contract.
pub fn lower_module_for_target(module: &Module, wasm32: bool) -> Result<String, String> {
    let Some(start) = module
        .functions
        .iter()
        .find(|function| function.name == "Start")
    else {
        return Err("BUILD_ENTRYPOINT_MISSING: module has no Start function".into());
    };
    if !start.parameters.is_empty() {
        return Err(format!(
            "BUILD_LOWERING_UNAVAILABLE: Start has {} parameter(s); LLVM entry point requires FUNCTION Start()",
            start.parameters.len()
        ));
    }
    if !matches!(&start.return_type, Type::Named(name) if name == "VOID")
        && !matches!(&start.return_type, Type::Integer(_))
    {
        return Err(format!(
            "BUILD_LOWERING_UNAVAILABLE: Start return type '{}' is unsupported; LLVM entry point supports VOID or INTEGER",
            render_start_type(&start.return_type)
        ));
    }
    let functions = analyze_reachable(module, start)?;
    let mut text = String::from(
        "; Basic Next 0.2\n@.bn_fmt_int = private unnamed_addr constant [5 x i8] c\"%lld\\00\"\n@.bn_fmt_uint = private unnamed_addr constant [5 x i8] c\"%llu\\00\"\n@.bn_fmt_float = private unnamed_addr constant [6 x i8] c\"%.17g\\00\"\n@.bn_fmt_str = private unnamed_addr constant [3 x i8] c\"%s\\00\"\n@.bn_true = private unnamed_addr constant [5 x i8] c\"TRUE\\00\"\n@.bn_false = private unnamed_addr constant [6 x i8] c\"FALSE\\00\"\n@.bn_empty = private unnamed_addr constant [1 x i8] c\"\\00\"\n@.bn_eof = private unnamed_addr constant [4 x i8] c\"EOF\\00\"\n",
    );
    let rng_global = emit_preamble(&mut text, &functions, !wasm32);
    for (function, analysis) in &functions {
        emit_function(&mut text, module, function, analysis, rng_global, !wasm32)?;
    }
    Ok(text)
}

fn render_start_type(ty: &Type) -> &'static str {
    match ty {
        Type::Named(name) if name == "VOID" => "VOID",
        Type::Integer(_) => "INTEGER",
        Type::Named(_) => "named type",
        Type::Boolean => "BOOLEAN",
        Type::String => "STRING",
        Type::Float(_) | Type::FloatLiteral => "FLOAT",
        _ => "unsupported type",
    }
}

#[allow(clippy::too_many_lines)]
#[path = "llvm/analysis.rs"]
mod analysis;
use analysis::analyze_function;
fn llvm_type(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Boolean => Some("i1"),
        Type::Integer(IntegerType::Byte | IntegerType::Int8) => Some("i8"),
        Type::Integer(IntegerType::Int16 | IntegerType::UInt16) => Some("i16"),
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) => Some("i32"),
        // Untyped integer literals lower as i64 so UINT64/INT64 initializers
        // (including hex source text) stay in range; Store coerces to the slot.
        Type::IntegerLiteral(_) | Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => {
            Some("i64")
        }
        Type::Float(FloatType::Float32) => Some("float"),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some("double"),
        Type::String => Some("ptr"),
        Type::Named(name) if name == "DATE" || name == "TIME" => Some("i32"),
        Type::NotAvailable => Some("{ i1, double }"),
        Type::Alternative(alternatives) if float_or_na(alternatives) => Some("{ i1, double }"),
        // HOST.Net aggregate results OR Error, and narrowed network values.
        Type::Alternative(alternatives) if net_or_error(alternatives) => {
            if alternatives.iter().any(is_net_addresses_type) {
                Some("{ i1, ptr }")
            } else if alternatives.iter().any(is_net_endpoint_type) {
                Some("{ i1, ptr, i32 }")
            } else {
                Some("{ i1, ptr, i64 }")
            }
        }
        Type::Alternative(alternatives) if void_or_error(alternatives) => Some("{ i1, ptr, i64 }"),
        Type::Alternative(alternatives) if integer_or_error(alternatives) => {
            Some("{ i1, ptr, i64 }")
        }
        Type::Alternative(alternatives) if integer_eof_or_error(alternatives) => {
            Some("{ i1, ptr, i64 }")
        }
        Type::Alternative(alternatives) if imported_or_error(alternatives) => {
            Some("{ i1, ptr, i64 }")
        }
        Type::Alternative(alternatives) if opaque_or_error(alternatives) => {
            Some("{ i1, ptr, i64 }")
        }
        Type::Named(name) if name == "Error" => Some("{ i1, ptr, i64 }"),
        Type::Named(name) if name == "HOST.Net.Address" || name == "HOST.Net.PingReply" => {
            Some("{ i1, ptr, i64 }")
        }
        Type::Named(name) if name == "HOST.Net.Addresses" => Some("{ i1, ptr }"),
        Type::Named(name) if name == "HOST.Net.Endpoint" => Some("{ ptr, i32 }"),
        Type::Named(name)
            if matches!(
                name.as_str(),
                "HOST.Net.TCPStream"
                    | "HOST.Net.TCPListener"
                    | "HOST.Net.UDPSocket"
                    | "HOST.Net.UDPPacket"
            ) =>
        {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { name, .. }
            if name == "Address" || name == "PingReply" || name == "Error" =>
        {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { name, .. } if name == "Addresses" => Some("{ i1, ptr }"),
        Type::ImportedNamed { name, .. } if name == "Endpoint" => Some("{ ptr, i32 }"),
        Type::ImportedNamed { name, .. }
            if matches!(
                name.as_str(),
                "TCPStream" | "TCPListener" | "UDPSocket" | "UDPPacket"
            ) =>
        {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { name, .. } if dispatch_handle_name(name) => Some("{ i1, ptr, i64 }"),
        Type::ImportedTypeName { name, .. } if dispatch_handle_name(name) => {
            Some("{ i1, ptr, i64 }")
        }
        Type::ImportedNamed { .. } => Some("ptr"),
        Type::ImportedTypeName { .. } => Some("ptr"),
        Type::Vector {
            element,
            dimensions,
        } if dimensions.len() == 1 && dimensions[0] != u64::MAX && llvm_type(element).is_some() => {
            Some("{ ptr, i32 }")
        }
        // Dynamic `NEW T[n]` / `POINTER TO T[]` share the vector fat pointer.
        Type::Pointer { element, .. } if llvm_type(element).is_some() => Some("{ ptr, i32 }"),
        Type::Named(name) if name == "POINTER" => Some("{ ptr, i32 }"),
        // User class instances (NEW Box(...)) lower as opaque pointers.
        Type::Named(name)
            if !matches!(
                name.as_str(),
                "VOID" | "POINTER" | "DATE" | "TIME" | "Error"
            ) =>
        {
            Some("ptr")
        }
        _ => None,
    }
}

fn float_or_na(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::Float(_) | Type::FloatLiteral))
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::NotAvailable))
}

fn is_error_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "Error")
        || matches!(ty, Type::ImportedNamed { name, .. } if name == "Error")
}

fn is_net_address_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "HOST.Net.Address" || name == "Address")
        || matches!(ty, Type::ImportedNamed { name, .. } if name == "Address")
}

fn is_net_ping_reply_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "HOST.Net.PingReply" || name == "PingReply")
        || matches!(ty, Type::ImportedNamed { name, .. } if name == "PingReply")
}

fn is_net_addresses_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "HOST.Net.Addresses" || name == "Addresses")
        || matches!(ty, Type::ImportedNamed { name, .. } if name == "Addresses")
}

fn is_net_endpoint_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "HOST.Net.Endpoint" || name == "Endpoint")
        || matches!(ty, Type::ImportedNamed { name, .. } if name == "Endpoint")
}

fn is_net_handle_type(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if matches!(name.as_str(), "HOST.Net.TCPStream" | "HOST.Net.TCPListener" | "HOST.Net.UDPSocket" | "HOST.Net.UDPPacket"))
        || matches!(ty, Type::ImportedNamed { name, .. } if matches!(name.as_str(), "TCPStream" | "TCPListener" | "UDPSocket" | "UDPPacket"))
}

fn net_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives.iter().any(is_error_type)
        && alternatives.iter().any(|ty| {
            is_net_address_type(ty)
                || is_net_ping_reply_type(ty)
                || is_net_addresses_type(ty)
                || is_net_endpoint_type(ty)
                || is_net_handle_type(ty)
                || matches!(ty, Type::String)
        })
}

fn void_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::Named(name) if name == "VOID"))
        && alternatives.iter().any(is_error_type)
}

fn integer_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives.iter().any(|ty| matches!(ty, Type::Integer(_)))
        && alternatives.iter().any(is_error_type)
}

fn integer_eof_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 3
        && alternatives.iter().any(|ty| matches!(ty, Type::Integer(_)))
        && alternatives.iter().any(|ty| matches!(ty, Type::EndOfFile))
        && alternatives.iter().any(is_error_type)
}

fn imported_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::Named(name) if name == "Error"))
        && alternatives.iter().any(|ty| {
            matches!(
                ty,
                Type::ImportedNamed { .. } | Type::ImportedTypeName { .. }
            )
        })
}

fn opaque_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::Named(name) if name == "Error"))
        && alternatives.iter().any(|ty| {
            matches!(
                ty,
                Type::Named(_) | Type::ImportedNamed { .. } | Type::ImportedTypeName { .. }
            )
        })
}

fn dispatch_handle_name(name: &str) -> bool {
    matches!(
        name,
        "Queue" | "Ticket" | "Group" | "Barrier" | "Semaphore" | "Mutex"
    )
}

fn is_int_vector(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Vector { element, dimensions }
            if dimensions.len() == 1
                && dimensions[0] != u64::MAX
                && matches!(element.as_ref(), Type::Integer(IntegerType::Int32) | Type::IntegerLiteral(_))
    )
}

fn is_int_pointer(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Pointer { element, .. }
            if matches!(
                element.as_ref(),
                Type::Integer(IntegerType::Int32) | Type::IntegerLiteral(_)
            )
    ) || matches!(ty, Type::Named(name) if name == "POINTER")
}

fn printable_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Boolean
            | Type::String
            | Type::Integer(_)
            | Type::IntegerLiteral(_)
            | Type::Float(_)
            | Type::FloatLiteral
            | Type::Named(_)
            | Type::NotAvailable
            | Type::Alternative(_)
    ) && llvm_type(ty).is_some()
}

fn unary_supported(operator: &str, operand: Option<&Type>, ty: &Type) -> bool {
    let Some(operand) = operand else {
        return false;
    };
    matches!(
        (operator, llvm_type(operand), llvm_type(ty)),
        (
            "Plus" | "Minus" | "NOT",
            Some("i8" | "i16" | "i32" | "i64"),
            Some("i8" | "i16" | "i32" | "i64")
        ) | ("Plus" | "Minus", Some("float"), Some("float"))
            | ("Plus" | "Minus", Some("double"), Some("double"))
            | ("NOT", Some("i1"), Some("i1"))
    )
}

fn integer_llvm(llvm_ty: &str) -> bool {
    matches!(llvm_ty, "i8" | "i16" | "i32" | "i64")
}

fn float_llvm(llvm_ty: &str) -> bool {
    matches!(llvm_ty, "float" | "double")
}

fn binary_supported(operator: &str, left: &Type, right: &Type, result: &Type) -> bool {
    let Some(left_llvm) = llvm_type(left) else {
        return false;
    };
    let Some(right_llvm) = llvm_type(right) else {
        return false;
    };
    let Some(result_llvm) = llvm_type(result) else {
        return false;
    };
    match operator {
        "Plus" | "Minus" | "Star" | "Multiply" => {
            integer_llvm(left_llvm) && integer_llvm(right_llvm) && integer_llvm(result_llvm)
                || float_llvm(left_llvm) && float_llvm(right_llvm) && float_llvm(result_llvm)
                || operator == "Plus"
                    && left_llvm == "ptr"
                    && right_llvm == "ptr"
                    && result_llvm == "ptr"
        }
        "Slash" | "Divide" => {
            float_llvm(left_llvm) && float_llvm(right_llvm) && float_llvm(result_llvm)
                || integer_llvm(left_llvm) && integer_llvm(right_llvm) && float_llvm(result_llvm)
        }
        // IntegerLiteral is i64; expression results may be narrower INTEGER/BYTE/….
        // Emission coerces operands to the result width.
        "DIV" | "Percent" | "SHL" | "SHR" | "Power" => {
            integer_llvm(left_llvm) && integer_llvm(right_llvm) && integer_llvm(result_llvm)
                || operator == "Power"
                    && float_llvm(left_llvm)
                    && float_llvm(right_llvm)
                    && float_llvm(result_llvm)
        }
        "AND" | "OR" | "XOR" => {
            integer_llvm(left_llvm) && integer_llvm(right_llvm) && integer_llvm(result_llvm)
                || left_llvm == "i1" && right_llvm == "i1" && result_llvm == "i1"
        }
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual" => {
            result_llvm == "i1"
                && (left_llvm == right_llvm
                    || integer_llvm(left_llvm) && integer_llvm(right_llvm)
                    || float_llvm(left_llvm) && float_llvm(right_llvm))
        }
        _ => false,
    }
}

fn cast_supported(source: Option<&Type>, target: &Type) -> bool {
    let Some(source) = source else {
        return false;
    };
    matches!(
        (source, target),
        (
            Type::Integer(_) | Type::IntegerLiteral(_),
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::Boolean
        ) | (
            Type::Float(_) | Type::FloatLiteral,
            Type::Float(_) | Type::Integer(_) | Type::Boolean
        ) | (Type::Boolean, Type::Boolean)
            | (Type::String, Type::Boolean | Type::String)
    )
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
#[path = "llvm/emission1.rs"]
mod emission1;
use emission1::lower_scalar_instruction;
#[path = "llvm/emission2.rs"]
mod emission2;
use emission2::{
    checked_intrinsic_declaration, emit_checked_integer_op, float_compare_opcode,
    integer_compare_opcode, lower_print_value, lower_terminator,
};
#[path = "llvm/casts.rs"]
mod casts;
use casts::lower_cast;
#[path = "llvm/euclidean.rs"]
mod euclidean;
use euclidean::emit_euclidean_integer_op;
#[path = "llvm/power_shift.rs"]
mod power_shift;
use power_shift::{
    STRING_CONCAT_DECLS, emit_float_power, emit_integer_not, emit_integer_power, emit_shift,
    emit_string_concat, pow_intrinsic_declaration,
};
#[path = "llvm/binary.rs"]
mod binary;
use binary::emit_runtime_binary;
#[path = "llvm/runtime.rs"]
mod runtime;
use runtime::{
    BN_RT_DECLS, bn_rt_call_supported, emit_handle_result, emit_void_result, is_bn_rt_host_call,
    lower_bn_dispatch_call, lower_bn_rt_call,
};
#[path = "llvm/math.rs"]
mod math;
use math::{BN_RT_MATH_DECLS, bnmath_call_supported, bnmath_method, lower_bnmath_call};
#[path = "llvm/vectors.rs"]
mod vectors;
use vectors::{
    emit_allocate, emit_delete, emit_is, emit_member, emit_optional_float_default,
    emit_pointer_set_index, emit_set_member, emit_store_object_class, emit_vector,
    emit_vector_index, emit_vector_length, extract_optional_float,
};
#[path = "llvm/functions.rs"]
mod functions;
use functions::{
    analyze_reachable, emit_function, emit_preamble, is_void_type, llvm_function_symbol,
    lower_user_call, string_global,
};
#[path = "llvm/emission3.rs"]
mod emission3;
use emission3::{
    define_boolean, define_boolean_from, emit_constant_assignment, emit_constant_value,
    emit_constant_value_analyzed, i1_operand,
};

mod helpers;
use helpers::{
    OBJECT_HEADER_BYTES, class_init_flag, class_instance_bytes, coerce_return_operand,
    coerce_to_type, escape_llvm, extend_to_i64, field_byte_offset, fold_binary, fold_cast,
    fold_unary, input_runtime_ir, instruction_name, integer_kind, is_unsigned,
    parse_float_constant, parse_integer, render_float, render_llvm_integer, sanitize_symbol,
    static_global_name, unsupported_call_detail, unsupported_instruction,
    unsupported_instruction_detail,
};
#[cfg(test)]
mod tests {
    use super::{helpers::render_llvm_integer, llvm_type};
    use crate::semantic::{FloatType, IntegerType, Type};

    #[test]
    fn llvm_type_uses_contract_integer_widths() {
        assert_eq!(llvm_type(&Type::Integer(IntegerType::Int32)), Some("i32"));
        assert_eq!(llvm_type(&Type::Integer(IntegerType::Byte)), Some("i8"));
        assert_eq!(llvm_type(&Type::Integer(IntegerType::UInt64)), Some("i64"));
    }

    #[test]
    fn llvm_type_uses_contract_float_widths() {
        assert_eq!(llvm_type(&Type::Float(FloatType::Float32)), Some("float"));
        assert_eq!(llvm_type(&Type::Float(FloatType::Float64)), Some("double"));
    }

    #[test]
    fn llvm_integer_rendering_preserves_signed_bit_patterns_without_cast_wrap() {
        assert_eq!(render_llvm_integer(255, "i8"), "-1");
        assert_eq!(render_llvm_integer(-129, "i8"), "127");
        assert_eq!(render_llvm_integer(65_536, "i16"), "0");
        assert_eq!(
            render_llvm_integer(-9_223_372_036_854_775_809, "i64"),
            "9223372036854775807"
        );
    }
}
