// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Lowering of scalar instructions (constants, stores, copies, unary and binary
// operators, casts) with constant propagation through the block state.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::typed_llvm;
#[path = "emission_tail.rs"]
mod emission_tail;
use emission_tail::lower_scalar_instruction_tail;

pub(crate) fn lower_scalar_instruction(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    if arc_ops::lower_ownership_instruction(
        text,
        module,
        function,
        block_id,
        instruction,
        analysis,
        symbols,
        block_state,
        state,
    ) {
        return Ok(());
    }
    match instruction {
        Instruction::Constant {
            destination,
            value,
            ty,
            ..
        } => match value {
            Constant::Integer(value) => {
                let parsed = parse_integer(value).expect("validated integer constant");
                block_state.constants.insert(
                    *destination,
                    ConstantValue::Integer(parsed, integer_kind(ty)),
                );
                emit_constant_assignment(text, *destination, ty, value);
            }
            Constant::Float(value) => {
                let parsed = parse_float_constant(value).expect("validated float constant");
                block_state.constants.insert(
                    *destination,
                    typed_constant(ConstantValue::Float(parsed), ty),
                );
                emit_constant_assignment(text, *destination, ty, &render_float(parsed, ty));
            }
            Constant::Boolean(value) => {
                block_state
                    .constants
                    .insert(*destination, ConstantValue::Boolean(*value));
                define_boolean(text, analysis, *destination, *value);
            }
            Constant::String(value) => {
                block_state
                    .constants
                    .insert(*destination, ConstantValue::String(value.clone()));
                let global = O::raw(string_global(&function.name, destination.0));
                text.assign(
                    format!("v{}", destination.0),
                    I::gep(T::I8, global, vec![(T::I64, O::int(0))]),
                );
            }
            Constant::Function(name) => {
                if module
                    .functions
                    .iter()
                    .any(|function| function.name == *name)
                {
                    let symbol = llvm_function_symbol(name);
                    let select = I::select(O::bool(true), T::Ptr, O::global(symbol), O::null());
                    text.assign(format!("v{}", destination.0), select);
                }
            }
            Constant::HostArgs | Constant::Type(_) | Constant::HostConsole => {}
            Constant::NotAvailable => {
                let (dest, na) = (destination.0, T::struct_of([T::I1, T::Double]));
                text.assign(
                    format!("na{dest}"),
                    I::insert(na.clone(), O::undef(), T::I1, O::bool(true), 0),
                );
                text.assign(
                    format!("v{dest}"),
                    I::insert(na, O::reg(format!("na{dest}")), T::Double, O::float(0.0), 1),
                );
            }
            Constant::Null => {
                text.assign(
                    format!("v{}", destination.0),
                    I::cast(CastOp::IntToPtr, T::I64, O::int(0), T::Ptr),
                );
            }
            Constant::EndOfFile => {
                unreachable!("EndOfFile must be rejected during target validation");
            }
        },
        Instruction::Phi {
            destination,
            incoming,
            ty,
            ..
        } => {
            let llvm_ty = llvm_type(ty).expect("validated Phi type");
            state
                .control_flow
                .defer_phi(text.len(), *destination, llvm_ty, incoming);
        }
        Instruction::Default {
            destination,
            ty,
            dimensions,
            ..
        } if !dimensions.is_empty() => {
            let Type::Vector { element, .. } = ty else {
                unreachable!("validated multidimensional default type");
            };
            let element_llvm = llvm_type(element).expect("validated vector element type");
            let element_ty = typed_llvm(element_llvm);
            let len = dimensions[0];
            let dest = destination.0;
            let array = T::Array(len, Box::new(element_ty.clone()));
            let base = O::reg(format!("vecdefault{dest}"));
            let at = |index: usize| {
                let index = O::int(i64::try_from(index).expect("vector length fits i64"));
                vec![(T::I32, O::int(0)), (T::I32, index)]
            };
            text.assign(format!("vecdefault{dest}"), I::alloca(array.clone()));
            for index in 0..len {
                let slot = format!("vecdefaultslot{dest}_{index}");
                text.assign(slot.clone(), I::gep(array.clone(), base.clone(), at(index)));
                let zero = match element_llvm {
                    "i1" | "i8" | "i16" | "i32" | "i64" => O::int(0),
                    "float" | "double" => O::float(0.0),
                    "ptr" => O::null(),
                    "{ i1, ptr, i64 }" | "{ i1, ptr }" => O::zero_initializer(),
                    _ => unreachable!("validated vector element type"),
                };
                text.emit(I::store(element_ty.clone(), zero, O::reg(slot)));
            }
            text.assign(format!("vecdefaultptr{dest}"), I::gep(array, base, at(0)));
            let fat = T::struct_of([T::Ptr, T::I32]);
            let pointer = O::reg(format!("vecdefaultptr{dest}"));
            text.assign(
                format!("vecdefaultfat{dest}"),
                I::insert(fat.clone(), O::undef(), T::Ptr, pointer, 0),
            );
            let length = O::int(i64::try_from(len).expect("vector length fits i64"));
            text.assign(
                format!("v{dest}"),
                I::insert(
                    fat,
                    O::reg(format!("vecdefaultfat{dest}")),
                    T::I32,
                    length,
                    1,
                ),
            );
        }
        Instruction::Default {
            destination, ty, ..
        } => match llvm_type(ty).expect("validated default type") {
            GENERAL_LAYOUT => general_alternative::emit_default(
                text,
                *destination,
                general_alternative(ty).expect("general alternative default"),
            ),
            "i1" => define_boolean(text, analysis, *destination, false),
            layout @ ("i8" | "i16" | "i32" | "i64") => {
                let add = I::binary(BinaryOp::Add, typed_llvm(layout), O::int(0), O::int(0));
                text.assign(format!("v{}", destination.0), add);
            }
            layout @ ("float" | "double") => {
                let fadd = I::binary(
                    BinaryOp::FAdd,
                    typed_llvm(layout),
                    O::float(0.0),
                    O::float(0.0),
                );
                text.assign(format!("v{}", destination.0), fadd);
            }
            "ptr" => {
                let value = if function.kind == FunctionKind::Default {
                    let owner = function
                        .owner
                        .as_deref()
                        .expect("validated default constructor owner");
                    let bytes = i64::try_from(class_instance_bytes(module, owner))
                        .expect("object size fits i64");
                    I::call(
                        T::Ptr,
                        "calloc",
                        vec![(T::I64, O::int(1)), (T::I64, O::int(bytes))],
                    )
                } else {
                    I::gep(T::I8, O::global(".bn_empty"), vec![(T::I64, O::int(0))])
                };
                text.assign(format!("v{}", destination.0), value);
            }
            "{ i1, double }" => emit_optional_float_default(text, *destination),
            // An aggregate default: every field zero or null, built field by
            // field as `RETURN` builds it (`%<prefix>0_` … then `%v`).
            layout @ ("{ ptr, i32 }" | "{ i1, ptr, i64 }" | "{ i1, ptr }" | "{ i1, ptr, i32 }") => {
                let dest = destination.0;
                let prefix = match layout {
                    "{ ptr, i32 }" => "vec",
                    "{ i1, ptr, i64 }" => "errdef",
                    "{ i1, ptr }" => "ptrdef",
                    _ => "epdef",
                };
                let aggregate = typed_llvm(layout);
                let T::Struct(fields) = aggregate.clone() else {
                    unreachable!("aggregate layout");
                };
                let mut previous = O::undef();
                for (index, field) in fields.iter().enumerate() {
                    let zero = match field {
                        T::I1 => O::bool(false),
                        T::Ptr => O::null(),
                        _ => O::int(0),
                    };
                    let name = if index + 1 == fields.len() {
                        format!("v{dest}")
                    } else if layout == "{ ptr, i32 }" {
                        format!("{prefix}{dest}")
                    } else {
                        format!("{prefix}{index}_{dest}")
                    };
                    text.assign(
                        name.clone(),
                        I::insert(aggregate.clone(), previous, field.clone(), zero, index),
                    );
                    previous = O::reg(name);
                }
            }
            _ => unreachable!("validated scalar default type"),
        },
        Instruction::Store {
            symbol,
            value,
            previous,
            ..
        } => {
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated stored value type");
            let slot_ty = analysis.symbols.get(symbol).unwrap_or(value_ty);
            let value_llvm = llvm_type(value_ty).expect("validated store LLVM type");
            let slot_llvm = llvm_type(slot_ty).expect("validated slot LLVM type");
            let operand = if is_struct_type(module, slot_ty) {
                let Type::Named(owner) = slot_ty else {
                    unreachable!("validated struct type");
                };
                let bytes = class_instance_bytes(module, owner);
                let tag = value.0;
                let copy = O::reg(format!("structcopy{tag}"));
                text.assign(
                    format!("structcopy{tag}"),
                    I::alloca(T::Array(
                        usize::try_from(bytes).expect("struct size fits usize"),
                        Box::new(T::I8),
                    )),
                );
                let bytes = O::int(i64::try_from(bytes).expect("struct size fits i64"));
                let args = vec![
                    (T::Ptr, copy),
                    (T::Ptr, O::reg(format!("v{tag}"))),
                    (T::I64, bytes),
                    (T::I1, O::bool(false)),
                ];
                text.emit(I::call(T::Void, "llvm.memcpy.p0.p0.i64", args));
                format!("%structcopy{tag}")
            } else if slot_llvm == "{ i1, double }" && matches!(value_llvm, "float" | "double") {
                let optional_value =
                    coerce_to_type(text, *value, value_ty, &Type::Float(FloatType::Float64));
                let optional = T::struct_of([T::I1, T::Double]);
                let tag = O::reg(format!("optstoretag{}", value.0));
                text.assign(
                    format!("optstoretag{}", value.0),
                    I::insert(optional.clone(), O::undef(), T::I1, O::bool(false), 0),
                );
                text.assign(
                    format!("optstore{}", value.0),
                    I::insert(optional, tag, T::Double, O::raw(optional_value), 1),
                );
                format!("%optstore{}", value.0)
            } else if slot_llvm == "i1" {
                i1_operand(text, analysis, state, *value)
            } else if value_llvm != slot_llvm
                && (matches!(value_llvm, "i8" | "i16" | "i32" | "i64")
                    && matches!(slot_llvm, "i8" | "i16" | "i32" | "i64")
                    || matches!(
                        (value_llvm, slot_llvm),
                        ("float", "double") | ("double", "float")
                    )
                    || matches!(slot_llvm, "{ i1, ptr, i64 }" | GENERAL_LAYOUT)
                        && value_llvm != slot_llvm)
            {
                coerce_to_type(text, *value, value_ty, slot_ty)
            } else {
                format!("%v{}", value.0)
            };
            if analysis.input_symbols.contains(symbol)
                && !analysis.input_targets.contains_key(value)
            {
                let slot = symbols[symbol];
                let tag = value.0;
                let owned = O::reg(format!("inputreplaceowned{tag}"));
                let old = O::reg(format!("inputreplaceold{tag}"));
                text.assign(
                    format!("inputreplaceowned{tag}"),
                    I::load(T::I1, O::reg(format!("inputowned{slot}"))),
                );
                text.assign(
                    format!("inputreplaceold{tag}"),
                    I::load(T::Ptr, O::reg(format!("s{slot}"))),
                );
                text.assign(
                    format!("inputreplacefree{tag}"),
                    I::select(owned, T::Ptr, old, O::null()),
                );
                text.emit(I::call(
                    T::Void,
                    "free",
                    vec![(T::Ptr, O::reg(format!("inputreplacefree{tag}")))],
                ));
            }
            let slot = symbols[symbol];
            if previous.is_some() && arc_ops::holds_references(module, slot_ty) {
                let (storage, kept, binding) = (
                    format!("%vstore{slot}"),
                    format!("%vprev{slot}"),
                    format!("%s{slot}"),
                );
                vectors::emit_vector_keep_previous(text, (&storage, &kept, &binding), slot_ty);
            }
            // Explicit ownership: the write gives back the previous content
            // (the IR releases it) and stores a value it owns.
            if let Some(previous) = previous {
                crate::ir::InstSink::assign(
                    text,
                    format!("v{}", previous.0),
                    crate::ir::LlvmInst::load(
                        crate::layout::typed_llvm(slot_llvm),
                        crate::ir::LlvmOperand::raw(format!("%s{slot}")),
                    ),
                );
            }
            let operand = if function.weak_symbols.contains(symbol) {
                arc_ops::weak_store_operand(text, &operand, state)
            } else {
                vectors::emit_vector_copy(text, &format!("%vstore{slot}"), &operand, slot_ty)
                    .unwrap_or(operand)
            };
            text.emit(I::store(
                typed_llvm(slot_llvm),
                O::raw(operand),
                O::reg(format!("s{slot}")),
            ));
            if analysis.released_symbols.contains(symbol) {
                text.emit(I::store(T::I1, O::bool(true), O::raw(live_flag(*symbol))));
            }
            if analysis.input_symbols.contains(symbol) {
                let slot = symbols[symbol];
                // A slot whose line escapes never owns it (`escaping_inputs`).
                if analysis.input_targets.contains_key(value)
                    && !analysis.escaping_inputs.contains(symbol)
                {
                    let line = O::reg(format!("v{}", value.0));
                    text.assign(
                        format!("inputisvalue{}", value.0),
                        I::icmp(ICmpCond::Ne, T::Ptr, line, O::global(".bn_eof")),
                    );
                    text.emit(I::store(
                        T::I1,
                        O::reg(format!("inputisvalue{}", value.0)),
                        O::reg(format!("inputowned{slot}")),
                    ));
                } else {
                    text.emit(I::store(
                        T::I1,
                        O::bool(false),
                        O::reg(format!("inputowned{slot}")),
                    ));
                }
            }
            if let Some(value) = block_state.constants.get(value).cloned() {
                block_state.bindings.insert(*symbol, value);
            } else {
                block_state.bindings.remove(symbol);
            }
        }
        Instruction::Load {
            destination,
            symbol,
            ty,
            ..
        } => lower_load(
            text,
            block_id,
            function,
            analysis,
            symbols,
            block_state,
            *destination,
            *symbol,
            ty,
            true,
            state,
        ),
        Instruction::Copy {
            destination,
            source,
            ty,
            ..
        } => {
            if let Some(value) = block_state.constants.get(source).cloned() {
                let value = typed_constant(value, ty);
                block_state.constants.insert(*destination, value.clone());
                emit_constant_value_analyzed(text, analysis, *destination, ty, &value);
            } else {
                block_state.constants.remove(destination);
                match llvm_type(ty).expect("validated copy type") {
                    "i1" => define_boolean_from(text, analysis, state, *destination, *source),
                    layout @ ("i8" | "i16" | "i32" | "i64") => {
                        let copy = I::binary(
                            BinaryOp::Add,
                            typed_llvm(layout),
                            O::int(0),
                            O::reg(format!("v{}", source.0)),
                        );
                        text.assign(format!("v{}", destination.0), copy);
                    }
                    layout @ ("float" | "double") => {
                        let copy = I::binary(
                            BinaryOp::FAdd,
                            typed_llvm(layout),
                            O::float(0.0),
                            O::reg(format!("v{}", source.0)),
                        );
                        text.assign(format!("v{}", destination.0), copy);
                    }
                    "ptr" => {
                        let copy = I::gep(
                            T::I8,
                            O::reg(format!("v{}", source.0)),
                            vec![(T::I64, O::int(0))],
                        );
                        text.assign(format!("v{}", destination.0), copy);
                    }
                    "{ i1, ptr, i64 }" => {
                        // An `Error` record is wrapped again so the copy owns it.
                        let dest = destination.0;
                        let union = T::struct_of([T::I1, T::Ptr, T::I64]);
                        let source = O::reg(format!("v{}", source.0));
                        let field = |index: usize| O::reg(format!("netc{index}{dest}"));
                        for index in 0..3 {
                            text.assign(
                                format!("netc{index}{dest}"),
                                I::extract(union.clone(), source.clone(), index),
                            );
                        }
                        text.assign(
                            format!("netca{dest}"),
                            I::insert(union.clone(), O::undef(), T::I1, field(0), 0),
                        );
                        let wrap = vec![(T::I1, field(0)), (T::Ptr, field(1)), (T::Ptr, O::null())];
                        text.assign(
                            format!("netcbwrap{dest}"),
                            I::call(T::Ptr, "bn_rt_error_wrap", wrap),
                        );
                        let wrapped = O::reg(format!("netcbwrap{dest}"));
                        text.assign(
                            format!("netcb{dest}"),
                            I::insert(
                                union.clone(),
                                O::reg(format!("netca{dest}")),
                                T::Ptr,
                                wrapped,
                                1,
                            ),
                        );
                        text.assign(
                            format!("v{dest}"),
                            I::insert(union, O::reg(format!("netcb{dest}")), T::I64, field(2), 2),
                        );
                    }
                    _ => unreachable!("validated copy type"),
                }
            }
        }
        Instruction::Unary {
            destination,
            operator,
            operand,
            ty,
            ..
        } => {
            if let Some(result) = fold_unary(operator, block_state.constants.get(operand), ty) {
                block_state.constants.insert(*destination, result.clone());
                emit_constant_value(text, *destination, ty, &result);
                return Ok(());
            }
            block_state.constants.remove(destination);
            match (
                operator.as_str(),
                llvm_type(ty).expect("validated unary LLVM type"),
            ) {
                ("Plus", "i8" | "i16" | "i32" | "i64") => {
                    let operand_ty = analysis.values.get(operand).unwrap_or(ty);
                    let operand_op = coerce_to_type(text, *operand, operand_ty, ty);
                    let layout = typed_llvm(llvm_type(ty).expect("validated integer unary type"));
                    let plus = I::binary(BinaryOp::Add, layout, O::int(0), O::raw(operand_op));
                    text.assign(format!("v{}", destination.0), plus);
                }
                ("Minus", "i8" | "i16" | "i32" | "i64") => {
                    let operand_ty = analysis.values.get(operand).unwrap_or(ty);
                    emit_checked_integer_op(
                        text,
                        block_id,
                        *destination,
                        "Minus",
                        *operand,
                        None,
                        operand_ty,
                        ty,
                        ty,
                        state,
                    );
                }
                (sign @ ("Plus" | "Minus"), layout @ ("float" | "double")) => {
                    let op = if sign == "Plus" {
                        BinaryOp::FAdd
                    } else {
                        BinaryOp::FSub
                    };
                    let value = I::binary(
                        op,
                        typed_llvm(layout),
                        O::float(0.0),
                        O::reg(format!("v{}", operand.0)),
                    );
                    text.assign(format!("v{}", destination.0), value);
                }
                ("NOT", "i1") => {
                    let not = I::binary(
                        BinaryOp::Xor,
                        T::I1,
                        O::int(1),
                        O::reg(format!("v{}", operand.0)),
                    );
                    text.assign(format!("v{}", destination.0), not);
                }
                ("NOT", "i8" | "i16" | "i32" | "i64") => {
                    emit_integer_not(text, block_id, *destination, *operand, ty, state);
                }
                _ => unreachable!("validated unary operator"),
            }
        }
        Instruction::Binary {
            destination,
            operator,
            left,
            right,
            ty,
            ..
        } => {
            if let Some(result) = fold_binary(
                operator,
                block_state.constants.get(left),
                block_state.constants.get(right),
                ty,
            ) {
                block_state.constants.insert(*destination, result.clone());
                emit_constant_value(text, *destination, ty, &result);
                return Ok(());
            }
            block_state.constants.remove(destination);
            let left_ty = analysis.values.get(left).expect("validated left type");
            let right_ty = analysis.values.get(right).expect("validated right type");
            if operator == "IS" {
                if !emit_is(text, *destination, *left, left_ty, right_ty) {
                    return Err(unsupported_instruction(
                        module,
                        function,
                        instruction,
                        "IS on this alternative",
                    ));
                }
            } else if matches!(operator.as_str(), "Assign" | "NotEqual")
                && let Some((alternative, members, other, other_ty, left_is_alt)) =
                    general_alternative::comparable_alternative(left_ty)
                        .map(|members| (*left, members, *right, right_ty, true))
                        .or_else(|| {
                            general_alternative::comparable_alternative(right_ty)
                                .map(|members| (*right, members, *left, left_ty, false))
                        })
            {
                let (lhs, lhs_ty, rhs, rhs_ty) = if left_is_alt {
                    (alternative, left_ty, other, other_ty)
                } else {
                    (other, other_ty, alternative, right_ty)
                };
                general_alternative::emit_equals(
                    text,
                    general_alternative::EqualsOperands {
                        destination: *destination,
                        left: lhs,
                        left_ty: lhs_ty,
                        members,
                        right: rhs,
                        right_ty: rhs_ty,
                        not_equal: operator == "NotEqual",
                    },
                );
            } else {
                emit_runtime_binary(
                    text,
                    block_id,
                    *destination,
                    operator,
                    *left,
                    *right,
                    left_ty,
                    right_ty,
                    ty,
                    state,
                );
            }
        }
        Instruction::Cast {
            destination,
            value,
            ty,
            ..
        } => {
            if let Some(result) = fold_cast(block_state.constants.get(value), ty) {
                block_state.constants.insert(*destination, result.clone());
                emit_constant_value(text, *destination, ty, &result);
                return Ok(());
            }
            block_state.constants.remove(destination);
            let source_ty = analysis
                .values
                .get(value)
                .expect("validated cast source type");
            if llvm_type(source_ty) == Some("{ i1, double }")
                && matches!(ty, Type::Float(_) | Type::FloatLiteral)
            {
                extract_optional_float(text, *destination, *value);
            } else {
                lower_cast(text, block_id, *destination, *value, source_ty, ty, state);
            }
        }
        _ => {
            return lower_scalar_instruction_tail(
                text,
                module,
                function,
                block_id,
                instruction,
                analysis,
                symbols,
                block_state,
                state,
            );
        }
    }
    Ok(())
}
