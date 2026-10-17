// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Lowering of scalar instructions (constants, stores, copies, unary and binary
// operators, casts) with constant propagation through the block state.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
#[path = "emission_tail.rs"]
mod emission_tail;
use emission_tail::lower_scalar_instruction_tail;

pub(crate) fn lower_scalar_instruction(
    text: &mut crate::ir::LlvmModule,
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
                let constant = ConstantValue::Integer(parsed, integer_kind(ty));
                emit_constant_assignment(text, *destination, ty, &constant);
                block_state.constants.insert(*destination, constant);
            }
            Constant::Float(value) => {
                let parsed = parse_float_constant(value).expect("validated float constant");
                block_state.constants.insert(
                    *destination,
                    typed_constant(ConstantValue::Float(parsed), ty),
                );
                emit_constant_assignment(text, *destination, ty, &ConstantValue::Float(parsed));
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
                let global = string_global(&function.name, destination.0);
                text.assign(
                    format!("v{}", destination.0),
                    crate::layout::string_payload_address(&global),
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
                if let Type::Alternative(members) = ty
                    && crate::layout::AlternativeLayout::of(members)
                        == Some(crate::layout::AlternativeLayout::Region)
                {
                    let _ = alternatives::null_region(text, &format!("v{}", destination.0));
                } else {
                    text.assign(
                        format!("v{}", destination.0),
                        I::cast(CastOp::IntToPtr, T::I64, O::int(0), T::Ptr),
                    );
                }
            }
            // `EOF` is the `@.bn_eof` sentinel, the value `INPUT` reads at end
            // of input (T.K9).
            Constant::EndOfFile => {
                text.assign(
                    format!("v{}", destination.0),
                    crate::layout::string_payload_address(".bn_eof"),
                );
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
                .defer_phi(text.position(), *destination, llvm_ty, incoming);
        }
        Instruction::Default {
            destination, ty, ..
        } if vectors::nest(ty).is_some() => {
            let strings = matches!(ty, Type::Vector { element, .. } if **element == Type::String);
            let Some(nest) = vectors::nest(ty) else {
                unreachable!()
            };
            nest.emit_default(text, destination.0, strings);
        }
        Instruction::Default {
            destination,
            ty,
            dimensions,
            dynamic_dimensions,
            ..
        } if dimensions.is_empty() && dynamic_dimensions.len() == 1 => {
            let Type::Vector { element, .. } = ty else {
                unreachable!("validated dynamic default type");
            };
            let element_llvm = llvm_type(element).expect("validated vector element type");
            let element_ty = element_llvm.clone();
            let dest = destination.0;
            let dim_id = dynamic_dimensions[0];
            let dim_ty = analysis
                .values
                .get(&dim_id)
                .expect("validated dynamic dimension type");
            let dim_llvm = llvm_type(dim_ty).expect("validated dynamic dimension llvm type");
            let dim_op = O::reg(format!("v{}", dim_id.0));

            // Check if dimension < 0 -> trap INVALID_VECTOR_DIMENSION
            let is_neg = I::icmp(ICmpCond::Slt, dim_llvm.clone(), dim_op.clone(), O::int(0));
            text.assign(format!("dynvecneg{dest}"), is_neg);
            let ok_label = format!("dynvecok{dest}");
            let neg_op = O::reg(format!("dynvecneg{dest}"));
            traps::emit_trap(
                text,
                block_id,
                state,
                &neg_op,
                ok_label,
                bn_diag::DiagId::INVALID_VECTOR_DIMENSION,
                vec![("message", Fact::Text("vector dimension must be a non-negative size".into()))],
            );

            // Coerce/cast to i32 for fat pointer and i64 for byte size
            let len_i32 = if dim_llvm == T::I32 {
                dim_op.clone()
            } else if dim_llvm == T::I64 {
                let tr = I::cast(CastOp::Trunc, T::I64, dim_op.clone(), T::I32);
                text.assign(format!("dynveclen32_{dest}"), tr);
                O::reg(format!("dynveclen32_{dest}"))
            } else {
                let ext = I::cast(CastOp::ZExt, dim_llvm.clone(), dim_op.clone(), T::I32);
                text.assign(format!("dynveclen32_{dest}"), ext);
                O::reg(format!("dynveclen32_{dest}"))
            };

            let len_i64 = if dim_llvm == T::I64 {
                dim_op
            } else {
                let ext = I::cast(CastOp::ZExt, T::I32, len_i32.clone(), T::I64);
                text.assign(format!("dynveclen64_{dest}"), ext);
                O::reg(format!("dynveclen64_{dest}"))
            };

            let elem_bytes: u64 = match element_ty {
                T::I1 | T::I8 => 1,
                T::I16 => 2,
                T::I32 | T::Float => 4,
                T::I64 | T::Double | T::Ptr => 8,
                _ => 8,
            };

            // Allocate buffer on stack with dynamic count
            text.assign(
                format!("dynvecbuf{dest}"),
                I::alloca_counted(element_ty, T::I64, len_i64.clone()),
            );

            // Compute byte size = len_i64 * elem_bytes
            let byte_count = I::binary(BinaryOp::Mul, T::I64, len_i64, O::uint(elem_bytes));
            text.assign(format!("dynvecbytes{dest}"), byte_count);

            // Zero-initialize buffer via llvm.memset.p0.i64
            let memset_args = vec![
                (T::Ptr, O::reg(format!("dynvecbuf{dest}"))),
                (T::I8, O::int(0)),
                (T::I64, O::reg(format!("dynvecbytes{dest}"))),
                (T::I1, O::bool(false)),
            ];
            text.emit(I::call(T::Void, "llvm.memset.p0.i64", memset_args));

            // Construct fat vector { ptr, i32 }
            let fat = T::struct_of([T::Ptr, T::I32]);
            let pointer = O::reg(format!("dynvecbuf{dest}"));
            text.assign(
                format!("dynvecfat0_{dest}"),
                I::insert(fat.clone(), O::undef(), T::Ptr, pointer, 0),
            );
            text.assign(
                format!("v{dest}"),
                I::insert(
                    fat,
                    O::reg(format!("dynvecfat0_{dest}")),
                    T::I32,
                    len_i32,
                    1,
                ),
            );
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
            let element_ty = element_llvm.clone();
            let len = dimensions[0];
            let dest = destination.0;
            let array = T::Array(len, Box::new(element_ty.clone()));
            let base = O::reg(format!("vecdefault{dest}"));
            let at = |index: usize| {
                let index = O::int(i64::try_from(index).expect("vector length fits i64"));
                vec![(T::I32, O::int(0)), (T::I32, index)]
            };
            text.assign(format!("vecdefault{dest}"), I::alloca(array.clone()));
            if let Some(owner) = struct_owner(module, element) {
                let bytes = class_instance_bytes(module, &owner);
                let total_bytes = (len as u64).saturating_mul(bytes);
                let struct_bytes = T::Array(
                    usize::try_from(total_bytes).expect("fits usize"),
                    Box::new(T::I8),
                );
                let struct_data = format!("vecdefstructdata{dest}");
                text.assign(struct_data.clone(), I::alloca(struct_bytes.clone()));
                text.emit(I::store(
                    struct_bytes,
                    O::zero_initializer(),
                    O::reg(struct_data.clone()),
                ));
                for index in 0..len {
                    let elem_ptr = format!("vecdefelemptr{dest}_{index}");
                    let offset = (index as u64).saturating_mul(bytes);
                    text.assign(
                        elem_ptr.clone(),
                        I::gep(
                            T::I8,
                            O::reg(struct_data.clone()),
                            vec![(T::I64, O::uint(offset))],
                        ),
                    );
                    let slot = format!("vecdefaultslot{dest}_{index}");
                    text.assign(slot.clone(), I::gep(array.clone(), base.clone(), at(index)));
                    text.emit(I::store(T::Ptr, O::reg(elem_ptr), O::reg(slot)));
                }
            } else {
                for index in 0..len {
                    let slot = format!("vecdefaultslot{dest}_{index}");
                    text.assign(slot.clone(), I::gep(array.clone(), base.clone(), at(index)));
                    let zero = match element_llvm.clone() {
                        T::I1 | T::I8 | T::I16 | T::I32 | T::I64 => O::int(0),
                        T::Float | T::Double => O::float(0.0),
                        // A STRING element defaults to "", as a scalar STRING does.
                        T::Ptr if **element == Type::String => {
                            crate::layout::string_payload(".bn_empty")
                        }
                        T::Ptr => O::null(),
                        other
                            if crate::layout::is_one_of(
                                &other,
                                &[
                                    crate::layout::handle_result_ty(),
                                    crate::layout::status_pointer_ty(),
                                ],
                            ) =>
                        {
                            O::zero_initializer()
                        }
                        _ => unreachable!("validated vector element type"),
                    };
                    text.emit(I::store(element_ty.clone(), zero, O::reg(slot)));
                }
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
            layout if layout == crate::general_alternative::layout() => {
                general_alternative::emit_default(
                    text,
                    *destination,
                    general_alternative(ty).expect("general alternative default"),
                );
            }
            T::I1 => define_boolean(text, analysis, *destination, false),
            layout @ (T::I8 | T::I16 | T::I32 | T::I64) => {
                let add = I::binary(BinaryOp::Add, layout, O::int(0), O::int(0));
                text.assign(format!("v{}", destination.0), add);
            }
            layout @ (T::Float | T::Double) => {
                let fadd = I::binary(BinaryOp::FAdd, layout, O::float(0.0), O::float(0.0));
                text.assign(format!("v{}", destination.0), fadd);
            }
            T::Ptr => {
                // A STRING default is the immortal empty literal, including
                // inside a STRUCT `$default`. The calloc below is the STRUCT
                // (or class) value itself; applying it to a STRING field
                // yields a zeroed header, and releasing that field traps.
                let value = if *ty == Type::String {
                    crate::layout::string_payload_address(".bn_empty")
                } else if function.kind == FunctionKind::Default {
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
                    crate::layout::string_payload_address(".bn_empty")
                };
                text.assign(format!("v{}", destination.0), value);
            }
            layout if layout == crate::layout::optional_float_ty() => {
                emit_optional_float_default(text, *destination);
            }
            // An aggregate default: every field zero or null, built field by
            // field as `RETURN` builds it (`%<prefix>0_` … then `%v`).
            layout
                if crate::layout::is_one_of(
                    &layout,
                    &[
                        crate::layout::vector_ty(),
                        crate::layout::region_ty(),
                        crate::layout::handle_result_ty(),
                        crate::layout::status_pointer_ty(),
                        crate::layout::status_endpoint_ty(),
                    ],
                ) =>
            {
                let dest = destination.0;
                let prefix = if crate::layout::is_one_of(
                    &layout,
                    &[crate::layout::vector_ty(), crate::layout::region_ty()],
                ) {
                    "vec"
                } else if layout == crate::layout::handle_result_ty() {
                    "errdef"
                } else if layout == crate::layout::status_pointer_ty() {
                    "ptrdef"
                } else {
                    "epdef"
                };
                let aggregate = layout.clone();
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
                    } else if layout == crate::layout::vector_ty() {
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
            let operand = if let Some(owner) = struct_owner(module, slot_ty) {
                // The slot owns fixed storage reserved at function entry
                // (`functions.rs`); a store copies the value into it, so a loop
                // never grows the stack.
                let storage = format!("structslot{}", symbols[symbol]);
                let copy_witness = arc_runtime::struct_copy_witness_name(&owner);
                let destroy_witness = arc_runtime::struct_destroy_witness_name(&owner);
                text.emit(I::call(
                    T::Void,
                    &destroy_witness,
                    vec![(T::Ptr, O::reg(&storage))],
                ));
                text.emit(I::call(
                    T::Void,
                    &copy_witness,
                    vec![
                        (T::Ptr, O::reg(&storage)),
                        (T::Ptr, O::reg(format!("v{}", value.0))),
                    ],
                ));
                O::reg(&storage)
            } else if slot_llvm == crate::layout::optional_float_ty()
                && matches!(value_llvm, T::Float | T::Double)
            {
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
                    I::insert(optional, tag, T::Double, optional_value, 1),
                );
                O::reg(format!("optstore{}", value.0))
            } else if slot_llvm == T::I1 {
                i1_operand(text, analysis, state, *value)
            } else if value_llvm == crate::layout::region_ty()
                && slot_llvm == crate::layout::vector_ty()
            {
                casts::emit_pointer_void_restore(
                    text,
                    block_id,
                    &format!("store{}", value.0),
                    O::reg(format!("v{}", value.0)),
                    slot_ty,
                    state,
                )
            } else if value_llvm != slot_llvm
                && (value_llvm.is_integer() && slot_llvm.is_integer()
                    || value_llvm.is_floating() && slot_llvm.is_floating()
                    || crate::layout::is_one_of(&slot_llvm, &[crate::layout::handle_result_ty(), crate::general_alternative::layout()])
                    // An `Error` into `Endpoint OR Error` / `Addresses OR Error` (K36).
                    || (crate::layout::is_one_of(&slot_llvm, &[crate::layout::status_endpoint_ty(), crate::layout::status_pointer_ty()])
                        && is_error_type(value_ty))
                    || value_llvm == crate::layout::vector_ty() && slot_llvm == crate::layout::region_ty()
                    || matches!(slot_ty, Type::Alternative(members)
                        if crate::layout::AlternativeLayout::of(members)
                            == Some(crate::layout::AlternativeLayout::Region)
                            && value_llvm != slot_llvm))
            {
                coerce_to_type(text, *value, value_ty, slot_ty)
            } else {
                O::reg(format!("v{}", value.0))
            };
            let slot = symbols[symbol];
            if previous.is_some() && arc_ops::holds_references(module, slot_ty) {
                let (storage, kept, binding) = (
                    O::reg(format!("vstore{slot}")),
                    O::reg(format!("vprev{slot}")),
                    O::reg(format!("s{slot}")),
                );
                vectors::emit_vector_keep_previous(
                    text,
                    module,
                    (&storage, &kept, &binding),
                    slot_ty,
                );
            }
            // Explicit ownership: the write gives back the previous content
            // (the IR releases it) and stores a value it owns.
            if let Some(previous) = previous {
                crate::ir::InstSink::assign(
                    text,
                    format!("v{}", previous.0),
                    crate::ir::LlvmInst::load(slot_llvm.clone(), O::reg(format!("s{slot}"))),
                );
            }
            let operand = if function.weak_symbols.contains(symbol) {
                arc_ops::weak_store_operand(text, &operand, state)
            } else {
                vectors::emit_vector_copy(
                    text,
                    module,
                    &O::reg(format!("vstore{slot}")),
                    &operand,
                    slot_ty,
                )
                .unwrap_or(operand)
            };
            text.emit(I::store(slot_llvm, operand, O::reg(format!("s{slot}"))));
            if analysis.released_symbols.contains(symbol) {
                text.emit(I::store(T::I1, O::bool(true), live_flag(*symbol)));
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
                    T::I1 => {
                        define_boolean_from(text, analysis, state, *destination, *source);
                    }
                    layout @ (T::I8 | T::I16 | T::I32 | T::I64) => {
                        let copy = I::binary(
                            BinaryOp::Add,
                            layout,
                            O::int(0),
                            O::reg(format!("v{}", source.0)),
                        );
                        text.assign(format!("v{}", destination.0), copy);
                    }
                    layout @ (T::Float | T::Double) => {
                        let copy = I::binary(
                            BinaryOp::FAdd,
                            layout,
                            O::float(0.0),
                            O::reg(format!("v{}", source.0)),
                        );
                        text.assign(format!("v{}", destination.0), copy);
                    }
                    T::Ptr => {
                        let copy = I::gep(
                            T::I8,
                            O::reg(format!("v{}", source.0)),
                            vec![(T::I64, O::int(0))],
                        );
                        text.assign(format!("v{}", destination.0), copy);
                    }
                    layout if layout == crate::layout::handle_result_ty() => {
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
                ("Plus", T::I8 | T::I16 | T::I32 | T::I64) => {
                    let operand_ty = analysis.values.get(operand).unwrap_or(ty);
                    let operand_op = coerce_to_type(text, *operand, operand_ty, ty);
                    let layout = llvm_type(ty).expect("validated integer unary type");
                    let plus = I::binary(BinaryOp::Add, layout, O::int(0), operand_op);
                    text.assign(format!("v{}", destination.0), plus);
                }
                ("Minus", T::I8 | T::I16 | T::I32 | T::I64) => {
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
                (sign @ ("Plus" | "Minus"), layout @ (T::Float | T::Double)) => {
                    let op = if sign == "Plus" {
                        BinaryOp::FAdd
                    } else {
                        BinaryOp::FSub
                    };
                    let value =
                        I::binary(op, layout, O::float(0.0), O::reg(format!("v{}", operand.0)));
                    text.assign(format!("v{}", destination.0), value);
                }
                ("NOT", T::I1) => {
                    let not = I::binary(
                        BinaryOp::Xor,
                        T::I1,
                        O::int(1),
                        O::reg(format!("v{}", operand.0)),
                    );
                    text.assign(format!("v{}", destination.0), not);
                }
                ("NOT", T::I8 | T::I16 | T::I32 | T::I64) => {
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
                let lowered = emit_is(text, *destination, *left, left_ty, right_ty);
                assert!(lowered, "target validation accepted this IS form");
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
                    module,
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
            if llvm_type(source_ty) == Some(crate::layout::optional_float_ty())
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
