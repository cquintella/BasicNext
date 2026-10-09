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
        if llvm_type(ty) == Some("ptr")
            && members.iter().any(|member| is_object_type(module, member)))
}

fn llvm(ty: &Type) -> LlvmType {
    LlvmType::parse_canonical(llvm_type(ty).expect("validated ARC value type"))
        .expect("canonical LLVM type")
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
    text: &mut String,
    fat: &str,
    ty: &Type,
    state: &mut EmissionState,
) -> String {
    let tag = fresh(state, "rbase");
    let data = format!("{tag}_data");
    let fat_ty = llvm(ty);
    text.assign(&data, LlvmInst::extract(fat_ty, LlvmOperand::raw(fat), 0));
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
    format!("%{tag}")
}

/// Applies `op` to every object and region that `operand`, a value of type
/// `ty`, holds: an object, a `T OR NULL` pointer, a general alternative
/// whose member is an object, a region, the strong fields of a `STRUCT`
/// (in reverse declaration order for a release, as an object's fields), or
/// the elements of a fixed vector.
pub(crate) fn emit_ownership(
    text: &mut String,
    module: &Module,
    op: Ownership,
    ty: &Type,
    operand: &str,
    state: &mut EmissionState,
) {
    let glue = match op {
        Ownership::Retain => arc_runtime::RETAIN,
        Ownership::Release => arc_runtime::RELEASE,
    };
    let (line, column) = site(state);
    let call = |text: &mut String, object: &str| {
        text.emit(LlvmInst::call(
            LlvmType::Void,
            glue,
            vec![
                (Ptr, LlvmOperand::raw(object)),
                (I32, LlvmOperand::int(line)),
                (I32, LlvmOperand::int(column)),
            ],
        ));
    };
    if is_object_type(module, ty) || nullable_object(module, ty) {
        call(text, operand);
    } else if general_alternative(ty).is_some() && object_class(module, ty).is_some() {
        let name = fresh(state, "heldobj");
        let object = held_object(text, ty, operand, &name);
        call(text, &object);
    } else if is_region_type(ty) {
        let base = region_base(text, operand, ty, state);
        call(text, &base);
    } else if is_struct_type(module, ty) {
        let Type::Named(owner) = ty else {
            unreachable!("a STRUCT type is named");
        };
        let mut fields = class_layout_fields(module, owner);
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
                    ptr: LlvmOperand::raw(operand),
                    indices: vec![(I32, LlvmOperand::int(i64::from(offset)))],
                },
            );
            text.assign(
                &tag,
                LlvmInst::load(llvm(&field.ty), LlvmOperand::reg(format!("{tag}_ptr"))),
            );
            emit_ownership(text, module, op, &field.ty, &format!("%{tag}"), state);
        }
        join(text, done, state);
    } else if let Type::Vector {
        element,
        dimensions,
    } = ty
        && holds_references(module, element)
    {
        let count = dimensions.iter().product::<u64>();
        let tag = fresh(state, "vecarc");
        text.assign(
            format!("{tag}_data"),
            LlvmInst::extract(llvm(ty), LlvmOperand::raw(operand), 0),
        );
        // An emptied slot (`Take` after `RELEASE`) holds no elements.
        let done = skip_if_null(text, &format!("%{tag}_data"), state);
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
            emit_ownership(text, module, op, element, &format!("%{element_reg}"), state);
        }
        join(text, done, state);
    }
}

/// Branches past the following code when the pointer `operand` is null;
/// returns the label [`join`] opens.
fn skip_if_null(text: &mut String, operand: &str, state: &mut EmissionState) -> String {
    let tag = fresh(state, "arcwalk");
    text.assign(
        format!("{tag}_null"),
        LlvmInst::icmp(
            ICmpCond::Eq,
            Ptr,
            LlvmOperand::raw(operand),
            LlvmOperand::null(),
        ),
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
fn join(text: &mut String, done: String, state: &mut EmissionState) {
    text.emit(LlvmInst::Br { dest: done.clone() });
    state.control_flow.label(text, done);
}

/// Whether values of `ty` hold strong references the native code counts
/// (the shapes `emit_ownership` handles).
pub(crate) fn holds_references(module: &Module, ty: &Type) -> bool {
    is_object_type(module, ty)
        || nullable_object(module, ty)
        || general_alternative(ty).is_some() && object_class(module, ty).is_some()
        || is_region_type(ty)
        || is_struct_type(module, ty)
            && matches!(ty, Type::Named(owner) if class_layout_fields(module, owner)
                .iter()
                .any(|field| !module.field_is_weak(&field.reference)
                    && holds_references(module, &field.ty)))
        || matches!(ty, Type::Vector { element, .. } if holds_references(module, element))
}

/// The zero value of `ty`'s native representation: what a slot holds once
/// `Take` moved its content out.
pub(crate) fn zero(ty: &Type) -> LlvmOperand {
    if llvm_type(ty) == Some("ptr") {
        LlvmOperand::null()
    } else {
        LlvmOperand::zero_initializer()
    }
}

/// What a weak binding or field stores for the object `object` (a `ptr`):
/// the object's core id, carried in a `ptr` slot (0.6.5, the core keeps the
/// address).
pub(crate) fn weak_store_operand(
    text: &mut String,
    object: &str,
    state: &mut EmissionState,
) -> String {
    let tag = fresh(state, "weakid");
    text.assign(
        format!("{tag}_id"),
        LlvmInst::call(
            I64,
            arc_runtime::WEAK_ID,
            vec![(Ptr, LlvmOperand::raw(object))],
        ),
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
    format!("%{tag}")
}

/// The object a weak binding or field reads from what it stores: the
/// object while it lives, else null.
pub(crate) fn weak_read(text: &mut String, dest: &str, stored: &str) {
    text.assign(
        format!("{dest}_weakid"),
        LlvmInst::cast(CastOp::PtrToInt, Ptr, LlvmOperand::raw(stored), I64),
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
    text: &mut String,
    destination: ValueId,
    class: Option<&str>,
    base: &str,
    state: &EmissionState,
) {
    let tag = format!("arcid{}", destination.0);
    let (line, column) = site(state);
    text.assign(
        &tag,
        arc_runtime::ARC_REGISTER.call([
            class.map_or_else(LlvmOperand::null, LlvmOperand::global),
            LlvmOperand::raw(base),
            LlvmOperand::int(line),
            LlvmOperand::int(column),
        ]),
    );
    text.assign(
        format!("{tag}_ptr"),
        LlvmInst::GetElementPtr {
            inbounds: false,
            elem_ty: LlvmType::I8,
            ptr: LlvmOperand::raw(base),
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
    text: &mut String,
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
            let operand = format!("%v{}", value.0);
            let ty = analysis.values.get(destination).unwrap_or(ty);
            emit_ownership(text, module, Ownership::Retain, ty, &operand, state);
            // The retained value is the same value, now owned.
            text.assign(
                format!("v{}", destination.0),
                LlvmInst::select(
                    LlvmOperand::bool(true),
                    llvm(ty),
                    LlvmOperand::raw(&operand),
                    LlvmOperand::raw(&operand),
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
            let slot = LlvmOperand::raw(format!("%s{}", symbols[symbol]));
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
                    ptr: LlvmOperand::raw(format!("%v{}", object.0)),
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
                LlvmInst::load(LlvmType::I1, LlvmOperand::raw(&live_flag)),
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
                &format!("%{tag}_released"),
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
                LlvmOperand::raw(live_flag),
            ));
        }
        _ => return false,
    }
    true
}
