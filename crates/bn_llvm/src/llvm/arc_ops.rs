// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The explicit ownership operations of the validated IR (proposal
//! `arc-shared-core-0.6.5`) on the native target. The IR says when to retain
//! and release; this module applies one operation to one value, by the shape
//! of its type, through the glue of `arc_runtime` (the counting is in
//! `bn_rt::arc`).

#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{self, I32, I64, Ptr},
};

/// Retain or release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Ownership {
    Retain,
    Release,
}

/// A class instance or an interface value (it holds one).
pub(crate) fn is_object_type(module: &Module, ty: &Type) -> bool {
    is_class_type(module, ty)
        || matches!(ty, Type::Named(name) if module.interfaces.contains(name))
        || matches!(ty, Type::ImportedNamed { module: id, name }
            if module.interfaces.contains(&format!("#{}.{name}", id.0)))
}

/// The member of a `T OR NULL` pointer form that holds an object, if `ty` is
/// one.
fn nullable_object(module: &Module, ty: &Type) -> bool {
    matches!(ty, Type::Alternative(members)
        if llvm_type(ty) == Some(LlvmType::Ptr)
            && members.iter().any(|member| is_object_type(module, member)))
}

/// `POINTER TO T OR NULL`: one region fat pointer, or null. Not a buffer
/// `free` can take — field 0 is the data, not the allocation base.
fn is_nullable_region(ty: &Type) -> bool {
    matches!(ty, Type::Alternative(members)
        if crate::layout::AlternativeLayout::of(members)
            == Some(crate::layout::AlternativeLayout::Region))
}

fn llvm(ty: &Type) -> LlvmType {
    llvm_type(ty).expect("validated ARC value type")
}

/// The `line` and `column` of the instruction being emitted, for the
/// `BN_ARC_TRACE` lines the core writes.
fn site(state: &EmissionState) -> (i64, i64) {
    (
        i64::try_from(state.span.start.line).unwrap_or(i64::MAX),
        i64::try_from(state.span.start.column).unwrap_or(i64::MAX),
    )
}

/// A fresh register name `{prefix}{n}`.
fn fresh(state: &mut EmissionState, prefix: &str) -> String {
    let n = state.continuation_count;
    state.continuation_count += 1;
    format!("{prefix}{n}")
}

/// The header base of a region fat pointer (null stays null).
pub(crate) fn region_base(
    text: &mut crate::ir::LlvmModule,
    fat: &crate::ir::LlvmOperand,
    ty: &Type,
    state: &mut EmissionState,
) -> crate::ir::LlvmOperand {
    let tag = fresh(state, "rbase");
    let data = format!("{tag}_data");
    let fat_ty = llvm(ty);
    text.assign(&data, LlvmInst::extract(fat_ty, fat.clone(), 0));
    text.assign(
        format!("{tag}_null"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            Ptr,
            LlvmOperand::reg(&data),
            LlvmOperand::null(),
        ),
    );
    text.assign(
        format!("{tag}_header"),
        LlvmInst::GetElementPtr {
            inbounds: false,
            elem_ty: LlvmType::I8,
            ptr: LlvmOperand::reg(&data),
            indices: vec![(
                I64,
                LlvmOperand::int(-i64::try_from(REGION_HEADER_BYTES).expect("header fits i64")),
            )],
        },
    );
    text.assign(
        &tag,
        LlvmInst::select(
            LlvmOperand::reg(format!("{tag}_null")),
            Ptr,
            LlvmOperand::null(),
            LlvmOperand::reg(format!("{tag}_header")),
        ),
    );
    LlvmOperand::reg(&tag)
}

/// Applies `op` to every object and region that `operand`, a value of type
/// `ty`, holds: an object, a `T OR NULL` pointer, a general alternative
/// whose member is an object, a region, the strong fields of a `STRUCT`
/// (in reverse declaration order for a release, as an object's fields), or
/// the elements of a fixed vector.
pub(crate) fn emit_ownership(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    op: Ownership,
    ty: &Type,
    operand: &crate::ir::LlvmOperand,
    state: &mut EmissionState,
) {
    let glue = match op {
        Ownership::Retain => arc_runtime::RETAIN,
        Ownership::Release => arc_runtime::RELEASE,
    };
    let (line, column) = site(state);
    let call = |text: &mut crate::ir::LlvmModule, object: &crate::ir::LlvmOperand| {
        text.emit(LlvmInst::call(
            LlvmType::Void,
            glue,
            vec![
                (Ptr, object.clone()),
                (I32, LlvmOperand::int(line)),
                (I32, LlvmOperand::int(column)),
            ],
        ));
    };
    if crate::log_emission::holds_counted_logger(module, ty) {
        let symbol = match op {
            Ownership::Retain => crate::log_emission::bnlog_retain_symbol(),
            Ownership::Release => bnlog_release_symbol(),
        };
        let tag = fresh(state, "logown");
        crate::log_emission::emit_logger_ownership(text, ty, operand, &tag, symbol);
    } else if is_fs_file(ty) {
        // Before the object path: a file aggregate is not an ARC pointer.
        emit_file_count(text, op, ty, operand, state);
    } else if is_object_type(module, ty) || nullable_object(module, ty) {
        call(text, operand);
    } else if general_alternative(ty).is_some() && object_class(module, ty).is_some() {
        let name = fresh(state, "heldobj");
        let object = held_object(text, ty, operand, &name);
        call(text, &object);
    } else if is_region_type(ty) || is_nullable_region(ty) {
        // `POINTER TO T OR NULL` is the same `{ ptr, i32 }` fat pointer. The
        // count lives in the header 16 bytes before the data, as for `POINTER TO T`.
        let base = region_base(text, operand, ty, state);
        call(text, &base);
    } else if let Some(owner) = struct_owner(module, ty) {
        let mut fields = class_layout_fields(module, &owner);
        if op == Ownership::Release {
            fields.reverse();
        }
        // An emptied slot (`Take` after `RELEASE`) holds no STRUCT.
        let done = skip_if_null(text, operand, state);
        for field in fields {
            if module.field_is_weak(&field.reference) || !holds_references(module, &field.ty) {
                continue;
            }
            let offset = field_byte_offset(module, &field.reference).expect("validated field");
            let tag = fresh(state, "structarc");
            text.assign(
                format!("{tag}_ptr"),
                LlvmInst::GetElementPtr {
                    inbounds: false,
                    elem_ty: LlvmType::I8,
                    ptr: operand.clone(),
                    indices: vec![(I32, LlvmOperand::int(i64::from(offset)))],
                },
            );
            text.assign(
                &tag,
                LlvmInst::load(llvm(&field.ty), LlvmOperand::reg(format!("{tag}_ptr"))),
            );
            emit_ownership(text, module, op, &field.ty, &LlvmOperand::reg(&tag), state);
        }
        join(text, done, state);
    } else if let Type::Vector {
        element,
        dimensions,
    } = ty
        && holds_references(module, element)
    {
        // A vector of more than one dimension is stored as rows: its data
        // holds one `{ ptr, i32 }` row per first index, each walked as the
        // vector of the remaining dimensions (K30), never as objects.
        let (count, item) = match dimensions.split_first() {
            Some((rows, rest)) if !rest.is_empty() => (
                *rows,
                Type::Vector {
                    element: element.clone(),
                    dimensions: rest.to_vec(),
                },
            ),
            _ => (dimensions.iter().product::<u64>(), (**element).clone()),
        };
        let element = &item;
        let tag = fresh(state, "vecarc");
        text.assign(
            format!("{tag}_data"),
            LlvmInst::extract(llvm(ty), operand.clone(), 0),
        );
        // An emptied slot (`Take` after `RELEASE`) holds no elements.
        let done = skip_if_null(text, &LlvmOperand::reg(format!("{tag}_data")), state);
        for index in 0..count {
            let element_reg = format!("{tag}_{index}");
            text.assign(
                format!("{element_reg}_ptr"),
                LlvmInst::GetElementPtr {
                    inbounds: false,
                    elem_ty: llvm(element),
                    ptr: LlvmOperand::reg(format!("{tag}_data")),
                    indices: vec![(
                        I64,
                        LlvmOperand::int(i64::try_from(index).expect("vector index fits i64")),
                    )],
                },
            );
            text.assign(
                &element_reg,
                LlvmInst::load(
                    llvm(element),
                    LlvmOperand::reg(format!("{element_reg}_ptr")),
                ),
            );
            emit_ownership(
                text,
                module,
                op,
                element,
                &LlvmOperand::reg(&element_reg),
                state,
            );
        }
        join(text, done, state);
    } else if *ty == Type::String {
        emit_string_buffer(text, op, operand);
    } else if contains_string(module, ty)
        && !is_object_type(module, ty)
        && object_class(module, ty).is_none()
    {
        emit_alternative_string(text, op, ty, operand, state);
    }
}

/// `FS.File`, or an alternative that has `FS.File` as a member (`FS.File OR Error`).
fn is_fs_file(ty: &Type) -> bool {
    match ty {
        Type::Named(name) | Type::TypeName(name) => name == "FS.File",
        Type::Alternative(members) => members.iter().any(is_fs_file),
        _ => false,
    }
}

/// One strong reference on the file handle in field 2. An error alternative
/// (field 0) holds no file. The call is `bn_rt_file_retain` / `bn_rt_file_release`,
/// never `bn_rt_arc_retain`.
fn emit_file_count(
    text: &mut crate::ir::LlvmModule,
    op: Ownership,
    ty: &Type,
    operand: &crate::ir::LlvmOperand,
    state: &mut EmissionState,
) {
    let Some(layout) = llvm_type(ty) else {
        return;
    };
    if layout != crate::layout::handle_result_ty() {
        return;
    }
    let symbol = match op {
        Ownership::Retain => "bn_rt_file_retain",
        Ownership::Release => "bn_rt_file_release",
    };
    let tag = fresh(state, "fileown");
    text.assign(
        format!("{tag}_err"),
        LlvmInst::extract(layout.clone(), operand.clone(), 0),
    );
    text.assign(
        format!("{tag}_handle"),
        LlvmInst::extract(layout, operand.clone(), 2),
    );
    text.emit(LlvmInst::CondBr {
        cond: LlvmOperand::reg(format!("{tag}_err")),
        true_dest: format!("{tag}_done"),
        false_dest: format!("{tag}_walk"),
    });
    state.control_flow.label(text, format!("{tag}_walk"));
    text.emit(LlvmInst::call(
        I32,
        symbol,
        vec![(I64, LlvmOperand::reg(format!("{tag}_handle")))],
    ));
    text.emit(LlvmInst::Br {
        dest: format!("{tag}_done"),
    });
    state.control_flow.label(text, format!("{tag}_done"));
}

/// `bn_rt_str_retain` / `bn_rt_str_release` on one STRING payload.
fn emit_string_buffer(
    text: &mut crate::ir::LlvmModule,
    op: Ownership,
    operand: &crate::ir::LlvmOperand,
) {
    let symbol = match op {
        Ownership::Retain => "bn_rt_str_retain",
        Ownership::Release => "bn_rt_str_release",
    };
    text.emit(LlvmInst::call(I32, symbol, vec![(Ptr, operand.clone())]));
}

/// A status or general alternative that holds a STRING. An error record is
/// not a STRING. A general alternative releases field 1 only for the string tag.
fn emit_alternative_string(
    text: &mut crate::ir::LlvmModule,
    op: Ownership,
    ty: &Type,
    operand: &crate::ir::LlvmOperand,
    state: &mut EmissionState,
) {
    let Some(layout) = llvm_type(ty) else {
        return;
    };
    if layout == LlvmType::Ptr {
        emit_string_buffer(text, op, operand);
        return;
    }
    if layout == crate::layout::handle_result_ty() {
        let tag = fresh(state, "strow");
        text.assign(
            format!("{tag}_err"),
            LlvmInst::extract(layout.clone(), operand.clone(), 0),
        );
        text.assign(
            format!("{tag}_ptr"),
            LlvmInst::extract(layout, operand.clone(), 1),
        );
        text.emit(LlvmInst::CondBr {
            cond: LlvmOperand::reg(format!("{tag}_err")),
            true_dest: format!("{tag}_done"),
            false_dest: format!("{tag}_walk"),
        });
        state.control_flow.label(text, format!("{tag}_walk"));
        emit_string_buffer(text, op, &LlvmOperand::reg(format!("{tag}_ptr")));
        text.emit(LlvmInst::Br {
            dest: format!("{tag}_done"),
        });
        state.control_flow.label(text, format!("{tag}_done"));
        return;
    }
    if layout == crate::general_alternative::layout() {
        let Some(code) = bn_types::alternatives::member_code(&Type::String) else {
            return;
        };
        let tag = fresh(state, "genstr");
        text.assign(
            format!("{tag}_tag"),
            LlvmInst::extract(layout.clone(), operand.clone(), 0),
        );
        text.assign(
            format!("{tag}_ptr"),
            LlvmInst::extract(layout, operand.clone(), 1),
        );
        text.assign(
            format!("{tag}_isstr"),
            LlvmInst::icmp(
                ICmpCond::Eq,
                I32,
                LlvmOperand::reg(format!("{tag}_tag")),
                LlvmOperand::int(i64::from(code)),
            ),
        );
        text.emit(LlvmInst::CondBr {
            cond: LlvmOperand::reg(format!("{tag}_isstr")),
            true_dest: format!("{tag}_walk"),
            false_dest: format!("{tag}_done"),
        });
        state.control_flow.label(text, format!("{tag}_walk"));
        emit_string_buffer(text, op, &LlvmOperand::reg(format!("{tag}_ptr")));
        text.emit(LlvmInst::Br {
            dest: format!("{tag}_done"),
        });
        state.control_flow.label(text, format!("{tag}_done"));
    }
}

/// Branches past the following code when the pointer `operand` is null;
/// returns the label [`join`] opens.
fn skip_if_null(
    text: &mut crate::ir::LlvmModule,
    operand: &crate::ir::LlvmOperand,
    state: &mut EmissionState,
) -> String {
    let tag = fresh(state, "arcwalk");
    text.assign(
        format!("{tag}_null"),
        LlvmInst::icmp(ICmpCond::Eq, Ptr, operand.clone(), LlvmOperand::null()),
    );
    text.emit(LlvmInst::CondBr {
        cond: LlvmOperand::reg(format!("{tag}_null")),
        true_dest: format!("{tag}_done"),
        false_dest: format!("{tag}_walk"),
    });
    state.control_flow.label(text, format!("{tag}_walk"));
    format!("{tag}_done")
}

/// Joins the path [`skip_if_null`] split.
fn join(text: &mut crate::ir::LlvmModule, done: String, state: &mut EmissionState) {
    text.emit(LlvmInst::Br { dest: done.clone() });
    state.control_flow.label(text, done);
}

/// Whether values of `ty` hold strong references the native code counts
/// (the shapes `emit_ownership` handles).
pub(crate) fn holds_references(module: &Module, ty: &Type) -> bool {
    is_object_type(module, ty)
        || crate::log_emission::holds_counted_logger(module, ty)
        || nullable_object(module, ty)
        || general_alternative(ty).is_some() && object_class(module, ty).is_some()
        || is_region_type(ty)
        || is_nullable_region(ty)
        || struct_owner(module, ty).is_some_and(|owner| {
            class_layout_fields(module, &owner).iter().any(|field| {
                !module.field_is_weak(&field.reference) && holds_references(module, &field.ty)
            })
        })
        || matches!(ty, Type::Vector { element, .. } if holds_references(module, element))
        || *ty == Type::String
        || is_fs_file(ty)
        || matches!(ty, Type::Alternative(_) if contains_string(module, ty))
}

/// Whether `ty` contains a STRING buffer the native code counts.
pub(crate) fn contains_string(module: &Module, ty: &Type) -> bool {
    match ty {
        Type::String => true,
        Type::Vector { element, .. } => contains_string(module, element),
        Type::Alternative(members) => members.iter().any(|member| contains_string(module, member)),
        _ => struct_owner(module, ty).is_some_and(|owner| {
            class_layout_fields(module, &owner).iter().any(|field| {
                !module.field_is_weak(&field.reference) && contains_string(module, &field.ty)
            })
        }),
    }
}

/// The zero value of `ty`'s native representation: what a slot holds once
/// `Take` moved its content out.
pub(crate) fn zero(ty: &Type) -> LlvmOperand {
    if llvm_type(ty) == Some(LlvmType::Ptr) {
        LlvmOperand::null()
    } else {
        LlvmOperand::zero_initializer()
    }
}

/// What a weak binding or field stores for the object `object` (a `ptr`):
/// the object's core id, carried in a `ptr` slot (0.6.5, the core keeps the
/// address).
pub(crate) fn weak_store_operand(
    text: &mut crate::ir::LlvmModule,
    object: &crate::ir::LlvmOperand,
    state: &mut EmissionState,
) -> crate::ir::LlvmOperand {
    let tag = fresh(state, "weakid");
    text.assign(
        format!("{tag}_id"),
        LlvmInst::call(I64, arc_runtime::WEAK_ID, vec![(Ptr, object.clone())]),
    );
    text.assign(
        &tag,
        LlvmInst::cast(
            CastOp::IntToPtr,
            I64,
            LlvmOperand::reg(format!("{tag}_id")),
            Ptr,
        ),
    );
    LlvmOperand::reg(&tag)
}

/// The object a weak binding or field reads from what it stores: the
/// object while it lives, else null.
pub(crate) fn weak_read(text: &mut crate::ir::LlvmModule, dest: &str, stored: &LlvmOperand) {
    text.assign(
        format!("{dest}_weakid"),
        LlvmInst::cast(CastOp::PtrToInt, Ptr, stored.clone(), I64),
    );
    text.assign(
        dest,
        LlvmInst::call(
            Ptr,
            arc_runtime::WEAK_READ,
            vec![(I64, LlvmOperand::reg(format!("{dest}_weakid")))],
        ),
    );
}

/// Registers the allocation at `base` (an object of the class named by the
/// constant `class`, or a region for `None`) in the ARC core and stores the
/// id in its header.
pub(crate) fn register(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    class: Option<&str>,
    base: &LlvmOperand,
    state: &EmissionState,
) {
    let tag = format!("arcid{}", destination.0);
    let (line, column) = site(state);
    text.assign(
        &tag,
        crate::runtime_abi::ARC_REGISTER.call([
            class.map_or_else(LlvmOperand::null, LlvmOperand::global),
            base.clone(),
            LlvmOperand::int(line),
            LlvmOperand::int(column),
        ]),
    );
    text.assign(
        format!("{tag}_ptr"),
        LlvmInst::GetElementPtr {
            inbounds: false,
            elem_ty: LlvmType::I8,
            ptr: base.clone(),
            indices: vec![(I64, LlvmOperand::int(arc_runtime::ID_OFFSET))],
        },
    );
    text.emit(LlvmInst::store(
        I64,
        LlvmOperand::reg(&tag),
        LlvmOperand::reg(format!("{tag}_ptr")),
    ));
}

/// The explicit ownership instructions: `Retain`, `Take`, `TakeMember`,
/// `EndBinding`. Returns `false` for any other instruction.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_ownership_instruction(
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
        Instruction::Retain {
            destination,
            value,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let mut operand = LlvmOperand::reg(format!("v{}", value.0));
            let ty = analysis.values.get(destination).unwrap_or(ty);
            // A narrowed `Logger OR Error` (the `IS Error` branch left) is
            // retained as the logger it holds in field 1.
            if llvm_type(ty) == Some(LlvmType::Ptr)
                && analysis.values.get(value).and_then(llvm_type)
                    == Some(crate::layout::handle_result_ty())
            {
                let narrowed = format!("narrowed{}", destination.0);
                text.assign(
                    &narrowed,
                    LlvmInst::extract(crate::layout::handle_result_ty(), operand.clone(), 1),
                );
                operand = LlvmOperand::reg(&narrowed);
            }
            emit_ownership(text, module, Ownership::Retain, ty, &operand, state);
            // The retained value is the same value, now owned.
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::select(
                    LlvmOperand::bool(true),
                    llvm(ty),
                    operand.clone(),
                    operand.clone(),
                ),
            );
        }
        Instruction::Take {
            destination,
            symbol,
            ty,
            ..
        } => {
            // The same read as a `Load` of the binding (a narrowed binding
            // yields its narrowed value), then the slot is emptied.
            lower_load(
                text,
                block_id,
                function,
                analysis,
                symbols,
                block_state,
                *destination,
                *symbol,
                ty,
                false,
                state,
            );
            block_state.bindings.remove(symbol);
            let slot_ty = analysis.symbols.get(symbol).unwrap_or(ty);
            let slot = LlvmOperand::reg(format!("s{}", symbols[symbol]));
            text.emit(LlvmInst::store(llvm(slot_ty), zero(slot_ty), slot));
        }
        Instruction::TakeMember {
            destination,
            object,
            field,
            ty,
            ..
        } => {
            block_state.constants.remove(destination);
            let offset = field_byte_offset(
                module,
                field.as_ref().expect("validated member field reference"),
            )
            .expect("validated member field slot");
            let field_ptr = format!("takemember{}", destination.0);
            text.assign(
                &field_ptr,
                LlvmInst::GetElementPtr {
                    inbounds: false,
                    elem_ty: LlvmType::I8,
                    ptr: LlvmOperand::reg(format!("v{}", object.0)),
                    indices: vec![(I32, LlvmOperand::int(i64::from(offset)))],
                },
            );
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::load(llvm(ty), LlvmOperand::reg(&field_ptr)),
            );
            text.emit(LlvmInst::store(
                llvm(ty),
                zero(ty),
                LlvmOperand::reg(&field_ptr),
            ));
        }
        // 0.6.md, "`STOP`": the check after a call that may stop, and the
        // code to stop with (see `arc_runtime::STOPPING_GLOBAL`).
        Instruction::Call {
            destination,
            callee,
            ..
        } if analysis.functions.get(callee).copied() == Some(bn_ir::names::STOPPING) => {
            let flag = LlvmOperand::global(arc_runtime::STOPPING_GLOBAL);
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::load(LlvmType::I1, flag.clone()),
            );
            text.emit(LlvmInst::store(
                LlvmType::I1,
                LlvmOperand::bool(false),
                flag,
            ));
        }
        Instruction::Call {
            destination,
            callee,
            ..
        } if analysis.functions.get(callee).copied() == Some(bn_ir::names::STOP_CODE) => {
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::load(I32, LlvmOperand::global(arc_runtime::STOP_CODE_GLOBAL)),
            );
        }
        Instruction::EndBinding { symbol, .. } => {
            // RELEASE ends the binding: a second RELEASE is DOUBLE_RELEASE;
            // a later use is checked by its Load.
            let live_flag = live_flag(*symbol);
            let tag = fresh(state, "endbinding");
            text.assign(
                format!("{tag}_live"),
                LlvmInst::load(LlvmType::I1, live_flag.clone()),
            );
            text.assign(
                format!("{tag}_released"),
                LlvmInst::binary(
                    crate::ir::BinaryOp::Xor,
                    LlvmType::I1,
                    LlvmOperand::reg(format!("{tag}_live")),
                    LlvmOperand::bool(true),
                ),
            );
            let live = take_continuation(block_id, state);
            emit_trap(
                text,
                block_id,
                state,
                &LlvmOperand::reg(format!("{tag}_released")),
                live,
                bn_diag::DiagId::DOUBLE_RELEASE,
                vec![(
                    "detail",
                    Fact::Text(bn_diag::trap_texts::BINDING_ALREADY_RELEASED.into()),
                )],
            );
            text.emit(LlvmInst::store(
                LlvmType::I1,
                LlvmOperand::bool(false),
                live_flag,
            ));
        }
        _ => return false,
    }
    true
}
