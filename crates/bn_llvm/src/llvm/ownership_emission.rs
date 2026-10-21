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
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    function: &Function,
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
                    &O::reg(format!("v{}", value.0)),
                    state,
                );
                if function.kind == FunctionKind::ReleaseFields && matches!(ty, Type::Vector { .. })
                {
                    emit_delete(text, module, *value, ty);
                }
            } else if matches!(
                ty,
                Type::Integer(_)
                    | Type::IntegerLiteral(_)
                    | Type::Float(_)
                    | Type::FloatLiteral
                    | Type::Boolean
                    | Type::Vector { .. }
                    | Type::Null
            ) || general_alternative(ty).is_some()
                || struct_owner(module, ty).is_some()
            {
                // RELEASE ends the binding (`EndBinding`); a value that holds
                // no reference needs no runtime work. A fixed vector's
                // storage is function-local; a STRUCT's is freed with its
                // function.
            } else if llvm_type(ty) == Some(crate::layout::handle_result_ty())
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
                let inst = if llvm_type(ty) == Some(crate::layout::handle_result_ty()) {
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
            text.assign(format!("initflag{tag}"), I::load(T::I1, O::global(&flag)));
            text.emit(I::cond_br(
                O::reg(format!("initflag{tag}")),
                format!("initdone{tag}"),
                format!("initrun{tag}"),
            ));
            state.control_flow.label(text, format!("initrun{tag}"));
            text.emit(I::store(T::I1, O::bool(true), O::global(&flag)));
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
                I::load(llvm_ty, O::global(global)),
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
                let slots = (O::global(&storage), O::global(&kept), O::global(&global));
                vectors::emit_vector_keep_previous(
                    text,
                    module,
                    (&slots.0, &slots.1, &slots.2),
                    ty,
                );
            }
            if let Some(previous) = previous {
                text.assign(
                    format!("v{}", previous.0),
                    I::load(llvm_ty.clone(), O::global(&global)),
                );
            }
            if let Some(owner) = struct_owner(module, ty) {
                let bytes = class_instance_bytes(module, &owner);
                let args = vec![
                    (T::Ptr, O::global(&storage)),
                    (T::Ptr, operand),
                    (T::I64, O::uint(bytes)),
                    (T::I1, O::bool(false)),
                ];
                text.emit(I::call(T::Void, crate::runtime_abi::memmove().name, args));
                text.emit(I::store(T::Ptr, O::global(&storage), O::global(global)));
            } else {
                let operand =
                    vectors::emit_vector_copy(text, module, &O::global(&storage), &operand, ty)
                        .unwrap_or(operand);
                text.emit(I::store(llvm_ty, operand, O::global(global)));
            }
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
                field.as_ref().unwrap_or_else(|| {
                    panic!(
                        "{}",
                        bn_catalog::text("llvm", "msg.validated_member_field_reference")
                    )
                }),
            )
            .unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_member_field_slot")
                )
            });
            let value_ty = analysis
                .values
                .get(value)
                .expect("validated member value type");
            let weak = field
                .as_ref()
                .is_some_and(|field| module.field_is_weak(field));
            emit_set_member(
                text, module, *object, offset, *value, value_ty, ty, *previous, weak, state,
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
            let resolved = fields.as_ref().and_then(|fields| fields.last());
            let offset: u32 = fields.as_ref().map_or(0, |fields| {
                fields
                    .iter()
                    .map(|f| field_byte_offset(module, f).unwrap_or(0))
                    .sum()
            });
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
                    I::load(llvm_ty.clone(), O::reg(format!("fieldptr{}", value.0))),
                );
            }
            if let Some(owner) = struct_owner(module, ty) {
                let copy_witness = arc_runtime::struct_copy_witness_name(&owner);
                let destroy_witness = arc_runtime::struct_destroy_witness_name(&owner);
                let fieldptr = O::reg(format!("fieldptr{}", value.0));
                text.emit(I::call(
                    T::Void,
                    &destroy_witness,
                    vec![(T::Ptr, fieldptr.clone())],
                ));
                text.emit(I::call(
                    T::Void,
                    &copy_witness,
                    vec![(T::Ptr, fieldptr), (T::Ptr, value_op)],
                ));
            } else if matches!(ty, Type::Vector { .. }) {
                let fieldptr = O::reg(format!("fieldptr{}", value.0));
                vectors::emit_vector_field_store(text, fieldptr, value_op, ty, state);
            } else {
                let value_op = if resolved.is_some_and(|field| module.field_is_weak(field)) {
                    arc_ops::weak_store_operand(text, &value_op, state)
                } else {
                    value_op
                };
                text.emit(I::store(
                    llvm_ty,
                    value_op,
                    O::reg(format!("fieldptr{}", value.0)),
                ));
            }
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
                module,
                block_id,
                symbols[symbol],
                offset,
                index,
                analysis.values.get(&index).unwrap_or_else(|| {
                    panic!("{}", bn_catalog::text("llvm", "msg.validated_index_type"))
                }),
                *value,
                analysis.values.get(value).unwrap_or_else(|| {
                    panic!("{}", bn_catalog::text("llvm", "msg.validated_value_type"))
                }),
                ty,
                *previous,
                state,
            );
        }
        Instruction::SetMemberIndex {
            object,
            field,
            indices,
            value,
            previous,
            ty,
            ..
        } => {
            let offset = field_byte_offset(
                module,
                field.as_ref().unwrap_or_else(|| {
                    panic!(
                        "{}",
                        bn_catalog::text("llvm", "msg.validated_member_field_reference")
                    )
                }),
            )
            .unwrap_or_else(|| {
                panic!(
                    "{}",
                    bn_catalog::text("llvm", "msg.validated_member_field_slot")
                )
            });
            let index = indices[0];
            indexed_store::emit_member_set_index(
                text,
                module,
                block_id,
                *object,
                offset,
                index,
                analysis.values.get(&index).unwrap_or_else(|| {
                    panic!("{}", bn_catalog::text("llvm", "msg.validated_index_type"))
                }),
                *value,
                analysis.values.get(value).unwrap_or_else(|| {
                    panic!("{}", bn_catalog::text("llvm", "msg.validated_value_type"))
                }),
                ty,
                *previous,
                state,
            );
        }
        Instruction::SetStaticIndex {
            class,
            field,
            indices,
            value,
            previous,
            ty,
            ..
        } => {
            let index = indices[0];
            indexed_store::emit_static_set_index(
                text,
                module,
                block_id,
                class,
                field,
                index,
                analysis.values.get(&index).unwrap_or_else(|| {
                    panic!("{}", bn_catalog::text("llvm", "msg.validated_index_type"))
                }),
                *value,
                analysis.values.get(value).unwrap_or_else(|| {
                    panic!("{}", bn_catalog::text("llvm", "msg.validated_value_type"))
                }),
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

/// `BNSqlite.Connection` releases through `bn_rt_sqlite_close` only when the
/// sqlite feature emitted the rest of the ABI. Without it, a connection is not
/// a supported value and must not name a symbol absent from `libbn_rt.a`.
fn sqlite_connection_release(
    module: &Module,
    ty: &Type,
) -> Option<(&'static str, &'static str, Rc)> {
    #[cfg(not(feature = "sqlite"))]
    {
        let _ = (module, ty);
        None
    }
    #[cfg(feature = "sqlite")]
    {
        is_bnsqlite_connection_type(module, ty).then_some((
            "sqlite",
            "bn_rt_sqlite_close",
            Rc::Named,
        ))
    }
}

/// `BNJson.Json` releases through `bn_rt_json_release` only when the json
/// feature emitted the rest of the ABI. Without it, a document is not a
/// supported value and must not name a symbol absent from `libbn_rt.a`.
fn json_document_release(module: &Module, ty: &Type) -> Option<(&'static str, &'static str, Rc)> {
    #[cfg(not(feature = "json"))]
    {
        let _ = (module, ty);
        None
    }
    #[cfg(feature = "json")]
    {
        carries_bnjson(module, ty).then_some(("json", "bn_rt_json_release", Rc::Named))
    }
}

/// The `bn_rt` call that releases a handle-backed value of type `ty`, with the
/// slot prefix of its registers.
fn release_call(module: &Module, ty: &Type) -> Option<(&'static str, &'static str, Rc)> {
    let aggregate = llvm_type(ty) == Some(crate::layout::handle_result_ty());
    let exec = matches!(ty, Type::Alternative(alternatives) if alternatives.iter().any(
        |item| matches!(item, Type::Named(name) if name == "HOST.Exec.Result")
    ));
    Some(if is_bndata_dataframe_type(module, ty) {
        ("df", "bn_rt_dataframe_close", Rc::Checked)
    } else if aggregate && exec {
        ("exec", "bn_rt_exec_result_close", Rc::Dropped)
    } else if aggregate {
        ("file", "bn_rt_file_release", Rc::Dropped)
    } else if let Some(release) = json_document_release(module, ty) {
        release
    } else if is_bncrypto_bytes_type(module, ty) {
        ("cry", "bn_rt_crypto_bytes_release", Rc::Named)
    } else if let Some(release) = sqlite_connection_release(module, ty) {
        release
    } else {
        bnlog_resource_kind(module, ty)?;
        let symbol = bnlog_release_symbol();
        ("log", symbol, Rc::Named)
    })
}
