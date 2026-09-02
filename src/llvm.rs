// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dependency-free LLVM textual backend. Unsupported IR is rejected explicitly.

use std::{
    collections::{BTreeSet, HashMap},
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

struct LoweringAnalysis<'a> {
    values: HashMap<ValueId, Type>,
    symbols: HashMap<SymbolId, Type>,
    functions: HashMap<ValueId, &'a str>,
    strings: Vec<(ValueId, String)>,
    input_count: usize,
    uses_random: bool,
    intrinsics: BTreeSet<&'static str>,
}

struct BlockState {
    constants: HashMap<ValueId, ConstantValue>,
    bindings: HashMap<SymbolId, ConstantValue>,
}

struct EmissionState {
    print_count: usize,
    continuation_count: usize,
    needs_numeric_overflow_trap: bool,
}

/// Lower the supported typed BN IR subset to LLVM IR.
///
/// # Errors
///
/// Returns a diagnostic when the module contains unsupported IR or has no
/// executable entry point.
pub fn lower_module(module: &Module) -> Result<String, String> {
    let Some(start) = module
        .functions
        .iter()
        .find(|function| function.name == "Start")
    else {
        return Err("BUILD_ENTRYPOINT_MISSING: module has no Start function".into());
    };
    lower_scalar_ir(module, start)
}

#[allow(clippy::too_many_lines)]
fn lower_scalar_ir(module: &Module, function: &Function) -> Result<String, String> {
    if !function.parameters.is_empty() {
        return Err(format!(
            "BUILD_LOWERING_UNAVAILABLE: Start has {} parameter(s); LLVM entry point requires FUNCTION Start()",
            function.parameters.len()
        ));
    }
    if !matches!(&function.return_type, Type::Named(name) if name == "VOID")
        && !matches!(&function.return_type, Type::Integer(_))
    {
        return Err(format!(
            "BUILD_LOWERING_UNAVAILABLE: Start return type '{}' is unsupported; LLVM entry point supports VOID or INTEGER",
            render_start_type(&function.return_type)
        ));
    }
    let analysis = analyze_function(module, function)?;
    let symbol_names = analysis
        .symbols
        .keys()
        .enumerate()
        .map(|(index, symbol)| (*symbol, index))
        .collect::<HashMap<_, _>>();
    let mut text = String::from(
        "; Basic Next 0.2\n@.bn_fmt_int = private unnamed_addr constant [5 x i8] c\"%lld\\00\"\n@.bn_fmt_float = private unnamed_addr constant [6 x i8] c\"%.17g\\00\"\n@.bn_fmt_str = private unnamed_addr constant [3 x i8] c\"%s\\00\"\n@.bn_true = private unnamed_addr constant [5 x i8] c\"TRUE\\00\"\n@.bn_false = private unnamed_addr constant [6 x i8] c\"FALSE\\00\"\n",
    );
    for (value, string) in &analysis.strings {
        let _ = writeln!(
            text,
            "@.bn_str{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            value.0,
            string.len() + 1,
            escape_llvm(string)
        );
    }
    if analysis.input_count > 0 {
        text.push_str(input_runtime_ir());
    }
    text.push_str("\ndeclare i32 @printf(ptr, ...)\ndeclare i32 @putchar(i32)\n");
    for intrinsic in &analysis.intrinsics {
        let _ = writeln!(text, "declare {intrinsic}");
    }
    text.push_str("\ndefine i32 @main(i32 %argc, ptr %argv) {\n");
    let mut state = EmissionState {
        print_count: 0,
        continuation_count: 0,
        needs_numeric_overflow_trap: false,
    };
    for block in &function.blocks {
        let _ = writeln!(text, "b{}:", block.id.0);
        if block.id == function.entry {
            for (symbol, ty) in &analysis.symbols {
                let llvm_ty = llvm_type(ty).expect("validated alloca type");
                let _ = writeln!(text, "  %s{} = alloca {llvm_ty}", symbol_names[symbol]);
            }
            if analysis.uses_random {
                text.push_str("  %rng = alloca i64\n  store i64 1, ptr %rng\n");
            }
        }
        let mut block_state = BlockState {
            constants: HashMap::new(),
            bindings: HashMap::new(),
        };
        for instruction in &block.instructions {
            lower_scalar_instruction(
                &mut text,
                module,
                function,
                block.id,
                instruction,
                &analysis,
                &symbol_names,
                &mut block_state,
                &mut state,
            )?;
        }
        lower_terminator(&mut text, &block.terminator, &analysis, &mut block_state);
    }
    if state.needs_numeric_overflow_trap {
        text.push_str("trap_numeric_overflow:\n  ret i32 1\n");
    }
    text.push_str("}\n");
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
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) | Type::IntegerLiteral(_) => {
            Some("i32")
        }
        Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => Some("i64"),
        Type::Float(FloatType::Float32) => Some("float"),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some("double"),
        Type::String => Some("ptr"),
        _ => None,
    }
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
    )
}

fn unary_supported(operator: &str, operand: Option<&Type>, ty: &Type) -> bool {
    let Some(operand) = operand else {
        return false;
    };
    matches!(
        (operator, llvm_type(operand), llvm_type(ty)),
        (
            "Plus" | "Minus",
            Some("i8" | "i16" | "i32" | "i64"),
            Some("i8" | "i16" | "i32" | "i64")
        ) | ("Plus" | "Minus", Some("float"), Some("float"))
            | ("Plus" | "Minus", Some("double"), Some("double"))
            | ("NOT", Some("i1"), Some("i1"))
    )
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
            left_llvm == right_llvm && result_llvm == left_llvm
        }
        "Slash" | "Divide" => {
            left_llvm == right_llvm
                && result_llvm == left_llvm
                && matches!(left_llvm, "float" | "double")
        }
        "AND" | "OR" | "XOR" => left_llvm == right_llvm && result_llvm == left_llvm,
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" | "Equal" | "Assign" | "NotEqual" => {
            left_llvm == right_llvm && result_llvm == "i1"
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
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_)
        ) | (
            Type::Float(_) | Type::FloatLiteral,
            Type::Float(_) | Type::Integer(_)
        ) | (Type::Boolean, Type::Boolean)
            | (Type::String, Type::String)
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
    integer_compare_opcode, lower_cast, lower_print_value, lower_terminator,
};
#[path = "llvm/emission3.rs"]
mod emission3;
use emission3::{emit_boolean_assignment, emit_constant_assignment, emit_constant_value};

mod helpers;
use helpers::{
    coerce_return_operand, escape_llvm, extend_to_i64, fold_binary, fold_cast, fold_unary,
    input_runtime_ir, instruction_name, integer_kind, integer_width_from_llvm, is_unsigned,
    parse_integer, render_float, unsupported_call_detail, unsupported_instruction,
};
#[cfg(test)]
mod tests {
    use super::llvm_type;
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
}
