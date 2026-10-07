// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Vectors, objects, and members: allocation, indexing with bounds checks,
// member access, release of vectors and objects, and `IS` type tests.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_arguments
)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, FCmpCond, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O,
    LlvmType as T,
};
use crate::layout::{AlternativeLayout, handle_result_ty, typed_llvm, vector_ty};

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

fn free(text: &mut String, pointer: O) {
    text.emit(I::call(T::Void, "free", vec![(T::Ptr, pointer)]));
}

fn memcpy(text: &mut String, target: O, source: O, bytes: O) {
    let args = vec![
        (T::Ptr, target),
        (T::Ptr, source),
        (T::I64, bytes),
        (T::I1, O::bool(false)),
    ];
    text.emit(I::call(T::Void, "llvm.memcpy.p0.p0.i64", args));
}

/// `%{name}0 = { ptr, i32 }` holding `pointer`, then `%{name}` with `length`
/// (`name0` is spelled `{name}0` or `{name}0_` per caller).
fn emit_fat(text: &mut String, first: String, name: String, pointer: O, length: O) {
    let fat = vector_ty();
    text.assign(
        &first,
        I::insert(fat.clone(), O::undef(), T::Ptr, pointer, 0),
    );
    text.assign(name, I::insert(fat, O::reg(first), T::I32, length, 1));
}

pub(crate) fn emit_vector(
    text: &mut String,
    module: &Module,
    destination: ValueId,
    elements: &[ValueId],
    ty: &Type,
    analysis: &LoweringAnalysis<'_>,
) {
    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        unreachable!("validated vector type");
    };
    let stored_type = if dimensions.len() == 1 {
        element.as_ref().clone()
    } else {
        Type::Vector {
            element: element.clone(),
            dimensions: dimensions[1..].to_vec(),
        }
    };
    let elem = typed_llvm(llvm_type(&stored_type).expect("validated vector element"));
    let len = u32::try_from(dimensions[0]).unwrap_or(0);
    let array = T::Array(len as usize, Box::new(elem.clone()));
    let dest = destination.0;
    let data = O::reg(format!("vecdata{dest}"));
    let at = |index: u64| vec![(T::I32, O::int(0)), (T::I32, O::uint(index))];
    text.assign(format!("vecdata{dest}"), I::alloca(array.clone()));
    for (index, element_id) in elements.iter().enumerate() {
        let slot = format!("vecslot{dest}_{index}");
        let gep = I::gep(array.clone(), data.clone(), at(index as u64));
        text.assign(&slot, gep);
        let source_ty = analysis
            .values
            .get(element_id)
            .expect("validated vector element value");
        let operand = if is_struct_type(module, &stored_type) {
            let Type::Named(owner) = &stored_type else {
                unreachable!("validated struct vector element");
            };
            let bytes = class_instance_bytes(module, owner);
            let copy = format!("vecstructcopy{dest}_{index}");
            let buffer = T::Array(
                usize::try_from(bytes).expect("object size"),
                Box::new(T::I8),
            );
            text.assign(&copy, I::alloca(buffer));
            memcpy(text, O::reg(&copy), v(*element_id), O::uint(bytes));
            format!("%{copy}")
        } else {
            coerce_to_type(text, *element_id, source_ty, &stored_type)
        };
        text.emit(I::store(elem.clone(), O::raw(operand), O::reg(slot)));
    }
    text.assign(format!("vecptr{dest}"), I::gep(array, data, at(0)));
    let pointer = O::reg(format!("vecptr{dest}"));
    emit_fat(
        text,
        format!("vecfat{dest}"),
        format!("v{dest}"),
        pointer,
        O::uint(u64::from(len)),
    );
}

pub(crate) fn emit_vector_length(text: &mut String, destination: ValueId, vector: ValueId) {
    text.assign(
        format!("v{}", destination.0),
        I::extract(vector_ty(), v(vector), 1),
    );
}

pub(crate) fn emit_allocate(
    text: &mut String,
    module: &Module,
    destination: ValueId,
    arguments: &[ValueId],
    ty: &Type,
    analysis: &LoweringAnalysis<'_>,
    object_bytes: u64,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    if matches!(ty, Type::Named(name) if name == "FS.File") {
        // `NEW FS.File()` is a file that was never opened (host.md): a runtime
        // handle whose methods, Close aside, return `Error(FS.CLOSED)`.
        let union = handle_result_ty();
        text.assign(format!("fsnewout{dest}"), I::alloca(T::I64));
        text.emit(I::store(T::I64, O::int(0), r("fsnewout")));
        let call = I::call(T::I32, "bn_rt_file_new", vec![(T::Ptr, r("fsnewout"))]);
        text.assign(format!("fsnewrc{dest}"), call);
        text.assign(format!("fsnewhandle{dest}"), I::load(T::I64, r("fsnewout")));
        let head = I::insert(union.clone(), O::undef(), T::I1, O::bool(false), 0);
        text.assign(format!("fsnew0{dest}"), head);
        let null = I::insert(union.clone(), r("fsnew0"), T::Ptr, O::null(), 1);
        text.assign(format!("fsnew1{dest}"), null);
        let handle = I::insert(union, r("fsnew1"), T::I64, r("fsnewhandle"), 2);
        text.assign(format!("v{dest}"), handle);
        return;
    }
    if is_bndata_dataframe_type(module, ty) {
        text.assign(format!("dfout{dest}"), I::alloca(T::I64));
        let args = vec![
            (T::Ptr, O::null()),
            (T::I32, O::int(0)),
            (T::Ptr, r("dfout")),
        ];
        let call = I::call(T::I32, "bn_rt_dataframe_create", args);
        text.assign(format!("dfrc{dest}"), call);
        text.assign(format!("dfhandle{dest}"), I::load(T::I64, r("dfout")));
        let pointer = I::cast(CastOp::IntToPtr, T::I64, r("dfhandle"), T::Ptr);
        text.assign(format!("v{dest}"), pointer);
        return;
    }
    if let Some(kind) = bnlog_resource_kind(module, ty) {
        let symbol = if kind == "Fields" {
            "bn_rt_log_fields_create"
        } else {
            "bn_rt_log_logger_create"
        };
        text.assign(format!("loghandle{dest}"), I::call(T::I64, symbol, vec![]));
        let pointer = I::cast(CastOp::IntToPtr, T::I64, r("loghandle"), T::Ptr);
        text.assign(format!("v{dest}"), pointer);
        text.emit(I::store(T::I64, r("loghandle"), r("logowned")));
        return;
    }
    if let Type::Pointer { element, .. } = ty {
        let elem_ty = llvm_type(element).expect("validated allocate element");
        let elem_bytes: u64 = match elem_ty {
            "i8" => 1,
            "i16" => 2,
            "i32" | "float" => 4,
            "i64" | "double" | "ptr" => 8,
            _ => 4,
        };
        let len_op = if let Some(len_value) = arguments.first().copied() {
            let len_ty = analysis
                .values
                .get(&len_value)
                .expect("validated allocate length type");
            coerce_to_type(text, len_value, len_ty, &Type::Integer(IntegerType::Int32))
        } else {
            "1".to_string()
        };
        let length = O::raw(len_op);
        let wide = I::cast(CastOp::ZExt, T::I32, length.clone(), T::I64);
        text.assign(format!("alloclen{dest}"), wide);
        let bytes = I::binary(BinaryOp::Mul, T::I64, r("alloclen"), O::uint(elem_bytes));
        text.assign(format!("allocbytes{dest}"), bytes);
        // Region header (core id at +8) precedes the elements so the region
        // is counted by the ARC core as an object is.
        let header = O::uint(REGION_HEADER_BYTES);
        let all = I::binary(BinaryOp::Add, T::I64, r("allocbytes"), header.clone());
        text.assign(format!("allocall{dest}"), all);
        let args = vec![(T::I64, O::int(1)), (T::I64, r("allocall"))];
        text.assign(format!("allocbase{dest}"), I::call(T::Ptr, "calloc", args));
        let elements = I::gep(T::I8, r("allocbase"), vec![(T::I64, header)]);
        text.assign(format!("allocptr{dest}"), elements);
        emit_fat(
            text,
            format!("allocfat{dest}"),
            format!("v{dest}"),
            r("allocptr"),
            length,
        );
        return;
    }
    let bytes = object_bytes.max(u64::from(OBJECT_HEADER_BYTES) + 4);
    let args = vec![(T::I64, O::int(1)), (T::I64, O::uint(bytes))];
    text.assign(format!("v{dest}"), I::call(T::Ptr, "calloc", args));
}

pub(crate) fn emit_store_object_class(text: &mut String, object: ValueId, class_global: &str) {
    text.emit(I::store(T::Ptr, O::raw(class_global), v(object)));
}

/// Stores `value` into the field at `field_offset` of `object`. Under
/// explicit ownership the field's previous content goes to `previous`, the
/// stored value is owned (the IR retained it), and a weak field stores the
/// object's core id.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_set_member(
    text: &mut String,
    object: ValueId,
    field_offset: u32,
    value: ValueId,
    value_ty: &Type,
    field_ty: &Type,
    previous: Option<ValueId>,
    weak: bool,
    state: &mut EmissionState,
) {
    let llvm_ty = typed_llvm(llvm_type(field_ty).expect("validated member type"));
    let mut value_op = coerce_to_type(text, value, value_ty, field_ty);
    let tag = state.continuation_count;
    let f = |name: &str| O::reg(format!("field{name}{tag}"));
    if let Type::Vector { element, .. } = field_ty {
        state.continuation_count += 1;
        let element_llvm = llvm_type(element).expect("validated vector field element");
        let fat = O::raw(&value_op);
        text.assign(
            format!("fieldsrc{tag}"),
            I::extract(vector_ty(), fat.clone(), 0),
        );
        text.assign(format!("fieldlen{tag}"), I::extract(vector_ty(), fat, 1));
        let wide = I::cast(CastOp::ZExt, T::I32, f("len"), T::I64);
        text.assign(format!("fieldlen64_{tag}"), wide);
        let element_bytes: u64 = match element_llvm {
            "i1" | "i8" => 1,
            "i16" => 2,
            "i32" | "float" => 4,
            "i64" | "double" | "ptr" => 8,
            _ => unreachable!("validated scalar vector field element"),
        };
        let bytes = I::binary(BinaryOp::Mul, T::I64, f("len64_"), O::uint(element_bytes));
        text.assign(format!("fieldbytes{tag}"), bytes);
        let copy = I::call(T::Ptr, "malloc", vec![(T::I64, f("bytes"))]);
        text.assign(format!("fieldcopy{tag}"), copy);
        memcpy(text, f("copy"), f("src"), f("bytes"));
        emit_fat(
            text,
            format!("fieldfat0_{tag}"),
            format!("fieldfat{tag}"),
            f("copy"),
            f("len"),
        );
        value_op = format!("%fieldfat{tag}");
    }
    let member = O::reg(format!("mbrptr{}", value.0));
    let offset = vec![(T::I32, O::uint(u64::from(field_offset)))];
    text.assign(
        format!("mbrptr{}", value.0),
        I::gep(T::I8, v(object), offset),
    );
    if matches!(field_ty, Type::Vector { .. }) {
        text.assign(
            format!("fieldoldfat{tag}"),
            I::load(vector_ty(), member.clone()),
        );
        let old = I::extract(vector_ty(), f("oldfat"), 0);
        text.assign(format!("fieldoldptr{tag}"), old);
        free(text, f("oldptr"));
    }
    if let Some(previous) = previous {
        text.assign(
            format!("v{}", previous.0),
            I::load(llvm_ty.clone(), member.clone()),
        );
    }
    if weak {
        value_op = arc_ops::weak_store_operand(text, &value_op, state);
    }
    text.emit(I::store(llvm_ty, O::raw(value_op), member));
}

pub(crate) fn emit_member(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    object: ValueId,
    field_offset: u32,
    field_ty: &Type,
    weak: bool,
    state: &mut EmissionState,
) {
    let llvm_ty = typed_llvm(llvm_type(field_ty).expect("validated member type"));
    let dest = destination.0;
    let null = I::icmp(ICmpCond::Eq, T::Ptr, v(object), O::null());
    text.assign(format!("membernull{dest}"), null);
    let live = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &format!("%membernull{dest}"),
        live,
        bn_diag::DiagId::USE_AFTER_RELEASE,
        vec![("detail", Fact::Text("binding was released".into()))],
    );
    let member = O::reg(format!("mbrptr{dest}"));
    let offset = vec![(T::I32, O::uint(u64::from(field_offset)))];
    text.assign(format!("mbrptr{dest}"), I::gep(T::I8, v(object), offset));
    if weak {
        // A weak field stores the core id (see `arc_ops::weak_read`).
        text.assign(format!("weakfield{dest}"), I::load(T::Ptr, member));
        arc_ops::weak_read(text, &format!("v{dest}"), &format!("%weakfield{dest}"));
    } else {
        text.assign(format!("v{dest}"), I::load(llvm_ty, member));
    }
}

pub(crate) fn emit_delete(text: &mut String, module: &Module, value: ValueId, ty: &Type) {
    let n = value.0;
    if llvm_type(ty) == Some("{ ptr, i32 }") {
        text.assign(format!("delptr{n}"), I::extract(vector_ty(), v(value), 0));
        free(text, O::reg(format!("delptr{n}")));
        return;
    }
    let owner = match ty {
        Type::Named(name) | Type::ImportedNamed { name, .. } if llvm_type(ty) == Some("ptr") => {
            Some(name.as_str())
        }
        _ => None,
    };
    for (index, offset) in owner
        .map(|owner| vector_field_offsets(module, owner))
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        let r = |name: &str| O::reg(format!("delfield{name}{n}_{index}"));
        let at = vec![(T::I32, O::uint(u64::from(offset)))];
        text.assign(
            format!("delfieldptr{n}_{index}"),
            I::gep(T::I8, v(value), at),
        );
        text.assign(
            format!("delfield{n}_{index}"),
            I::load(vector_ty(), r("ptr")),
        );
        let data = I::extract(vector_ty(), r(""), 0);
        text.assign(format!("delfielddata{n}_{index}"), data);
        free(text, r("data"));
    }
    free(text, v(value));
}

pub(crate) fn emit_vector_index(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    object: ValueId,
    index: ValueId,
    index_ty: &Type,
    ty: &Type,
    context: &'static str,
    state: &mut EmissionState,
) {
    let elem_ty = typed_llvm(llvm_type(ty).expect("validated index element"));
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("vec{name}{dest}"));
    let ok = take_continuation(block_id, state);
    let index_op = coerce_to_type(text, index, index_ty, &Type::Integer(IntegerType::Int32));
    let at = O::raw(&index_op);
    text.assign(
        format!("vecptr{dest}"),
        I::extract(vector_ty(), v(object), 0),
    );
    text.assign(
        format!("veclen{dest}"),
        I::extract(vector_ty(), v(object), 1),
    );
    let negative = I::icmp(ICmpCond::Slt, T::I32, at.clone(), O::int(0));
    text.assign(format!("vecneg{dest}"), negative);
    text.assign(
        format!("vecoob{dest}"),
        I::icmp(ICmpCond::Uge, T::I32, at.clone(), r("len")),
    );
    let bad = I::binary(BinaryOp::Or, T::I1, r("neg"), r("oob"));
    text.assign(format!("vecbad{dest}"), bad);
    emit_index_trap(
        text,
        block_id,
        state,
        &format!("%vecbad{dest}"),
        ok,
        &index_op,
        &format!("%veclen{dest}"),
        context,
    );
    let slot = I::gep(elem_ty.clone(), r("ptr"), vec![(T::I32, at)]);
    text.assign(format!("vecslot{dest}"), slot);
    text.assign(format!("v{dest}"), I::load(elem_ty, r("slot")));
    state.needs_numeric_overflow_trap = true;
}

/// `%v{dest} = xor i1 <flag>, true`: the negation of a flag in slot 0.
fn emit_not_flag(text: &mut String, destination: ValueId, flag: &str, aggregate: T, left: ValueId) {
    let dest = destination.0;
    text.assign(format!("{flag}{dest}"), I::extract(aggregate, v(left), 0));
    let flag = O::reg(format!("{flag}{dest}"));
    let not = I::binary(BinaryOp::Xor, T::I1, flag, O::bool(true));
    text.assign(format!("v{dest}"), not);
}

/// `%v{dest} = or i1 false, <bit>`: a constant `IS` result.
fn emit_is_constant(text: &mut String, destination: ValueId, bit: O) {
    let inst = I::binary(BinaryOp::Or, T::I1, O::bool(false), bit);
    text.assign(format!("v{}", destination.0), inst);
}

/// `IS`, by the layout of the tested value (`AlternativeLayout`). Returns
/// false for a form without an `IS` lowering, which the caller refuses with a
/// support diagnostic rather than fold to a constant.
pub(crate) fn emit_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
) -> bool {
    let test_name = is_test_name(right_ty);
    let Type::Alternative(members) = left_ty else {
        return emit_static_is(text, destination, left, left_ty, right_ty, test_name);
    };
    match AlternativeLayout::of(members) {
        Some(AlternativeLayout::General) => {
            general_alternative::emit_is(text, destination, left, members, test_name, right_ty);
            true
        }
        Some(AlternativeLayout::Sentinel) => {
            emit_sentinel_pointer_is(text, destination, left, left_ty, right_ty, test_name)
        }
        Some(
            AlternativeLayout::Status
            | AlternativeLayout::StatusPointer
            | AlternativeLayout::StatusEndpoint,
        ) => emit_status_is(text, destination, left, left_ty, right_ty, test_name),
        Some(AlternativeLayout::Region) | None => false,
    }
}

/// `IS` on a sentinel pointer: `STRING OR EOF` (`@.bn_eof`) or
/// `Class OR NULL` (null).
fn emit_sentinel_pointer_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    if emit_string_or_eof_is(text, destination, left, left_ty, test_name) {
        return true;
    }
    let Type::Alternative(members) = left_ty else {
        unreachable!("sentinel layout of an alternative");
    };
    let cond = if test_name == "NULL" || matches!(right_ty, Type::Null) {
        ICmpCond::Eq
    } else if members
        .iter()
        .any(|ty| !matches!(ty, Type::Null) && alternative_is(ty, test_name))
    {
        // The class member is the non-null pointer.
        ICmpCond::Ne
    } else {
        return false;
    };
    let check = I::icmp(cond, T::Ptr, v(left), O::null());
    text.assign(format!("v{}", destination.0), check);
    true
}

/// `IS` on a status aggregate (`T OR Error`, with `NA` or `EOF` as a
/// sentinel pointer): the error flag, the sentinel, or the value side.
fn emit_status_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    if emit_sentinel_is(text, destination, left, left_ty, right_ty, test_name)
        || emit_integer_error_union_is(text, destination, left, left_ty, test_name)
    {
        return true;
    }
    let Type::Alternative(members) = left_ty else {
        unreachable!("status layout of an alternative");
    };
    let aggregate = typed_llvm(llvm_type(left_ty).expect("status layout"));
    let own = format!("v{}", destination.0);
    if test_name == "Error" {
        text.assign(own, I::extract(aggregate, v(left), 0));
        return true;
    }
    if aggregate == handle_result_ty()
        && (test_name == "EOF" || matches!(right_ty, Type::EndOfFile))
    {
        emit_eof_is(text, destination, left);
        return true;
    }
    // `T OR Error`: `IS T` means "not an error".
    let value_ty = match members.as_slice() {
        [first, second] if is_error_type(first) => Some(second),
        [first, second] if is_error_type(second) => Some(first),
        _ => None,
    };
    if value_ty.is_some_and(|value_ty| alternative_is(value_ty, test_name)) {
        emit_not_flag(text, destination, "iserror", aggregate, left);
        return true;
    }
    false
}

/// `IS EOF` on a `{ i1, ptr, i64 }`: its pointer is the `@.bn_eof` marker.
fn emit_eof_is(text: &mut String, destination: ValueId, left: ValueId) {
    let dest = destination.0;
    let marker = T::Array(4, Box::new(T::I8));
    let zero = vec![(T::I64, O::int(0)), (T::I64, O::int(0))];
    let eof = I::gep(marker, O::global(".bn_eof"), zero);
    text.assign(format!("eofptr{dest}"), eof);
    let value = I::extract(handle_result_ty(), v(left), 1);
    text.assign(format!("eofvalue{dest}"), value);
    let (value, eof) = (
        O::reg(format!("eofvalue{dest}")),
        O::reg(format!("eofptr{dest}")),
    );
    text.assign(
        format!("v{dest}"),
        I::icmp(ICmpCond::Eq, T::Ptr, value, eof),
    );
}

/// `IS` on a value that is not an alternative: its static type decides,
/// except for the runtime facts a single type still carries (a null object,
/// an `Error` flag, a float that is `NAN` or infinite).
fn emit_static_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    if emit_string_or_eof_is(text, destination, left, left_ty, test_name) {
        return true;
    }
    let own = format!("v{}", destination.0);
    let left_llvm = llvm_type(left_ty);
    if left_llvm == Some("ptr") && (test_name == "NULL" || matches!(right_ty, Type::Null)) {
        text.assign(own, I::icmp(ICmpCond::Eq, T::Ptr, v(left), O::null()));
        return true;
    }
    if let Some(aggregate) = left_llvm
        .filter(|ty| matches!(*ty, "{ i1, ptr }" | "{ i1, ptr, i32 }" | "{ i1, ptr, i64 }"))
        && test_name == "Error"
    {
        text.assign(own, I::extract(typed_llvm(aggregate), v(left), 0));
        return true;
    }
    if left_llvm == Some("{ i1, ptr, i64 }")
        && (test_name == "EOF" || matches!(right_ty, Type::EndOfFile))
    {
        emit_eof_is(text, destination, left);
        return true;
    }
    let optional = T::struct_of([T::I1, T::Double]);
    if left_llvm == Some("{ i1, double }") && test_name == "NA" {
        text.assign(own, I::extract(optional, v(left), 0));
        return true;
    }
    if left_llvm == Some("{ i1, double }") && matches!(test_name, "FLOAT" | "FLOAT64") {
        emit_not_flag(text, destination, "isna", optional, left);
        return true;
    }
    let float_llvm = match left_ty {
        Type::Float(FloatType::Float32) => Some(T::Float),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some(T::Double),
        _ => None,
    };
    if let Some(llvm_ty) = float_llvm {
        let check = match test_name {
            "NAN" => Some((FCmpCond::Uno, "0.0")),
            "INF" => Some((FCmpCond::Oeq, "0x7FF0000000000000")),
            "-INF" => Some((FCmpCond::Oeq, "0xFFF0000000000000")),
            _ => None,
        };
        if let Some((cond, constant)) = check {
            text.assign(own, I::fcmp(cond, llvm_ty, v(left), O::raw(constant)));
            return true;
        }
    }
    // A value that is not an alternative has its static type: `IS` of that
    // type holds (a narrowed `FS.File` IS FS.File).
    let constant = if alternative_is(left_ty, test_name) {
        O::bool(true)
    } else {
        O::int(i64::from(
            test_name == "NA" && matches!(left_ty, Type::NotAvailable),
        ))
    };
    emit_is_constant(text, destination, constant);
    true
}

fn emit_integer_error_union_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    test_name: &str,
) -> bool {
    if integer_union_payload(left_ty).is_none()
        || !matches!(
            test_name,
            "INTEGER"
                | "INT8"
                | "INT16"
                | "INT32"
                | "INT64"
                | "BYTE"
                | "UINT16"
                | "UINT32"
                | "UINT64"
        )
    {
        return false;
    }
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("union{name}{dest}"));
    text.assign(
        format!("unioniserror{dest}"),
        I::extract(handle_result_ty(), v(left), 0),
    );
    let not_error = I::binary(BinaryOp::Xor, T::I1, r("iserror"), O::bool(true));
    text.assign(format!("unionnoterror{dest}"), not_error);
    if matches!(left_ty, Type::Alternative(alternatives) if alternatives.iter().any(|ty| matches!(ty, Type::EndOfFile)))
    {
        text.assign(
            format!("unionisptr{dest}"),
            I::extract(handle_result_ty(), v(left), 1),
        );
        let not_eof = I::icmp(ICmpCond::Ne, T::Ptr, r("isptr"), O::global(".bn_eof"));
        text.assign(format!("unionnoteof{dest}"), not_eof);
        let both = I::binary(BinaryOp::And, T::I1, r("noterror"), r("noteof"));
        text.assign(format!("v{dest}"), both);
    } else {
        emit_is_constant(text, destination, r("noterror"));
    }
    true
}

fn is_test_name(ty: &Type) -> &str {
    match ty {
        Type::TypeName(name) | Type::Named(name) => name,
        Type::NotAvailable => "NA",
        Type::Null => "NULL",
        _ => "",
    }
}

/// `INPUT` uses a string pointer or the unique EOF sentinel. Its erased
/// pointer representation still requires a dynamic tag test. A STRING whose
/// contents are `EOF` remains distinct because emitted string globals retain
/// address significance.
fn emit_string_or_eof_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    test_name: &str,
) -> bool {
    // A plain `STRING` slot (the `INPUT` target typed by its value) or a
    // `STRING OR EOF` value (a parameter, return, field): both are the
    // `@.bn_eof` sentinel pointer.
    let string_or_eof = matches!(left_ty, Type::Alternative(members)
        if members.len() == 2 && members.contains(&Type::String) && members.contains(&Type::EndOfFile));
    if (*left_ty != Type::String && !string_or_eof) || !matches!(test_name, "EOF" | "STRING") {
        return false;
    }
    let cond = if test_name == "EOF" {
        ICmpCond::Eq
    } else {
        ICmpCond::Ne
    };
    let compare = I::icmp(cond, T::Ptr, v(left), O::global(".bn_eof"));
    text.assign(format!("v{}", destination.0), compare);
    true
}

pub(crate) fn emit_optional_float_default(text: &mut String, destination: ValueId) {
    let dest = destination.0;
    let optional = T::struct_of([T::I1, T::Double]);
    let flag = I::insert(optional.clone(), O::undef(), T::I1, O::bool(false), 0);
    text.assign(format!("na{dest}"), flag);
    let na = O::reg(format!("na{dest}"));
    let value = I::insert(optional, na, T::Double, O::raw("0.0"), 1);
    text.assign(format!("v{dest}"), value);
}

pub(crate) fn extract_optional_float(text: &mut String, destination: ValueId, value: ValueId) {
    let optional = T::struct_of([T::I1, T::Double]);
    text.assign(
        format!("v{}", destination.0),
        I::extract(optional, v(value), 1),
    );
}

/// `llvm.memmove`: a vector may be stored into the slot it came from.
pub(crate) const MEMMOVE: &str = "void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)";

/// The storage array of a one-dimensional fixed vector, `[count x element]`,
/// and the count. A nested vector is an array of `{ ptr, i32 }` rows, which
/// a flat copy would not duplicate (pending, bucket typed-llvm-emitter).
pub(crate) fn fixed_vector_array(ty: &Type) -> Option<(String, u64)> {
    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        return None;
    };
    let [count] = dimensions.as_slice() else {
        return None;
    };
    (*count != u64::MAX).then(|| Some((format!("[{count} x {}]", llvm_type(element)?), *count)))?
}

/// Copies the elements of the array `array` from `source` to `target`.
fn emit_copy_elements(text: &mut String, array: &str, target: O, source: O) {
    let tag = text.len();
    let size = I::gep(typed_llvm(array), O::null(), vec![(T::I32, O::int(1))]);
    text.assign(format!("vsize{tag}"), size);
    let size = O::reg(format!("vsize{tag}"));
    text.assign(
        format!("vbytes{tag}"),
        I::cast(CastOp::PtrToInt, T::Ptr, size, T::I64),
    );
    let bytes = O::reg(format!("vbytes{tag}"));
    let args = vec![
        (T::Ptr, target),
        (T::Ptr, source),
        (T::I64, bytes),
        (T::I1, O::bool(false)),
    ];
    text.emit(I::call(T::Void, "llvm.memmove.p0.p0.i64", args));
}

/// The elements a vector slot owns: a fixed vector is a value (`0.6.md`,
/// "value / copy semantics"), so a slot never points at another binding's
/// elements. `vprev` keeps a replaced content alive for its release when
/// the elements hold references.
pub(crate) fn emit_vector_storage(text: &mut String, slot: usize, ty: &Type, references: bool) {
    let Some((array, count)) = fixed_vector_array(ty) else {
        return;
    };
    let array = typed_llvm(&array);
    let storage = O::reg(format!("vstore{slot}"));
    text.assign(format!("vstore{slot}"), I::alloca(array.clone()));
    text.emit(I::store(
        array.clone(),
        O::zero_initializer(),
        storage.clone(),
    ));
    emit_fat(
        text,
        format!("vinit{slot}"),
        format!("vinitlen{slot}"),
        storage,
        O::uint(count),
    );
    let fat = O::reg(format!("vinitlen{slot}"));
    text.emit(I::store(vector_ty(), fat, O::reg(format!("s{slot}"))));
    if references {
        text.assign(format!("vprev{slot}"), I::alloca(array));
    }
}

/// Before a store replaces a vector whose elements hold references: moves
/// the old elements from `storage` to `previous` and points `binding` there,
/// so the `previous` load the IR releases reads the old content.
pub(crate) fn emit_vector_keep_previous(
    text: &mut String,
    (storage, previous, binding): (&str, &str, &str),
    ty: &Type,
) {
    if let Some((array, _)) = fixed_vector_array(ty) {
        emit_copy_elements(text, &array, O::raw(previous), O::raw(storage));
        let tag = text.len();
        text.assign(format!("vpold{tag}"), I::load(vector_ty(), O::raw(binding)));
        let old = O::reg(format!("vpold{tag}"));
        let moved = I::insert(vector_ty(), old, T::Ptr, O::raw(previous), 0);
        text.assign(format!("vpfat{tag}"), moved);
        let fat = O::reg(format!("vpfat{tag}"));
        text.emit(I::store(vector_ty(), fat, O::raw(binding)));
    }
}

/// Copies the elements of `value` (a `{ ptr, i32 }`) into `storage`; the
/// result points there.
pub(crate) fn emit_vector_copy(
    text: &mut String,
    storage: &str,
    value: &str,
    ty: &Type,
) -> Option<String> {
    let (array, _) = fixed_vector_array(ty)?;
    let tag = text.len();
    text.assign(
        format!("vcsrc{tag}"),
        I::extract(vector_ty(), O::raw(value), 0),
    );
    let source = O::reg(format!("vcsrc{tag}"));
    emit_copy_elements(text, &array, O::raw(storage), source);
    let moved = I::insert(vector_ty(), O::raw(value), T::Ptr, O::raw(storage), 0);
    text.assign(format!("vcfat{tag}"), moved);
    Some(format!("%vcfat{tag}"))
}

/// The fixed vector a value of `ty` carries out of a function: `ty` itself,
/// or the vector member of a general alternative.
pub(crate) fn returned_vector(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Alternative(_) => general_alternative(ty)?
            .iter()
            .find(|member| fixed_vector_array(member).is_some()),
        _ => fixed_vector_array(ty).map(|_| ty),
    }
}

/// Moves a returned vector (of `ty`) out of storage about to die into
/// `buffer`, as register `out`: never a pointer into the callee's frame. A
/// general alternative moves only when it holds its vector member; the
/// `select` keeps the copy's source readable otherwise.
pub(crate) fn emit_vector_relocate(
    text: &mut String,
    value: &str,
    ty: &Type,
    buffer: &str,
    out: &str,
) -> Option<()> {
    let vector = returned_vector(ty)?;
    if vector == ty {
        let moved = emit_vector_copy(text, buffer, value, ty)?;
        let fat = I::insert(vector_ty(), O::raw(moved), T::Ptr, O::raw(buffer), 0);
        text.assign(out, fat);
        return Some(());
    }
    let (array, _) = fixed_vector_array(vector)?;
    let code = bn_types::alternatives::member_code(vector)?;
    let tag = text.len();
    let r = |name: &str| O::reg(format!("vr{name}{tag}"));
    let layout = typed_llvm(GENERAL_LAYOUT);
    text.assign(
        format!("vrtag{tag}"),
        I::extract(layout.clone(), O::raw(value), 0),
    );
    let holds = I::icmp(ICmpCond::Eq, T::I32, r("tag"), O::raw(code.to_string()));
    text.assign(format!("vris{tag}"), holds);
    text.assign(
        format!("vrptr{tag}"),
        I::extract(layout.clone(), O::raw(value), 1),
    );
    let source = I::select(r("is"), T::Ptr, r("ptr"), O::raw(buffer));
    text.assign(format!("vrsrc{tag}"), source);
    emit_copy_elements(text, &array, O::raw(buffer), r("src"));
    let target = I::select(r("is"), T::Ptr, O::raw(buffer), r("ptr"));
    text.assign(format!("vrnew{tag}"), target);
    text.assign(out, I::insert(layout, O::raw(value), T::Ptr, r("new"), 1));
    Some(())
}

#[cfg(test)]
mod tests {
    use super::emit_is;
    use bn_ir::ValueId;
    use bn_types::{IntegerType, Type};

    #[test]
    fn eof_type_name_uses_the_eof_marker_for_alternative_values() {
        let mut llvm = String::new();
        assert!(emit_is(
            &mut llvm,
            ValueId(2),
            ValueId(1),
            &Type::Alternative(vec![
                Type::Integer(IntegerType::Int32),
                Type::EndOfFile,
                Type::Named("Error".into()),
            ]),
            &Type::TypeName("EOF".into()),
        ));
        assert!(llvm.contains("icmp eq ptr"));
        assert!(llvm.contains("@.bn_eof"));
    }
}
