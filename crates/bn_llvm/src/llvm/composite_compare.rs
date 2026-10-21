// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Comparison of composite values: STRUCT field-by-field and vector element-by-element.
#![allow(
    clippy::wildcard_imports,
    clippy::too_many_arguments,
    clippy::similar_names
)]
use bn_ir::{Module, ValueId};
use bn_types::Type;

use super::*;
use crate::ir::{BinaryOp, FCmpCond, ICmpCond, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{
    class_layout_fields, field_byte_offset, is_struct_type, llvm_type, struct_owner, vector_ty,
};
use crate::runtime_abi::STR_EQ;

/// Above this length a vector compare is one `memcmp` (integer elements) or
/// one counted loop. At or below it, each element is compared in place
/// (known issue K84).
const VECTOR_COMPARE_UNROLL_LIMIT: u64 = 16;

/// Whether comparing `ty` emits `memcmp` for a long integer vector, including
/// one nested inside a wider vector.
pub(crate) fn vector_needs_memcmp(ty: &Type) -> bool {
    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        return false;
    };
    let count = dimensions.first().copied().unwrap_or(0);
    if dimensions.len() == 1
        && count > VECTOR_COMPARE_UNROLL_LIMIT
        && integer_element_bytes(element).is_some()
    {
        return true;
    }
    if dimensions.len() > 1 {
        let inner = Type::Vector {
            element: element.clone(),
            dimensions: dimensions[1..].to_vec(),
        };
        return vector_needs_memcmp(&inner);
    }
    false
}

fn integer_element_bytes(ty: &Type) -> Option<u64> {
    match ty {
        Type::Integer(kind) => Some(u64::from(kind.width() / 8)),
        _ => None,
    }
}

pub(crate) fn emit_struct_compare(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
    let tag = text.fresh_tag();
    let eq_dest = format!("structeq{tag}");
    let left_op = O::reg(format!("v{}", left.0));
    let right_op = O::reg(format!("v{}", right.0));
    emit_struct_pointers_compare(text, module, &eq_dest, ty, left_op, right_op, state);
    let final_dest = format!("v{}", destination.0);
    if matches!(operator, "NotEqual") {
        text.assign(
            final_dest,
            I::binary(BinaryOp::Xor, T::I1, O::reg(eq_dest), O::bool(true)),
        );
    } else {
        text.assign(
            final_dest,
            I::binary(BinaryOp::Or, T::I1, O::reg(eq_dest), O::bool(false)),
        );
    }
}

pub(crate) fn emit_vector_compare(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    destination: ValueId,
    operator: &str,
    left: ValueId,
    right: ValueId,
    ty: &Type,
    state: &mut EmissionState,
) {
    let tag = text.fresh_tag();
    let dest_str = format!("v{}", destination.0);
    let left_op = O::reg(format!("v{}", left.0));
    let right_op = O::reg(format!("v{}", right.0));
    let eq_dest = format!("veq{tag}");
    emit_vector_operands_compare(text, module, &eq_dest, ty, left_op, right_op, state);
    if matches!(operator, "NotEqual") {
        text.assign(
            dest_str,
            I::binary(BinaryOp::Xor, T::I1, O::reg(eq_dest), O::bool(true)),
        );
    } else {
        text.assign(
            dest_str,
            I::binary(BinaryOp::Or, T::I1, O::reg(eq_dest), O::bool(false)),
        );
    }
}

fn emit_struct_pointers_compare(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    dest: &str,
    ty: &Type,
    left_ptr: O,
    right_ptr: O,
    state: &mut EmissionState,
) {
    let Some(owner) = struct_owner(module, ty) else {
        text.assign(
            dest.to_string(),
            I::icmp(ICmpCond::Eq, T::Ptr, left_ptr, right_ptr),
        );
        return;
    };
    let fields = class_layout_fields(module, &owner);
    let tag = text.fresh_tag();
    let ptr_eq = format!("structptreq{tag}");
    text.assign(
        ptr_eq.clone(),
        I::icmp(ICmpCond::Eq, T::Ptr, left_ptr.clone(), right_ptr.clone()),
    );
    let mut current_and: Option<String> = None;
    for (idx, field) in fields.into_iter().enumerate() {
        let Some(offset) = field_byte_offset(module, &field.reference) else {
            continue;
        };
        let Some(field_llvm) = llvm_type(&field.ty) else {
            continue;
        };
        let fld_left_ptr = format!("fldl_{idx}_{tag}");
        let fld_right_ptr = format!("fldr_{idx}_{tag}");
        let offset_op = vec![(T::I32, O::uint(u64::from(offset)))];
        text.assign(
            fld_left_ptr.clone(),
            I::gep(T::I8, left_ptr.clone(), offset_op.clone()),
        );
        text.assign(
            fld_right_ptr.clone(),
            I::gep(T::I8, right_ptr.clone(), offset_op),
        );
        let fld_l = format!("fldlv_{idx}_{tag}");
        let fld_r = format!("fldrv_{idx}_{tag}");
        text.assign(
            fld_l.clone(),
            I::load(field_llvm.clone(), O::reg(fld_left_ptr)),
        );
        text.assign(fld_r.clone(), I::load(field_llvm, O::reg(fld_right_ptr)));

        let fld_eq = format!("fldeq_{idx}_{tag}");
        emit_value_compare(
            text,
            module,
            &fld_eq,
            &field.ty,
            O::reg(fld_l),
            O::reg(fld_r),
            state,
        );

        current_and = match current_and {
            None => Some(fld_eq),
            Some(prev) => {
                let next_and = format!("fldand_{idx}_{tag}");
                text.assign(
                    next_and.clone(),
                    I::binary(BinaryOp::And, T::I1, O::reg(prev), O::reg(fld_eq)),
                );
                Some(next_and)
            }
        };
    }
    let fields_eq = current_and.unwrap_or_else(|| {
        let true_val = format!("structtrue{tag}");
        text.assign(
            true_val.clone(),
            I::icmp(ICmpCond::Eq, T::I32, O::int(0), O::int(0)),
        );
        true_val
    });
    text.assign(
        dest.to_string(),
        I::binary(BinaryOp::Or, T::I1, O::reg(ptr_eq), O::reg(fields_eq)),
    );
}

fn emit_vector_operands_compare(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    dest: &str,
    ty: &Type,
    left_fat: O,
    right_fat: O,
    state: &mut EmissionState,
) {
    let tag = text.fresh_tag();
    let fat_ty = vector_ty();

    let len_l = format!("veclenl{tag}");
    let ptr_l = format!("vecptrl{tag}");
    text.assign(
        len_l.clone(),
        I::extract(fat_ty.clone(), left_fat.clone(), 1),
    );
    text.assign(ptr_l.clone(), I::extract(fat_ty.clone(), left_fat, 0));

    let len_r = format!("veclenr{tag}");
    let ptr_r = format!("vecptrr{tag}");
    text.assign(
        len_r.clone(),
        I::extract(fat_ty.clone(), right_fat.clone(), 1),
    );
    text.assign(ptr_r.clone(), I::extract(fat_ty, right_fat, 0));

    let len_eq = format!("vecleneq{tag}");
    text.assign(
        len_eq.clone(),
        I::icmp(ICmpCond::Eq, T::I32, O::reg(len_l), O::reg(len_r)),
    );

    let ptr_eq = format!("vecptreq{tag}");
    text.assign(
        ptr_eq.clone(),
        I::icmp(
            ICmpCond::Eq,
            T::Ptr,
            O::reg(ptr_l.clone()),
            O::reg(ptr_r.clone()),
        ),
    );

    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        text.assign(
            dest.to_string(),
            I::binary(BinaryOp::And, T::I1, O::reg(len_eq), O::reg(ptr_eq)),
        );
        return;
    };

    let count = dimensions.first().copied().unwrap_or(0);
    let child_ty = if dimensions.len() > 1 {
        Type::Vector {
            element: element.clone(),
            dimensions: dimensions[1..].to_vec(),
        }
    } else {
        element.as_ref().clone()
    };
    let elems_eq = if count == 0 {
        let true_val = format!("vectrue{tag}");
        text.assign(
            true_val.clone(),
            I::icmp(ICmpCond::Eq, T::I32, O::int(0), O::int(0)),
        );
        true_val
    } else if count > VECTOR_COMPARE_UNROLL_LIMIT && integer_element_bytes(&child_ty).is_some() {
        emit_integer_memcmp(text, tag, &ptr_l, &ptr_r, count, &child_ty)
    } else if count > VECTOR_COMPARE_UNROLL_LIMIT {
        emit_element_loop(text, module, tag, &child_ty, &ptr_l, &ptr_r, count, state)
    } else {
        emit_unrolled_elements(text, module, tag, &child_ty, &ptr_l, &ptr_r, count, state)
    };

    let elems_match = format!("vecelemmatch{tag}");
    text.assign(
        elems_match.clone(),
        I::binary(
            BinaryOp::And,
            T::I1,
            O::reg(len_eq.clone()),
            O::reg(elems_eq),
        ),
    );

    let same_buf = format!("vecsamebuf{tag}");
    text.assign(
        same_buf.clone(),
        I::binary(BinaryOp::And, T::I1, O::reg(len_eq), O::reg(ptr_eq)),
    );

    text.assign(
        dest.to_string(),
        I::binary(BinaryOp::Or, T::I1, O::reg(same_buf), O::reg(elems_match)),
    );
}

fn emit_value_compare(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    dest: &str,
    ty: &Type,
    left_val: O,
    right_val: O,
    state: &mut EmissionState,
) {
    if let Some(llvm_ty) = llvm_type(ty) {
        if llvm_ty.is_integer() {
            text.assign(
                dest.to_string(),
                I::icmp(ICmpCond::Eq, llvm_ty, left_val, right_val),
            );
            return;
        }
        if llvm_ty == T::I1 {
            text.assign(
                dest.to_string(),
                I::icmp(ICmpCond::Eq, T::I1, left_val, right_val),
            );
            return;
        }
        if llvm_ty.is_floating() {
            text.assign(
                dest.to_string(),
                I::fcmp(FCmpCond::Oeq, llvm_ty, left_val, right_val),
            );
            return;
        }
        if *ty == Type::String {
            let tag = text.fresh_tag();
            let streq = format!("streq{tag}");
            text.assign(streq.clone(), STR_EQ.call([left_val, right_val]));
            text.assign(
                dest.to_string(),
                I::icmp(ICmpCond::Eq, T::I32, O::reg(streq), O::int(1)),
            );
            return;
        }
        if is_struct_type(module, ty) {
            emit_struct_pointers_compare(text, module, dest, ty, left_val, right_val, state);
            return;
        }
        if llvm_ty == vector_ty() {
            emit_vector_operands_compare(text, module, dest, ty, left_val, right_val, state);
            return;
        }
        if llvm_ty == T::Ptr {
            text.assign(
                dest.to_string(),
                I::icmp(ICmpCond::Eq, T::Ptr, left_val, right_val),
            );
        }
    }
}

fn emit_integer_memcmp(
    text: &mut crate::ir::LlvmModule,
    tag: usize,
    left: &str,
    right: &str,
    count: u64,
    element: &Type,
) -> String {
    let bytes = integer_element_bytes(element)
        .unwrap_or_else(|| panic!("{}", bn_catalog::text("llvm", "msg.integer_vector_element")));
    let total = count * bytes;
    let call = format!("vecmem{tag}");
    text.assign(
        call.clone(),
        I::call(
            T::I32,
            "memcmp",
            vec![
                (T::Ptr, O::reg(left)),
                (T::Ptr, O::reg(right)),
                (T::I64, O::uint(total)),
            ],
        ),
    );
    let equal = format!("vecmemeq{tag}");
    text.assign(
        equal.clone(),
        I::icmp(ICmpCond::Eq, T::I32, O::reg(call), O::int(0)),
    );
    equal
}

fn emit_unrolled_elements(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    tag: usize,
    child_ty: &Type,
    left: &str,
    right: &str,
    count: u64,
    state: &mut EmissionState,
) -> String {
    let elem_llvm = llvm_type(child_ty).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.vector_element_llvm_type")
        )
    });
    let mut current_and = None;
    for idx in 0..count {
        let elem_l_ptr = format!("vecl_ptr_{idx}_{tag}");
        let elem_r_ptr = format!("vecr_ptr_{idx}_{tag}");
        let at = vec![(T::I32, O::uint(idx))];
        text.assign(
            elem_l_ptr.clone(),
            I::gep(elem_llvm.clone(), O::reg(left), at.clone()),
        );
        text.assign(
            elem_r_ptr.clone(),
            I::gep(elem_llvm.clone(), O::reg(right), at),
        );
        let elem_l = format!("vecl_v_{idx}_{tag}");
        let elem_r = format!("vecr_v_{idx}_{tag}");
        text.assign(
            elem_l.clone(),
            I::load(elem_llvm.clone(), O::reg(elem_l_ptr)),
        );
        text.assign(
            elem_r.clone(),
            I::load(elem_llvm.clone(), O::reg(elem_r_ptr)),
        );
        let elem_eq = format!("vecelem_eq_{idx}_{tag}");
        emit_value_compare(
            text,
            module,
            &elem_eq,
            child_ty,
            O::reg(elem_l),
            O::reg(elem_r),
            state,
        );
        current_and = match current_and {
            None => Some(elem_eq),
            Some(prev) => {
                let next_and = format!("vecand_{idx}_{tag}");
                text.assign(
                    next_and.clone(),
                    I::binary(BinaryOp::And, T::I1, O::reg(prev), O::reg(elem_eq)),
                );
                Some(next_and)
            }
        };
    }
    current_and.unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.a_positive_element_count")
        )
    })
}

/// One index, one element compare, stop at the first mismatch. The index and
/// the accumulator are `phi` nodes so the body stays one copy.
fn emit_element_loop(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    tag: usize,
    child_ty: &Type,
    left: &str,
    right: &str,
    count: u64,
    state: &mut EmissionState,
) -> String {
    let elem_llvm = llvm_type(child_ty).unwrap_or_else(|| {
        panic!(
            "{}",
            bn_catalog::text("llvm", "msg.vector_element_llvm_type")
        )
    });
    let pre = format!("veccmp.pre{tag}");
    let header = format!("veccmp.h{tag}");
    let body = format!("veccmp.b{tag}");
    let latch = format!("veccmp.l{tag}");
    let exit = format!("veccmp.x{tag}");
    let idx = format!("vecidx{tag}");
    let acc = format!("vecacc{tag}");
    let next = format!("vecnext{tag}");
    let acc_and = format!("vecaccand{tag}");
    text.emit(I::Br { dest: pre.clone() });
    state.control_flow.label(text, pre.clone());
    text.emit(I::Br {
        dest: header.clone(),
    });
    state.control_flow.label(text, header.clone());
    text.assign(
        idx.clone(),
        I::Phi {
            ty: T::I32,
            incoming: vec![(O::int(0), pre), (O::reg(&next), latch.clone())],
        },
    );
    text.assign(
        acc.clone(),
        I::Phi {
            ty: T::I1,
            incoming: vec![
                (O::bool(true), format!("veccmp.pre{tag}")),
                (O::reg(&acc_and), latch.clone()),
            ],
        },
    );
    let in_range = format!("vecin{tag}");
    text.assign(
        in_range.clone(),
        I::icmp(ICmpCond::Ult, T::I32, O::reg(&idx), O::uint(count)),
    );
    let go = format!("vecgo{tag}");
    text.assign(
        go.clone(),
        I::binary(BinaryOp::And, T::I1, O::reg(&acc), O::reg(in_range)),
    );
    text.emit(I::CondBr {
        cond: O::reg(go),
        true_dest: body.clone(),
        false_dest: exit.clone(),
    });
    state.control_flow.label(text, body);
    let at = vec![(T::I32, O::reg(&idx))];
    let elem_l_ptr = format!("vecl_ptr_{tag}");
    let elem_r_ptr = format!("vecr_ptr_{tag}");
    text.assign(
        elem_l_ptr.clone(),
        I::gep(elem_llvm.clone(), O::reg(left), at.clone()),
    );
    text.assign(
        elem_r_ptr.clone(),
        I::gep(elem_llvm.clone(), O::reg(right), at),
    );
    let elem_l = format!("vecl_v_{tag}");
    let elem_r = format!("vecr_v_{tag}");
    text.assign(
        elem_l.clone(),
        I::load(elem_llvm.clone(), O::reg(elem_l_ptr)),
    );
    text.assign(elem_r.clone(), I::load(elem_llvm, O::reg(elem_r_ptr)));
    let elem_eq = format!("vecelem_eq_{tag}");
    emit_value_compare(
        text,
        module,
        &elem_eq,
        child_ty,
        O::reg(elem_l),
        O::reg(elem_r),
        state,
    );
    text.emit(I::Br {
        dest: latch.clone(),
    });
    state.control_flow.label(text, latch);
    text.assign(
        acc_and,
        I::binary(BinaryOp::And, T::I1, O::reg(&acc), O::reg(elem_eq)),
    );
    text.assign(
        next,
        I::binary(BinaryOp::Add, T::I32, O::reg(&idx), O::int(1)),
    );
    text.emit(I::Br { dest: header });
    state.control_flow.label(text, exit);
    acc
}
