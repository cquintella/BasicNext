// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dependency-free LLVM textual backend. Unsupported IR is rejected explicitly.

use std::{collections::HashMap, fmt::Write as _};

use crate::{
    ir::{Constant, Instruction, Module, Terminator, ValueId},
    semantic::Type,
};

#[derive(Clone)]
enum ConstantValue {
    Integer(i64),
    Float(f64),
    Boolean(bool),
    String(String),
    Newline,
    Function(String),
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
    if let Some(values) = evaluate_constant_ir(module, start) {
        return Ok(if values.is_empty() {
            empty_module()
        } else {
            print_integer_module(&values)
        });
    }
    match lower_scalar_ir(start) {
        Ok(module) => Ok(module),
        Err(unsupported) => Err(format!(
            "BUILD_LOWERING_UNAVAILABLE: LLVM lowering for {unsupported} is not implemented"
        )),
    }
}

/// Lower the scalar, non-host subset directly instead of requiring values to
/// be known at build time. Complex object/vector operations keep the explicit
/// unavailable diagnostic below.
#[allow(clippy::too_many_lines)]
fn lower_scalar_ir(function: &crate::ir::Function) -> Result<String, &'static str> {
    if !function.parameters.is_empty()
        || !matches!(&function.return_type, Type::Named(name) if name == "VOID")
    {
        return Err("the Start signature");
    }
    let mut values = HashMap::<ValueId, Type>::new();
    let mut symbols = HashMap::new();
    let mut functions = HashMap::new();
    let mut uses_random = false;
    let mut seeds_random = false;
    let mut input_count = 0;
    let mut strings = Vec::new();
    for block in &function.blocks {
        for instruction in &block.instructions {
            match instruction {
                Instruction::Constant {
                    destination,
                    value,
                    ty,
                    ..
                } => {
                    if let Constant::Function(name) = value {
                        functions.insert(*destination, name.as_str());
                        if matches!(name.as_str(), "HOST.Random.Random" | "HOST.Random.Seed") {
                            uses_random = true;
                        }
                        continue;
                    }
                    if matches!(
                        value,
                        Constant::HostConsole
                            | Constant::Null
                            | Constant::NotAvailable
                            | Constant::EndOfFile
                    ) {
                        return Err(instruction_name(instruction));
                    }
                    if matches!(value, Constant::Type(_)) {
                        continue;
                    }
                    values.insert(*destination, ty.clone());
                    if let Constant::String(value) = value {
                        strings.push((*destination, value.clone()));
                    }
                }
                Instruction::Default {
                    destination,
                    ty,
                    dimensions,
                    ..
                } if dimensions.is_empty() => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Load {
                    destination,
                    symbol,
                    ty,
                    ..
                } => {
                    let ty = symbols.get(symbol).cloned().unwrap_or_else(|| ty.clone());
                    values.insert(*destination, ty.clone());
                    symbols.entry(*symbol).or_insert(ty);
                }
                Instruction::Store {
                    symbol, value, ty, ..
                } => {
                    symbols.entry(*symbol).or_insert_with(|| {
                        values.get(value).cloned().unwrap_or_else(|| ty.clone())
                    });
                }
                Instruction::Copy {
                    destination, ty, ..
                }
                | Instruction::Unary {
                    destination, ty, ..
                }
                | Instruction::Binary {
                    destination, ty, ..
                }
                | Instruction::Cast {
                    destination, ty, ..
                } => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Call {
                    destination,
                    callee,
                    ty,
                    ..
                } if functions.contains_key(callee) => {
                    seeds_random |= functions.get(callee) == Some(&"HOST.Random.Seed");
                    if !matches!(ty, Type::Named(name) if name == "VOID") {
                        values.insert(*destination, ty.clone());
                    }
                }
                Instruction::Input { destination, .. } => {
                    input_count += 1;
                    values.insert(*destination, Type::String);
                }
                Instruction::Length {
                    destination,
                    vector,
                    ..
                } if values.get(vector) == Some(&Type::HostArgs) => {
                    values.insert(
                        *destination,
                        Type::Integer(crate::semantic::IntegerType::Int32),
                    );
                }
                Instruction::Index {
                    destination,
                    object,
                    ty,
                    ..
                } if values.get(object) == Some(&Type::HostArgs) => {
                    values.insert(*destination, ty.clone());
                }
                Instruction::Print { .. } => {}
                _ => return Err(instruction_name(instruction)),
            }
        }
    }
    if uses_random && !seeds_random {
        return Err("calls");
    }
    if symbols.values().any(|ty| llvm_type(ty).is_none())
        || values
            .values()
            .any(|ty| *ty != Type::HostArgs && llvm_type(ty).is_none())
    {
        return Err("value types");
    }

    let symbol_names = symbols
        .keys()
        .enumerate()
        .map(|(index, symbol)| (*symbol, index))
        .collect::<HashMap<_, _>>();
    let mut text = String::from(
        "; Basic Next 0.2\n@.bn_fmt_int = private unnamed_addr constant [5 x i8] c\"%lld\\00\"\n@.bn_fmt_float = private unnamed_addr constant [6 x i8] c\"%.17g\\00\"\n@.bn_fmt_str = private unnamed_addr constant [3 x i8] c\"%s\\00\"\n@.bn_true = private unnamed_addr constant [5 x i8] c\"TRUE\\00\"\n@.bn_false = private unnamed_addr constant [6 x i8] c\"FALSE\\00\"\n",
    );
    for (value, string) in &strings {
        let _ = writeln!(
            text,
            "@.bn_str{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            value.0,
            string.len() + 1,
            escape_llvm(string)
        );
    }
    if input_count > 0 {
        text.push_str(input_runtime_ir());
    }
    text.push_str("\ndeclare i32 @printf(ptr, ...)\ndeclare i32 @putchar(i32)\n");
    text.push_str("\ndefine i32 @main(i32 %argc, ptr %argv) {\n");
    let mut print_count = 0;
    for block in &function.blocks {
        let _ = writeln!(text, "b{}:", block.id.0);
        if block.id == function.entry {
            for (symbol, ty) in &symbols {
                let _ = writeln!(
                    text,
                    "  %s{} = alloca {}",
                    symbol_names[symbol],
                    llvm_type(ty).ok_or("stores")?
                );
            }
            if uses_random {
                text.push_str("  %rng = alloca i64\n  store i64 1, ptr %rng\n");
            }
        }
        for instruction in &block.instructions {
            lower_scalar_instruction(
                &mut text,
                instruction,
                &values,
                &symbol_names,
                &functions,
                &mut print_count,
            )
            .ok_or_else(|| instruction_name(instruction))?;
        }
        match block.terminator {
            Terminator::Jump { target } => {
                let _ = writeln!(text, "  br label %b{}", target.0);
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let _ = writeln!(
                    text,
                    "  br i1 %v{}, label %b{}, label %b{}",
                    condition.0, then_block.0, else_block.0
                );
            }
            Terminator::Return { value: None } => text.push_str("  ret i32 0\n"),
            Terminator::Return { value: Some(value) } => {
                let _ = writeln!(text, "  ret i32 %v{}", value.0);
            }
            Terminator::Stop { code } => {
                let _ = writeln!(text, "  ret i32 %v{}", code.0);
            }
        }
    }
    text.push_str("}\n");
    Ok(text)
}

fn llvm_type(ty: &Type) -> Option<&'static str> {
    match ty {
        Type::Boolean => Some("i1"),
        Type::Integer(_) | Type::IntegerLiteral(_) => Some("i64"),
        Type::Float(_) | Type::FloatLiteral => Some("double"),
        Type::String => Some("ptr"),
        _ => None,
    }
}

#[allow(clippy::too_many_lines)]
fn lower_scalar_instruction(
    text: &mut String,
    instruction: &Instruction,
    values: &HashMap<ValueId, Type>,
    symbols: &HashMap<crate::semantic::SymbolId, usize>,
    functions: &HashMap<ValueId, &str>,
    print_count: &mut usize,
) -> Option<()> {
    match instruction {
        Instruction::Constant {
            destination, value, ..
        } => match value {
            Constant::Integer(value) => {
                let _ = writeln!(text, "  %v{} = add i64 0, {value}", destination.0);
            }
            Constant::Float(value) => {
                let _ = writeln!(text, "  %v{} = fadd double 0.0, {value}", destination.0);
            }
            Constant::Boolean(value) => {
                let _ = writeln!(
                    text,
                    "  %v{} = or i1 0, {}",
                    destination.0,
                    i32::from(*value)
                );
            }
            Constant::String(_) => {
                let _ = writeln!(
                    text,
                    "  %v{} = getelementptr i8, ptr @.bn_str{}, i64 0",
                    destination.0, destination.0
                );
            }
            Constant::HostArgs | Constant::Type(_) | Constant::Function(_) => {}
            _ => return None,
        },
        Instruction::Default {
            destination,
            ty,
            dimensions,
            ..
        } if dimensions.is_empty() => match llvm_type(ty)? {
            "i1" | "i64" => {
                let _ = writeln!(text, "  %v{} = add {} 0, 0", destination.0, llvm_type(ty)?);
            }
            "double" => {
                let _ = writeln!(text, "  %v{} = fadd double 0.0, 0.0", destination.0);
            }
            "ptr" => {
                let _ = writeln!(text, "  %v{} = inttoptr i64 0 to ptr", destination.0);
            }
            _ => return None,
        },
        Instruction::Store { symbol, value, .. } => {
            let _ = writeln!(
                text,
                "  store {} %v{}, ptr %s{}",
                llvm_type(values.get(value)?)?,
                value.0,
                symbols.get(symbol)?
            );
        }
        Instruction::Load {
            destination,
            symbol,
            ..
        } => {
            let _ = writeln!(
                text,
                "  %v{} = load {}, ptr %s{}",
                destination.0,
                llvm_type(values.get(destination)?)?,
                symbols.get(symbol)?
            );
        }
        Instruction::Copy {
            destination,
            source,
            ty,
            ..
        } => match llvm_type(ty)? {
            "i64" => {
                let _ = writeln!(text, "  %v{} = add i64 0, %v{}", destination.0, source.0);
            }
            "double" => {
                let _ = writeln!(
                    text,
                    "  %v{} = fadd double 0.0, %v{}",
                    destination.0, source.0
                );
            }
            "i1" => {
                let _ = writeln!(text, "  %v{} = or i1 false, %v{}", destination.0, source.0);
            }
            "ptr" => {
                let _ = writeln!(
                    text,
                    "  %v{} = getelementptr i8, ptr %v{}, i64 0",
                    destination.0, source.0
                );
            }
            _ => return None,
        },
        Instruction::Unary {
            destination,
            operator,
            operand,
            ty,
            ..
        } => {
            let op = match (operator.as_str(), llvm_type(ty)?) {
                ("Minus", "i64") => "sub i64 0,",
                ("Minus", "double") => "fsub double 0.0,",
                ("Plus", "i64") => "add i64 0,",
                ("Plus", "double") => "fadd double 0.0,",
                ("NOT", "i1") => "xor i1 1,",
                _ => return None,
            };
            let _ = writeln!(text, "  %v{} = {op} %v{}", destination.0, operand.0);
        }
        Instruction::Binary {
            destination,
            operator,
            left,
            right,
            ty,
            ..
        } => {
            let left_ty = llvm_type(values.get(left)?)?;
            if left_ty == "i64"
                && matches!(
                    operator.as_str(),
                    "Plus" | "Minus" | "Star" | "DIV" | "Percent" | "SHL" | "SHR"
                )
            {
                return None;
            }
            let op = match (operator.as_str(), left_ty, llvm_type(ty)?) {
                ("Plus", "i64", _) => "add i64",
                ("Minus", "i64", _) => "sub i64",
                ("Star" | "Multiply", "i64", _) => "mul i64",
                ("SHL", "i64", _) => "shl i64",
                ("AND", "i64", _) => "and i64",
                ("OR", "i64", _) => "or i64",
                ("XOR", "i64", _) => "xor i64",
                ("Plus", "double", _) => "fadd double",
                ("Minus", "double", _) => "fsub double",
                ("Star" | "Multiply", "double", _) => "fmul double",
                ("Slash", "double", _) => "fdiv double",
                ("AND", "i1", _) => "and i1",
                ("OR", "i1", _) => "or i1",
                ("XOR", "i1", _) => "xor i1",
                ("Less", "i64", "i1") => "icmp slt i64",
                ("LessEqual", "i64", "i1") => "icmp sle i64",
                ("Greater", "i64", "i1") => "icmp sgt i64",
                ("GreaterEqual", "i64", "i1") => "icmp sge i64",
                ("Equal", "i64", "i1") => "icmp eq i64",
                ("NotEqual", "i64", "i1") => "icmp ne i64",
                ("Less", "double", "i1") => "fcmp olt double",
                ("LessEqual", "double", "i1") => "fcmp ole double",
                ("Greater", "double", "i1") => "fcmp ogt double",
                ("GreaterEqual", "double", "i1") => "fcmp oge double",
                ("Equal", "double", "i1") => "fcmp oeq double",
                ("NotEqual", "double", "i1") => "fcmp one double",
                ("Equal", "i1", "i1") => "icmp eq i1",
                ("NotEqual", "i1", "i1") => "icmp ne i1",
                _ => return None,
            };
            let _ = writeln!(
                text,
                "  %v{} = {op} %v{}, %v{}",
                destination.0, left.0, right.0
            );
        }
        Instruction::Call {
            destination,
            callee,
            arguments,
            ..
        } => match functions.get(callee).copied()? {
            "HOST.Random.Seed" if arguments.len() == 1 => {
                let _ = writeln!(
                    text,
                    "  %rngzero{} = icmp eq i64 %v{}, 0",
                    destination.0, arguments[0].0
                );
                let _ = writeln!(
                    text,
                    "  %rngseed{} = select i1 %rngzero{}, i64 1, i64 %v{}",
                    destination.0, destination.0, arguments[0].0
                );
                let _ = writeln!(text, "  store i64 %rngseed{}, ptr %rng", destination.0);
            }
            "HOST.Random.Random" if arguments.is_empty() => {
                let _ = writeln!(text, "  %rng0{} = load i64, ptr %rng", destination.0);
                let _ = writeln!(
                    text,
                    "  %rng1{} = lshr i64 %rng0{}, 12",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng2{} = xor i64 %rng0{}, %rng1{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng3{} = shl i64 %rng2{}, 25",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng4{} = xor i64 %rng2{}, %rng3{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng5{} = lshr i64 %rng4{}, 27",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rng6{} = xor i64 %rng4{}, %rng5{}",
                    destination.0, destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rngnext{} = mul i64 %rng6{}, 2685821657736338717",
                    destination.0, destination.0
                );
                let _ = writeln!(text, "  store i64 %rngnext{}, ptr %rng", destination.0);
                let _ = writeln!(
                    text,
                    "  %rngscale{} = lshr i64 %rngnext{}, 11",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %rngfloat{} = uitofp i64 %rngscale{} to double",
                    destination.0, destination.0
                );
                let _ = writeln!(
                    text,
                    "  %v{} = fdiv double %rngfloat{}, 9007199254740992.0",
                    destination.0, destination.0
                );
            }
            _ => return None,
        },
        Instruction::Input { destination, .. } => {
            let _ = writeln!(text, "  %v{} = call ptr @bn_input()", destination.0);
        }
        Instruction::Length {
            destination,
            vector,
            ..
        } if values.get(vector) == Some(&Type::HostArgs) => {
            let _ = writeln!(text, "  %v{} = sext i32 %argc to i64", destination.0);
        }
        Instruction::Index {
            destination,
            object,
            index,
            ..
        } if values.get(object) == Some(&Type::HostArgs) => {
            let _ = writeln!(
                text,
                "  %argindex{} = trunc i64 %v{} to i32",
                destination.0, index.0
            );
            let _ = writeln!(
                text,
                "  %argptr{} = getelementptr ptr, ptr %argv, i32 %argindex{}",
                destination.0, destination.0
            );
            let _ = writeln!(
                text,
                "  %v{} = load ptr, ptr %argptr{}",
                destination.0, destination.0
            );
        }
        Instruction::Print {
            values: printed, ..
        } => {
            for value in printed {
                match llvm_type(values.get(value)?)? {
                    "i64" => {
                        let _ = writeln!(
                            text,
                            "  %print{print_count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 %v{})",
                            value.0
                        );
                    }
                    "double" => {
                        let _ = writeln!(
                            text,
                            "  %print{print_count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_float, double %v{})",
                            value.0
                        );
                    }
                    "ptr" => {
                        let _ = writeln!(
                            text,
                            "  %print{print_count} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr %v{})",
                            value.0
                        );
                    }
                    _ => return None,
                }
                *print_count += 1;
            }
            let _ = writeln!(text, "  %newline{print_count} = call i32 @putchar(i32 10)");
            *print_count += 1;
        }
        _ => return None,
    }
    Some(())
}

fn input_runtime_ir() -> &'static str {
    r#"
@.bn_eof = private unnamed_addr constant [4 x i8] c"EOF\00"
declare i32 @getchar()
declare ptr @realloc(ptr, i64)

define ptr @bn_input() {
entry:
  %input.initial = call ptr @realloc(ptr null, i64 64)
  br label %input.read
input.read:
  %input.buffer = phi ptr [ %input.initial, %entry ], [ %input.active.buffer, %input.store ], [ %input.buffer, %input.carriage ]
  %input.capacity = phi i64 [ 64, %entry ], [ %input.active.capacity, %input.store ], [ %input.capacity, %input.carriage ]
  %input.index = phi i64 [ 0, %entry ], [ %input.next, %input.store ], [ %input.index, %input.carriage ]
  %input.char = call i32 @getchar()
  %input.eof = icmp eq i32 %input.char, -1
  br i1 %input.eof, label %input.eof.check, label %input.line.check
input.eof.check:
  %input.empty = icmp eq i64 %input.index, 0
  br i1 %input.empty, label %input.eof.out, label %input.done
input.line.check:
  %input.newline = icmp eq i32 %input.char, 10
  %input.cr = icmp eq i32 %input.char, 13
  br i1 %input.newline, label %input.done, label %input.cr.check
input.cr.check:
  br i1 %input.cr, label %input.carriage, label %input.store.check
input.carriage:
  br label %input.read
input.store.check:
  %input.required = add i64 %input.index, 2
  %input.must.grow = icmp ugt i64 %input.required, %input.capacity
  br i1 %input.must.grow, label %input.grow, label %input.keep
input.grow:
  %input.new.capacity = shl i64 %input.capacity, 1
  %input.grown = call ptr @realloc(ptr %input.buffer, i64 %input.new.capacity)
  br label %input.store
input.keep:
  br label %input.store
input.store:
  %input.active.buffer = phi ptr [ %input.grown, %input.grow ], [ %input.buffer, %input.keep ]
  %input.active.capacity = phi i64 [ %input.new.capacity, %input.grow ], [ %input.capacity, %input.keep ]
  %input.byte = trunc i32 %input.char to i8
  %input.slot = getelementptr i8, ptr %input.active.buffer, i64 %input.index
  store i8 %input.byte, ptr %input.slot
  %input.next = add i64 %input.index, 1
  br label %input.read
input.done:
  %input.end = getelementptr i8, ptr %input.buffer, i64 %input.index
  store i8 0, ptr %input.end
  ret ptr %input.buffer
input.eof.out:
  ret ptr @.bn_eof
}
"#
}

#[allow(clippy::too_many_lines)]
fn evaluate_constant_ir(
    module: &Module,
    function: &crate::ir::Function,
) -> Option<Vec<ConstantValue>> {
    fn walk(
        function: &crate::ir::Function,
        module: &Module,
        block_id: crate::ir::BlockId,
        mut constants: HashMap<crate::ir::ValueId, ConstantValue>,
        mut bindings: HashMap<crate::semantic::SymbolId, crate::ir::ValueId>,
        mut output: Vec<ConstantValue>,
        steps: usize,
    ) -> Option<Vec<ConstantValue>> {
        if steps >= 10_000 {
            return None;
        }
        let block = function.blocks.iter().find(|block| block.id == block_id)?;
        for instruction in &block.instructions {
            match instruction {
                Instruction::Constant {
                    destination, value, ..
                } => {
                    let value = match value {
                        Constant::Integer(value) => ConstantValue::Integer(value.parse().ok()?),
                        Constant::Float(value) => ConstantValue::Float(value.parse().ok()?),
                        Constant::Boolean(value) => ConstantValue::Boolean(*value),
                        Constant::String(value) => ConstantValue::String(value.clone()),
                        Constant::Function(value) => ConstantValue::Function(value.clone()),
                        _ => return None,
                    };
                    constants.insert(*destination, value);
                }
                Instruction::Binary {
                    destination,
                    operator,
                    left,
                    right,
                    ..
                } => {
                    let left = constants.get(left)?.clone();
                    let right = constants.get(right)?.clone();
                    constants.insert(*destination, eval_binary(operator, left, right)?);
                }
                Instruction::Unary {
                    destination,
                    operator,
                    operand,
                    ..
                } => {
                    constants.insert(
                        *destination,
                        eval_unary(operator, constants.get(operand)?.clone())?,
                    );
                }
                Instruction::Copy {
                    destination,
                    source,
                    ..
                } => {
                    constants.insert(*destination, constants.get(source)?.clone());
                }
                Instruction::Store { symbol, value, .. } => {
                    bindings.insert(*symbol, *value);
                }
                Instruction::Load {
                    destination,
                    symbol,
                    ..
                } => {
                    let value = *bindings.get(symbol)?;
                    constants.insert(*destination, constants.get(&value)?.clone());
                }
                Instruction::Print { values, .. } => {
                    output.extend(
                        values
                            .iter()
                            .map(|value| constants.get(value).cloned())
                            .collect::<Option<Vec<_>>>()?,
                    );
                    output.push(ConstantValue::Newline);
                }
                Instruction::Call {
                    destination,
                    callee,
                    arguments,
                    ..
                } => {
                    let ConstantValue::Function(name) = constants.get(callee)?.clone() else {
                        return None;
                    };
                    let arguments = arguments
                        .iter()
                        .map(|value| constants.get(value).cloned())
                        .collect::<Option<Vec<_>>>()?;
                    constants.insert(
                        *destination,
                        constant_function_value(module, &name, &arguments, 0)?,
                    );
                }
                _ => return None,
            }
        }
        match block.terminator {
            Terminator::Return { value: None } => {
                if matches!(output.last(), Some(ConstantValue::Newline)) {
                    output.pop();
                }
                Some(output)
            }
            Terminator::Jump { target } => walk(
                function,
                module,
                target,
                constants,
                bindings,
                output,
                steps + 1,
            ),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let ConstantValue::Boolean(condition) = constants.get(&condition)?.clone() else {
                    return None;
                };
                walk(
                    function,
                    module,
                    if condition { then_block } else { else_block },
                    constants,
                    bindings,
                    output,
                    steps + 1,
                )
            }
            Terminator::Return { value: Some(_) } | Terminator::Stop { .. } => None,
        }
    }
    walk(
        function,
        module,
        function.entry,
        HashMap::new(),
        HashMap::new(),
        Vec::new(),
        0,
    )
}

#[allow(clippy::too_many_lines)]
fn constant_function_value(
    module: &Module,
    name: &str,
    arguments: &[ConstantValue],
    depth: usize,
) -> Option<ConstantValue> {
    if depth >= 128 {
        return None;
    }
    let function = module
        .functions
        .iter()
        .find(|function| function.name == name)?;
    if function.blocks.len() != 1 || function.parameters.len() != arguments.len() {
        return None;
    }
    let block = &function.blocks[0];
    let mut constants = HashMap::new();
    let mut bindings = HashMap::new();
    for (index, parameter) in function.parameters.iter().enumerate() {
        let value = crate::ir::ValueId(u32::MAX - u32::try_from(index).ok()?);
        constants.insert(value, arguments[index].clone());
        bindings.insert(*parameter, value);
    }
    for instruction in &block.instructions {
        match instruction {
            Instruction::Load {
                destination,
                symbol,
                ..
            } => {
                let value = *bindings.get(symbol)?;
                constants.insert(*destination, constants.get(&value)?.clone());
            }
            Instruction::Constant {
                destination, value, ..
            } => {
                constants.insert(
                    *destination,
                    match value {
                        Constant::Integer(value) => ConstantValue::Integer(value.parse().ok()?),
                        Constant::Float(value) => ConstantValue::Float(value.parse().ok()?),
                        Constant::Boolean(value) => ConstantValue::Boolean(*value),
                        Constant::String(value) => ConstantValue::String(value.clone()),
                        Constant::Function(value) => ConstantValue::Function(value.clone()),
                        _ => return None,
                    },
                );
            }
            Instruction::Binary {
                destination,
                operator,
                left,
                right,
                ..
            } => {
                constants.insert(
                    *destination,
                    eval_binary(
                        operator,
                        constants.get(left)?.clone(),
                        constants.get(right)?.clone(),
                    )?,
                );
            }
            Instruction::Unary {
                destination,
                operator,
                operand,
                ..
            } => {
                constants.insert(
                    *destination,
                    eval_unary(operator, constants.get(operand)?.clone())?,
                );
            }
            Instruction::Copy {
                destination,
                source,
                ..
            } => {
                constants.insert(*destination, constants.get(source)?.clone());
            }
            Instruction::Call {
                destination,
                callee,
                arguments,
                ..
            } => {
                let ConstantValue::Function(name) = constants.get(callee)?.clone() else {
                    return None;
                };
                let arguments = arguments
                    .iter()
                    .map(|value| constants.get(value).cloned())
                    .collect::<Option<Vec<_>>>()?;
                constants.insert(
                    *destination,
                    constant_function_value(module, &name, &arguments, depth + 1)?,
                );
            }
            Instruction::Store { symbol, value, .. } => {
                bindings.insert(*symbol, *value);
            }
            _ => return None,
        }
    }
    let Terminator::Return { value: Some(value) } = block.terminator else {
        return None;
    };
    constants.get(&value).cloned()
}

fn eval_unary(operator: &str, operand: ConstantValue) -> Option<ConstantValue> {
    match (operator, operand) {
        ("Minus", ConstantValue::Integer(value)) => {
            Some(ConstantValue::Integer(value.checked_neg()?))
        }
        ("Plus", ConstantValue::Integer(value)) => Some(ConstantValue::Integer(value)),
        ("Minus", ConstantValue::Float(value)) => Some(ConstantValue::Float(-value)),
        ("Plus", ConstantValue::Float(value)) => Some(ConstantValue::Float(value)),
        ("NOT", ConstantValue::Boolean(value)) => Some(ConstantValue::Boolean(!value)),
        _ => None,
    }
}

#[allow(clippy::cast_precision_loss)] // BN `/` converts INTEGER operands to FLOAT.
fn eval_binary(operator: &str, left: ConstantValue, right: ConstantValue) -> Option<ConstantValue> {
    match (left, right) {
        (ConstantValue::Integer(left), ConstantValue::Integer(right)) => match operator {
            "Plus" => Some(ConstantValue::Integer(left.checked_add(right)?)),
            "Minus" => Some(ConstantValue::Integer(left.checked_sub(right)?)),
            "Star" | "Multiply" => Some(ConstantValue::Integer(left.checked_mul(right)?)),
            "Slash" | "Divide" => Some(ConstantValue::Float(left as f64 / right as f64)),
            "DIV" if right != 0 => Some(ConstantValue::Integer(left.checked_div_euclid(right)?)),
            "Percent" if right != 0 => {
                Some(ConstantValue::Integer(left.checked_rem_euclid(right)?))
            }
            "Less" => Some(ConstantValue::Boolean(left < right)),
            "LessEqual" => Some(ConstantValue::Boolean(left <= right)),
            "Greater" => Some(ConstantValue::Boolean(left > right)),
            "GreaterEqual" => Some(ConstantValue::Boolean(left >= right)),
            "Equal" => Some(ConstantValue::Boolean(left == right)),
            "NotEqual" => Some(ConstantValue::Boolean(left != right)),
            _ => None,
        },
        (ConstantValue::Boolean(left), ConstantValue::Boolean(right)) => {
            Some(ConstantValue::Boolean(match operator {
                "AND" => left && right,
                "OR" => left || right,
                "XOR" => left ^ right,
                "Equal" => left == right,
                "NotEqual" => left != right,
                _ => return None,
            }))
        }
        (ConstantValue::Float(left), ConstantValue::Float(right)) => {
            Some(ConstantValue::Float(match operator {
                "Plus" => left + right,
                "Minus" => left - right,
                "Star" | "Multiply" => left * right,
                "Slash" | "Divide" => left / right,
                _ => return None,
            }))
        }
        _ => None,
    }
}

fn empty_module() -> String {
    "; Basic Next 0.2\ndefine i32 @main() {\nentry:\n  ret i32 0\n}\n".into()
}

fn print_integer_module(values: &[ConstantValue]) -> String {
    let calls = values
        .iter()
        .enumerate()
        .map(|(index, value)| match value {
            ConstantValue::Integer(value) => format!(
                "  %printed{index} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_int, i64 {value})"
            ),
            ConstantValue::Float(_) | ConstantValue::String(_) => format!(
                "  %printed{index} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr @.bn_str{index})"
            ),
            ConstantValue::Boolean(value) => format!(
                "  %printed{index} = call i32 (ptr, ...) @printf(ptr @.bn_fmt_str, ptr @.bn_{})",
                if *value { "true" } else { "false" }
            ),
            ConstantValue::Newline => format!("  %newline{index} = call i32 @putchar(i32 10)"),
            ConstantValue::Function(_) => unreachable!("function values are not printable"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let suffix = if matches!(values.last(), Some(ConstantValue::Newline)) {
        ""
    } else {
        "  %newline-final = call i32 @putchar(i32 10)\n"
    };
    let calls = if suffix.is_empty() {
        calls
    } else {
        format!("{calls}\n{suffix}")
    };
    let strings = values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| match value {
            ConstantValue::String(value) => Some(format!(
                "@.bn_str{index} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                value.len() + 1,
                escape_llvm(value)
            )),
            ConstantValue::Float(value) => {
                let value = render_float(*value);
                Some(format!(
                    "@.bn_str{index} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
                    value.len() + 1,
                    escape_llvm(&value)
                ))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "; Basic Next 0.2\n@.bn_fmt_int = private unnamed_addr constant [5 x i8] c\"%lld\\00\"\n@.bn_fmt_float = private unnamed_addr constant [3 x i8] c\"%g\\00\"\n@.bn_fmt_str = private unnamed_addr constant [3 x i8] c\"%s\\00\"\n@.bn_true = private unnamed_addr constant [5 x i8] c\"TRUE\\00\"\n@.bn_false = private unnamed_addr constant [6 x i8] c\"FALSE\\00\"\n{strings}\n\ndeclare i32 @printf(ptr, ...)\ndeclare i32 @putchar(i32)\n\ndefine i32 @main() {{\nentry:\n{calls}\n  ret i32 0\n}}\n"
    )
}

fn render_float(value: f64) -> String {
    if value.is_nan() {
        "NAN".into()
    } else if value == f64::INFINITY {
        "INF".into()
    } else if value == f64::NEG_INFINITY {
        "-INF".into()
    } else {
        let mut text = value.to_string();
        if !text.contains(['.', 'e', 'E']) {
            text.push_str(".0");
        }
        text
    }
}

fn escape_llvm(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b' '..=b'!' | b'#'..=b'[' | b']'..=b'~' => (byte as char).to_string(),
            _ => format!("\\{byte:02X}"),
        })
        .collect()
}

fn instruction_name(instruction: &Instruction) -> &'static str {
    match instruction {
        Instruction::Constant { .. } => "constants",
        Instruction::Default { .. } => "defaults",
        Instruction::Load { .. } => "loads",
        Instruction::Store { .. } => "stores",
        Instruction::Copy { .. } => "copies",
        Instruction::Unary { .. } => "unary operations",
        Instruction::Binary { .. } => "binary operations",
        Instruction::Cast { .. } => "casts",
        Instruction::Call { .. } => "calls",
        Instruction::Input { .. } => "INPUT",
        Instruction::Vector { .. } => "vectors",
        Instruction::Index { .. } => "indexing",
        Instruction::Member { .. } => "member access",
        Instruction::SetIndex { .. } => "indexed stores",
        Instruction::Length { .. } => "LEN",
        Instruction::SizeOf { .. } => "SIZEOF",
        Instruction::Print { .. } => "PRINT",
        Instruction::ClearScreen { .. } | Instruction::Beep { .. } => "console operations",
        Instruction::Allocate { .. } => "allocation",
        Instruction::Delete { .. } => "deletion",
        Instruction::SetMember { .. } => "member stores",
        Instruction::SetField { .. } => "field stores",
        Instruction::EnsureClass { .. } => "class initialization",
        Instruction::LoadStatic { .. } => "static loads",
        Instruction::StoreStatic { .. } => "static stores",
    }
}
