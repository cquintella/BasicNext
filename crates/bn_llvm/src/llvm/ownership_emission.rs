// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Lowering of `RELEASE` (heap values and HOST handles), static fields,
// object field and member stores, and class initialization.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_ownership_emission(
    text: &mut String,
    module: &Module,
    _function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::Release { value, .. } => {
            let ty = analysis
                .values
                .get(value)
                .expect("validated delete value type");
            if arc_ops::holds_references(module, ty) {
                // Explicit ownership: the IR consumes one owned reference.
                arc_ops::emit_ownership(
                    text,
                    module,
                    arc_ops::Ownership::Release,
                    ty,
                    &format!("%v{}", value.0),
                    state,
                );
            } else if matches!(
                ty,
                Type::Integer(_)
                    | Type::IntegerLiteral(_)
                    | Type::Float(_)
                    | Type::FloatLiteral
                    | Type::Boolean
                    | Type::String
                    | Type::Vector { .. }
                    | Type::Null
            ) || is_struct_type(module, ty)
                || general_alternative(ty).is_some()
            {
                // RELEASE ends the binding (`EndBinding`); a value that holds
                // no reference needs no runtime work. A fixed vector's
                // storage is function-local; a STRUCT's is freed with its
                // function.
            } else if llvm_type(ty) == Some("{ i1, ptr, i64 }")
                && matches!(ty, Type::Alternative(alternatives) if alternatives.iter().any(|item| matches!(item, Type::ImportedNamed { name, .. } | Type::ImportedTypeName { name, .. } if name == "DataFrame")))
            {
                let v = value.0;
                let union = T::struct_of([T::I1, T::Ptr, T::I64]);
                let tag = format!("dfaltdelete{v}");
                let continuation = take_continuation(block_id, state);
                let own = O::reg(format!("v{v}"));
                text.assign(
                    format!("dfaltiserr{v}"),
                    I::extract(union.clone(), own.clone(), 0),
                );
                text.emit(I::CondBr {
                    cond: O::reg(format!("dfaltiserr{v}")),
                    true_dest: continuation.clone(),
                    false_dest: tag.clone(),
                });
                state.control_flow.label(text, tag);
                text.assign(format!("dfalthandle{v}"), I::extract(union, own, 2));
                let handle = vec![(T::I64, O::reg(format!("dfalthandle{v}")))];
                let call = I::call(T::I32, "bn_rt_dataframe_close", handle);
                emit_checked_i32_eq_zero(text, block_id, *value, call, &[], state);
                text.emit(I::Br {
                    dest: continuation.clone(),
                });
                state.control_flow.label(text, continuation);
            } else if let Some((slot, symbol, checked)) = release_call(module, ty) {
                // A handle-backed value: a `bn_rt` table index behind a ptr,
                // or slot 2 of the aggregate (`FS.File`, `HOST.Exec.Result`).
                let v = value.0;
                let own = O::reg(format!("v{v}"));
                let handle = format!("{slot}delhandle{v}");
                let inst = if llvm_type(ty) == Some("{ i1, ptr, i64 }") {
                    I::extract(T::struct_of([T::I1, T::Ptr, T::I64]), own, 2)
                } else {
                    I::cast(CastOp::PtrToInt, T::Ptr, own, T::I64)
                };
                text.assign(&handle, inst);
                let call = I::call(T::I32, symbol, vec![(T::I64, O::reg(handle))]);
                match checked {
                    Rc::Checked => {
                        emit_checked_i32_eq_zero(text, block_id, *value, call, &[], state);
                    }
                    Rc::Named => text.assign(format!("{slot}delrc{v}"), call),
                    Rc::Dropped => text.emit(call),
                }
            } else {
                emit_delete(text, module, *value, ty);
            }
        }
        Instruction::EnsureClass { class, .. } => {
            let flag = class_init_flag(class);
            let n = state.continuation_count;
            state.continuation_count += 1;
            let tag = format!("{}{n}", sanitize_symbol(class));
            text.assign(format!("initflag{tag}"), I::load(T::I1, O::raw(&flag)));
            text.emit(I::cond_br(
                O::reg(format!("initflag{tag}")),
                format!("initdone{tag}"),
                format!("initrun{tag}"),
            ));
            state.control_flow.label(text, format!("initrun{tag}"));
            text.emit(I::store(T::I1, O::bool(true), O::raw(&flag)));
            if let Some(init) = module.function_of_kind(FunctionKind::Init, class) {
                let init = llvm_function_symbol(&init.name);
                text.emit(I::call(T::Void, &init, vec![]));
            }
            text.emit(I::br(format!("initdone{tag}")));
            state.control_flow.label(text, format!("initdone{tag}"));
        }
        Instruction::LoadStatic {
            destination,
            class,
            field,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let llvm_ty = llvm_type(ty).expect("validated static type");
            let global = static_global_name(class, field);
            text.assign(
                format!("v{}", destination.0),
                I::load(crate::layout::typed_llvm(llvm_ty), O::raw(global)),
            );
        }
        Instruction::StoreStatic {
            class,
            field,
            value,
            previous,
            ty,
            ..
        } => {
            let llvm_ty = llvm_type(ty).expect("validated static type");
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated static value type");
            let operand = coerce_to_type(text, *value, value_ty, ty);
            let global = static_global_name(class, field);
            let storage = global.replacen("@bn_st_", "@bn_sv_", 1);
            if previous.is_some() && arc_ops::holds_references(module, ty) {
                let kept = global.replacen("@bn_st_", "@bn_svp_", 1);
                vectors::emit_vector_keep_previous(text, (&storage, &kept, &global), ty);
            }
            if let Some(previous) = previous {
                text.assign(
                    format!("v{}", previous.0),
                    I::load(crate::layout::typed_llvm(llvm_ty), O::raw(&global)),
                );
            }
            let operand =
                vectors::emit_vector_copy(text, &storage, &operand, ty).unwrap_or(operand);
            text.emit(I::store(
                crate::layout::typed_llvm(llvm_ty),
                O::raw(operand),
                O::raw(global),
            ));
        }
        Instruction::SetMember {
            object,
            field,
            name: _,
            owner: _,
            value,
            previous,
            ty,
            ..
        } => {
            let offset = field_byte_offset(
                module,
                field.as_ref().expect("validated member field reference"),
            )
            .expect("validated member field slot");
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated member value type");
            let weak = field
                .as_ref()
                .is_some_and(|field| module.field_is_weak(field));
            emit_set_member(
                text, *object, offset, *value, value_ty, ty, *previous, weak, state,
            );
        }
        Instruction::SetField {
            symbol,
            path: _,
            fields,
            value,
            previous,
            ty,
            ..
        } => {
            let resolved = fields.as_ref().and_then(|fields| fields.first());
            let offset =
                field_byte_offset(module, resolved.expect("validated field path reference"))
                    .expect("validated field path slot");
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated field value type");
            text.assign(
                format!("fieldobj{}", value.0),
                I::load(T::Ptr, O::reg(format!("s{}", symbols[symbol]))),
            );
            // Reuse SetMember emitter with a synthetic object value id name via temp.
            let llvm_ty = llvm_type(ty).expect("validated field type");
            let value_op = coerce_to_type(text, *value, value_ty, ty);
            let gep = I::gep(
                T::I8,
                O::reg(format!("fieldobj{}", value.0)),
                vec![(T::I32, O::int(i64::from(offset)))],
            );
            text.assign(format!("fieldptr{}", value.0), gep);
            if let Some(previous) = previous {
                text.assign(
                    format!("v{}", previous.0),
                    I::load(
                        crate::layout::typed_llvm(llvm_ty),
                        O::reg(format!("fieldptr{}", value.0)),
                    ),
                );
            }
            let value_op = if resolved.is_some_and(|field| module.field_is_weak(field)) {
                arc_ops::weak_store_operand(text, &value_op, state)
            } else {
                value_op
            };
            text.emit(I::store(
                crate::layout::typed_llvm(llvm_ty),
                O::raw(value_op),
                O::reg(format!("fieldptr{}", value.0)),
            ));
        }
        Instruction::SetFieldIndex {
            symbol,
            path: _,
            fields,
            indices,
            value,
            previous,
            ty,
            ..
        } => {
            let resolved = fields
                .as_ref()
                .and_then(|fields| fields.first())
                .expect("validated indexed field path reference");
            let offset =
                field_byte_offset(module, resolved).expect("validated indexed field path slot");
            let index = indices[0];
            emit_field_set_index(
                text,
                block_id,
                symbols[symbol],
                offset,
                index,
                analysis.values.get(&index).expect("validated index type"),
                *value,
                analysis.values.get(value).expect("validated value type"),
                ty,
                *previous,
                state,
            );
        }
        _ => return false,
    }
    true
}

/// What happens to the status of a handle's release call.
enum Rc {
    /// Non-zero traps with the failure `bn_rt` recorded.
    Checked,
    /// Bound to `%<slot>delrc` and unused.
    Named,
    /// Not bound.
    Dropped,
}

/// The `bn_rt` call that releases a handle-backed value of type `ty`, with the
/// slot prefix of its registers.
fn release_call(module: &Module, ty: &Type) -> Option<(&'static str, &'static str, Rc)> {
    let aggregate = llvm_type(ty) == Some("{ i1, ptr, i64 }");
    let exec = matches!(ty, Type::Alternative(alternatives) if alternatives.iter().any(
        |item| matches!(item, Type::Named(name) if name == "HOST.Exec.Result")
    ));
    Some(if is_bndata_dataframe_type(module, ty) {
        ("df", "bn_rt_dataframe_close", Rc::Checked)
    } else if aggregate && exec {
        ("exec", "bn_rt_exec_result_close", Rc::Dropped)
    } else if aggregate {
        ("file", "bn_rt_file_release", Rc::Dropped)
    } else if carries_bnjson(module, ty) {
        ("json", "bn_rt_json_release", Rc::Named)
    } else if is_bncrypto_bytes_type(module, ty) {
        ("cry", "bn_rt_crypto_bytes_release", Rc::Named)
    } else if is_bnsqlite_connection_type(module, ty) {
        ("sqlite", "bn_rt_sqlite_close", Rc::Named)
    } else {
        let symbol = if bnlog_resource_kind(module, ty)? == "Fields" {
            "bn_rt_log_fields_close"
        } else {
            "bn_rt_log_logger_delete"
        };
        ("log", symbol, Rc::Named)
    })
}
