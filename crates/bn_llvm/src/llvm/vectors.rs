// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Vectors, objects, and members: allocation, indexing with bounds checks,
// member access, and release of vectors and objects (`IS` tests: is_emission.rs).
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_arguments
)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::{AlternativeLayout, handle_result_ty, vector_ty};
#[path = "nested_vectors.rs"]
mod nested_vectors;
pub(crate) use nested_vectors::nest;
#[path = "is_emission.rs"]
mod is_emission;
pub(crate) use is_emission::emit_is;

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

pub(crate) fn free(text: &mut crate::ir::LlvmModule, pointer: O) {
    text.emit(I::call(T::Void, "free", vec![(T::Ptr, pointer)]));
}

pub(crate) fn memcpy(text: &mut crate::ir::LlvmModule, target: O, source: O, bytes: O) {
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
pub(crate) fn emit_fat(
    text: &mut crate::ir::LlvmModule,
    first: String,
    name: String,
    pointer: O,
    length: O,
) {
    let fat = vector_ty();
    text.assign(
        &first,
        I::insert(fat.clone(), O::undef(), T::Ptr, pointer, 0),
    );
    text.assign(name, I::insert(fat, O::reg(first), T::I32, length, 1));
}

pub(crate) fn emit_vector(
    text: &mut crate::ir::LlvmModule,
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
    let elem = llvm_type(&stored_type).expect("validated vector element");
    let len = u32::try_from(dimensions[0]).unwrap_or(0);
    let array = T::Array(len as usize, Box::new(elem.clone()));
    let dest = destination.0;
    let data = O::reg(format!("vecdata{dest}"));
    let at = |index: u64| vec![(T::I32, O::int(0)), (T::I32, O::uint(index))];
    for (index, element_id) in elements.iter().enumerate() {
        let slot = format!("vecslot{dest}_{index}");
        let gep = I::gep(array.clone(), data.clone(), at(index as u64));
        text.assign(&slot, gep);
        let source_ty = analysis
            .values
            .get(element_id)
            .expect("validated vector element value");
        let operand = if let Some(owner) = struct_owner(module, &stored_type) {
            let copy = format!("vecstructcopy{dest}_{index}");
            let copy_witness = arc_runtime::struct_copy_witness_name(&owner);
            text.emit(I::call(
                T::Void,
                &copy_witness,
                vec![(T::Ptr, O::reg(&copy)), (T::Ptr, v(*element_id))],
            ));
            O::reg(&copy)
        } else {
            coerce_to_type(text, *element_id, source_ty, &stored_type)
        };
        text.emit(I::store(elem.clone(), operand, O::reg(slot)));
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

pub(crate) fn emit_vector_length(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    vector: ValueId,
    analysis: &LoweringAnalysis<'_>,
) {
    let ty = analysis
        .values
        .get(&vector)
        .and_then(llvm_type)
        .unwrap_or(crate::layout::vector_ty());
    text.assign(format!("v{}", destination.0), I::extract(ty, v(vector), 1));
}

pub(crate) fn emit_allocate(
    text: &mut crate::ir::LlvmModule,
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
    if is_bnweb_resource_type(module, ty) {
        let Type::ImportedNamed { name, .. } = ty else { unreachable!() };
        let (symbol, args) = match name.as_str() {
            "Response" => ("bn_web_response_create", vec![]),
            "Request" => ("bn_web_request_create", vec![]),
            "Server" => ("bn_web_server_create", vec![]),
            "CookieJar" => ("bn_web_cookie_jar_create", vec![]),
            "SessionStore" => ("bn_web_session_store_create", vec![]),
            "Scraper" => {
                let html_arg = if let Some(arg) = arguments.first() {
                    (T::Ptr, O::reg(format!("v{}", arg.0)))
                } else {
                    (T::Ptr, O::null())
                };
                ("bn_web_scraper_create", vec![html_arg])
            }
            "ACL" => ("bn_web_acl_create", vec![]),
            "Client" => ("bn_web_client_create", vec![]),
            _ => unreachable!("unhandled BNWeb resource allocation"),
        };
        let call = I::call(T::I64, symbol, args);
        text.assign(format!("webh{dest}"), call);
        let pointer = I::cast(CastOp::IntToPtr, T::I64, O::reg(format!("webh{dest}")), T::Ptr);
        text.assign(format!("v{dest}"), pointer);
        return;
    }
    if let Type::Pointer { element, .. } = ty {
        let elem_ty = llvm_type(element).expect("validated allocate element");
        let elem_bytes: u64 = match elem_ty {
            T::I8 => 1,
            T::I16 => 2,
            T::I32 | T::Float => 4,
            T::I64 | T::Double | T::Ptr => 8,
            _ => 4,
        };
        let len_op = if let Some(len_value) = arguments.first().copied() {
            let len_ty = analysis
                .values
                .get(&len_value)
                .expect("validated allocate length type");
            coerce_to_type(text, len_value, len_ty, &Type::Integer(IntegerType::Int32))
        } else {
            O::int(1)
        };
        let length = len_op;
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

pub(crate) fn emit_store_object_class(
    text: &mut crate::ir::LlvmModule,
    object: ValueId,
    class_global: &str,
) {
    text.emit(I::store(T::Ptr, O::global(class_global), v(object)));
}

/// Stores `value` into the field at `field_offset` of `object`. Under
/// Deep-copies a vector value into `dest_ptr`, releasing the previous buffer held in `dest_ptr`.
pub(crate) fn emit_vector_field_store(
    text: &mut crate::ir::LlvmModule,
    dest_ptr: O,
    source_val: O,
    field_ty: &Type,
    state: &mut EmissionState,
) {
    let tag = state.continuation_count;
    state.continuation_count += 1;
    let Type::Vector { element, .. } = field_ty else {
        unreachable!("vector field store on non-vector type")
    };
    let element_llvm = llvm_type(element).expect("validated vector field element");
    let fat = source_val;
    text.assign(
        format!("vcopy_src_{tag}"),
        I::extract(vector_ty(), fat.clone(), 0),
    );
    text.assign(format!("vcopy_len_{tag}"), I::extract(vector_ty(), fat, 1));
    let wide = I::cast(
        CastOp::ZExt,
        T::I32,
        O::reg(format!("vcopy_len_{tag}")),
        T::I64,
    );
    text.assign(format!("vcopy_len64_{tag}"), wide);
    let element_bytes: u64 = match element_llvm {
        T::I1 | T::I8 => 1,
        T::I16 => 2,
        T::I32 | T::Float => 4,
        T::I64 | T::Double | T::Ptr => 8,
        _ => unreachable!("validated scalar vector field element"),
    };
    let bytes = I::binary(
        BinaryOp::Mul,
        T::I64,
        O::reg(format!("vcopy_len64_{tag}")),
        O::uint(element_bytes),
    );
    text.assign(format!("vcopy_bytes_{tag}"), bytes);
    let copy = I::call(
        T::Ptr,
        "malloc",
        vec![(T::I64, O::reg(format!("vcopy_bytes_{tag}")))],
    );
    text.assign(format!("vcopy_buf_{tag}"), copy);
    memcpy(
        text,
        O::reg(format!("vcopy_buf_{tag}")),
        O::reg(format!("vcopy_src_{tag}")),
        O::reg(format!("vcopy_bytes_{tag}")),
    );
    text.assign(
        format!("vcopy_old_{tag}"),
        I::load(vector_ty(), dest_ptr.clone()),
    );
    let old = I::extract(vector_ty(), O::reg(format!("vcopy_old_{tag}")), 0);
    text.assign(format!("vcopy_oldptr_{tag}"), old);
    free(text, O::reg(format!("vcopy_oldptr_{tag}")));
    emit_fat(
        text,
        format!("vcopy_fat0_{tag}"),
        format!("vcopy_fat_{tag}"),
        O::reg(format!("vcopy_buf_{tag}")),
        O::reg(format!("vcopy_len_{tag}")),
    );
    text.emit(I::store(
        vector_ty(),
        O::reg(format!("vcopy_fat_{tag}")),
        dest_ptr,
    ));
}

/// Stores `value` into the field at `field_offset` of `object`. Under
/// explicit ownership the field's previous content goes to `previous`, the
/// stored value is owned (the IR retained it), and a weak field stores the
/// object's core id.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_set_member(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    object: ValueId,
    field_offset: u32,
    value: ValueId,
    value_ty: &Type,
    field_ty: &Type,
    previous: Option<ValueId>,
    weak: bool,
    state: &mut EmissionState,
) {
    let llvm_ty = llvm_type(field_ty).expect("validated member type");
    let mut value_op = coerce_to_type(text, value, value_ty, field_ty);
    let member = O::reg(format!("mbrptr{}", value.0));
    let offset = vec![(T::I32, O::uint(u64::from(field_offset)))];
    text.assign(
        format!("mbrptr{}", value.0),
        I::gep(T::I8, v(object), offset),
    );
    if let Some(owner) = struct_owner(module, field_ty) {
        let copy_witness = arc_runtime::struct_copy_witness_name(&owner);
        let destroy_witness = arc_runtime::struct_destroy_witness_name(&owner);
        text.emit(I::call(
            T::Void,
            &destroy_witness,
            vec![(T::Ptr, member.clone())],
        ));
        text.emit(I::call(
            T::Void,
            &copy_witness,
            vec![(T::Ptr, member), (T::Ptr, value_op)],
        ));
        return;
    }
    if let Some(previous) = previous {
        text.assign(
            format!("v{}", previous.0),
            I::load(llvm_ty.clone(), member.clone()),
        );
    }
    if matches!(field_ty, Type::Vector { .. }) {
        emit_vector_field_store(text, member, value_op, field_ty, state);
        return;
    }
    if weak {
        value_op = arc_ops::weak_store_operand(text, &value_op, state);
    }
    text.emit(I::store(llvm_ty, value_op, member));
}

pub(crate) fn emit_member(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    block_id: BlockId,
    destination: ValueId,
    object: ValueId,
    field_offset: u32,
    field_ty: &Type,
    weak: bool,
    state: &mut EmissionState,
) {
    let llvm_ty = llvm_type(field_ty).expect("validated member type");
    let dest = destination.0;
    let null = I::icmp(ICmpCond::Eq, T::Ptr, v(object), O::null());
    text.assign(format!("membernull{dest}"), null);
    let live = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &O::reg(format!("membernull{dest}")),
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
        arc_ops::weak_read(
            text,
            &format!("v{dest}"),
            &O::reg(format!("weakfield{dest}")),
        );
    } else if is_struct_type(module, field_ty) {
        text.assign(
            format!("v{dest}"),
            I::gep(T::I8, member, vec![(T::I32, O::int(0))]),
        );
    } else {
        text.assign(format!("v{dest}"), I::load(llvm_ty, member));
    }
}

pub(crate) fn emit_delete(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    value: ValueId,
    ty: &Type,
) {
    let n = value.0;
    if llvm_type(ty) == Some(crate::layout::vector_ty()) {
        text.assign(format!("delptr{n}"), I::extract(vector_ty(), v(value), 0));
        free(text, O::reg(format!("delptr{n}")));
        return;
    }
    let owner = match ty {
        Type::Named(name) | Type::ImportedNamed { name, .. } if llvm_type(ty) == Some(T::Ptr) => {
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
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    object: ValueId,
    index: ValueId,
    index_ty: &Type,
    ty: &Type,
    context: &'static str,
    state: &mut EmissionState,
) {
    let elem_ty = llvm_type(ty).expect("validated index element");
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("vec{name}{dest}"));
    let ok = take_continuation(block_id, state);
    let index = lower_index(text, index, index_ty);
    text.assign(
        format!("vecptr{dest}"),
        I::extract(vector_ty(), v(object), 0),
    );
    text.assign(
        format!("veclen{dest}"),
        I::extract(vector_ty(), v(object), 1),
    );
    if context == "region" {
        emit_null_region_check(text, block_id, state, &r("ptr"));
    }
    emit_index_check(text, block_id, state, &index, &r("len"), ok, context);
    let slot = I::gep(elem_ty.clone(), r("ptr"), vec![(T::I32, index.at)]);
    text.assign(format!("vecslot{dest}"), slot);
    text.assign(format!("v{dest}"), I::load(elem_ty, r("slot")));
    state.needs_numeric_overflow_trap = true;
}

pub(crate) fn emit_optional_float_default(text: &mut crate::ir::LlvmModule, destination: ValueId) {
    let dest = destination.0;
    let optional = T::struct_of([T::I1, T::Double]);
    let flag = I::insert(optional.clone(), O::undef(), T::I1, O::bool(false), 0);
    text.assign(format!("na{dest}"), flag);
    let na = O::reg(format!("na{dest}"));
    let value = I::insert(optional, na, T::Double, O::float(0.0), 1);
    text.assign(format!("v{dest}"), value);
}

pub(crate) fn extract_optional_float(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    value: ValueId,
) {
    let optional = T::struct_of([T::I1, T::Double]);
    text.assign(
        format!("v{}", destination.0),
        I::extract(optional, v(value), 1),
    );
}

/// The storage of a fixed vector and its outermost count: `[count x element]`
/// for one dimension, the block of [`nested_vectors`] for more.
pub(crate) fn fixed_vector_array(ty: &Type) -> Option<(T, u64)> {
    let Type::Vector {
        element,
        dimensions,
    } = ty
    else {
        return None;
    };
    let [count] = dimensions.as_slice() else {
        return nest(ty).map(|nest| (nest.storage(), nest.count()));
    };
    let length = usize::try_from(*count)
        .ok()
        .filter(|_| *count != u64::MAX)?;
    Some((T::Array(length, Box::new(llvm_type(element)?)), *count))
}

/// Copies the elements of the fixed vector `ty` from `source` to the
/// storage `target`.
pub(crate) fn emit_copy_elements(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    ty: &Type,
    target: O,
    source: O,
) {
    if let Some(nest) = nest(ty) {
        nest.emit_copy(text, &target, source);
        return;
    }
    let Some((array, count)) = fixed_vector_array(ty) else {
        return;
    };
    if let Type::Vector { element, .. } = ty
        && let Some(owner) = struct_owner(module, element)
    {
        for i in 0..count {
            let tag = text.fresh_tag();
            let dst_slot = format!("cpdstslot{tag}");
            let src_slot = format!("cpsrcslot{tag}");
            text.assign(
                dst_slot.clone(),
                I::gep(T::Ptr, target.clone(), vec![(T::I32, O::uint(i))]),
            );
            text.assign(
                src_slot.clone(),
                I::gep(T::Ptr, source.clone(), vec![(T::I32, O::uint(i))]),
            );
            let dst_ptr = format!("cpdstptr{tag}");
            let src_ptr = format!("cpsrcptr{tag}");
            text.assign(dst_ptr.clone(), I::load(T::Ptr, O::reg(dst_slot)));
            text.assign(src_ptr.clone(), I::load(T::Ptr, O::reg(src_slot)));
            let is_null = format!("cpnull{tag}");
            text.assign(
                is_null.clone(),
                I::icmp(ICmpCond::Eq, T::Ptr, O::reg(src_ptr.clone()), O::null()),
            );
            let safe_src = format!("cpsafesrc{tag}");
            text.assign(
                safe_src.clone(),
                I::select(
                    O::reg(is_null.clone()),
                    T::Ptr,
                    O::reg(dst_ptr.clone()),
                    O::reg(src_ptr),
                ),
            );
            let copy_witness = arc_runtime::struct_copy_witness_name(&owner);
            let destroy_witness = arc_runtime::struct_destroy_witness_name(&owner);
            text.emit(I::call(
                T::Void,
                &destroy_witness,
                vec![(T::Ptr, O::reg(dst_ptr.clone()))],
            ));
            text.emit(I::call(
                T::Void,
                &copy_witness,
                vec![(T::Ptr, O::reg(dst_ptr)), (T::Ptr, O::reg(safe_src))],
            ));
        }
        return;
    }
    let tag = text.fresh_tag();
    let size = I::gep(array.clone(), O::null(), vec![(T::I32, O::int(1))]);
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
    text.emit(I::call(T::Void, crate::runtime_abi::memmove().name, args));
}

/// The elements a vector slot owns: a fixed vector is a value (`0.6.md`,
/// "value / copy semantics"), so a slot never points at another binding's
/// elements. `vprev` keeps a replaced content alive for its release when
/// the elements hold references.
pub(crate) fn emit_vector_storage(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    slot: usize,
    ty: &Type,
    references: bool,
) {
    let Some((array, count)) = fixed_vector_array(ty) else {
        return;
    };
    let array = array.clone();
    let storage = O::reg(format!("vstore{slot}"));
    text.assign(format!("vstore{slot}"), I::alloca(array.clone()));
    text.emit(I::store(
        array.clone(),
        O::zero_initializer(),
        storage.clone(),
    ));
    if let Some(nest) = nest(ty) {
        nest.emit_copy(text, &storage, O::null());
    }
    if let Type::Vector { element, .. } = ty
        && let Some(owner) = struct_owner(module, element)
    {
        let bytes = class_instance_bytes(module, &owner);
        let total_bytes = count.saturating_mul(bytes);
        let array_bytes = T::Array(
            usize::try_from(total_bytes).expect("fits usize"),
            Box::new(T::I8),
        );
        let struct_data = format!("vstructdata{slot}");
        text.assign(struct_data.clone(), I::alloca(array_bytes.clone()));
        text.emit(I::store(
            array_bytes,
            O::zero_initializer(),
            O::reg(struct_data.clone()),
        ));
        for i in 0..count {
            let elem_ptr = format!("vstructelem{slot}_{i}");
            let offset = i.saturating_mul(bytes);
            text.assign(
                elem_ptr.clone(),
                I::gep(
                    T::I8,
                    O::reg(struct_data.clone()),
                    vec![(T::I64, O::uint(offset))],
                ),
            );
            let elem_slot = format!("vstructslot{slot}_{i}");
            text.assign(
                elem_slot.clone(),
                I::gep(T::Ptr, storage.clone(), vec![(T::I32, O::uint(i))]),
            );
            text.emit(I::store(T::Ptr, O::reg(elem_ptr), O::reg(elem_slot)));
        }
    }
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
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    (storage, previous, binding): (&O, &O, &O),
    ty: &Type,
) {
    if fixed_vector_array(ty).is_some() {
        emit_copy_elements(text, module, ty, previous.clone(), storage.clone());
        let tag = text.fresh_tag();
        text.assign(format!("vpold{tag}"), I::load(vector_ty(), binding.clone()));
        let old = O::reg(format!("vpold{tag}"));
        let moved = I::insert(vector_ty(), old, T::Ptr, previous.clone(), 0);
        text.assign(format!("vpfat{tag}"), moved);
        let fat = O::reg(format!("vpfat{tag}"));
        text.emit(I::store(vector_ty(), fat, binding.clone()));
    }
}

/// Copies the elements of `value` (a `{ ptr, i32 }`) into `storage`; the
/// result points there.
pub(crate) fn emit_vector_copy(
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    storage: &crate::ir::LlvmOperand,
    value: &crate::ir::LlvmOperand,
    ty: &Type,
) -> Option<crate::ir::LlvmOperand> {
    fixed_vector_array(ty)?;
    let tag = text.fresh_tag();
    text.assign(
        format!("vcsrc{tag}"),
        I::extract(vector_ty(), value.clone(), 0),
    );
    let source = O::reg(format!("vcsrc{tag}"));
    emit_copy_elements(text, module, ty, storage.clone(), source);
    let moved = I::insert(vector_ty(), value.clone(), T::Ptr, storage.clone(), 0);
    text.assign(format!("vcfat{tag}"), moved);
    Some(O::reg(format!("vcfat{tag}")))
}

/// Whether a `Default` of `ty` with `dimensions` is a fixed vector of two or
/// more dimensions with scalar or reference elements.
pub(crate) fn nested_default(ty: &Type, dimensions: &[usize], dynamic: &[ValueId]) -> bool {
    dynamic.is_empty() && nest(ty).is_some_and(|nest| nest.has_default(dimensions))
}

/// Whether a value of `ty` carries a vector of two or more dimensions, whose
/// copies call `bn_rt_vector_nest`.
pub(crate) fn carries_nested_vector(ty: &Type) -> bool {
    returned_vector(ty).and_then(nest).is_some()
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
    text: &mut crate::ir::LlvmModule,
    module: &Module,
    value: &O,
    ty: &Type,
    buffer: &O,
    out: &str,
) -> Option<()> {
    let vector = returned_vector(ty)?;
    if vector == ty {
        let moved = emit_vector_copy(text, module, buffer, value, ty)?;
        let fat = I::insert(vector_ty(), moved, T::Ptr, buffer.clone(), 0);
        text.assign(out, fat);
        return Some(());
    }
    fixed_vector_array(vector)?;
    let code = bn_types::alternatives::member_code(vector)?;
    let tag = text.fresh_tag();
    let r = |name: &str| O::reg(format!("vr{name}{tag}"));
    let layout = crate::general_alternative::layout();
    text.assign(
        format!("vrtag{tag}"),
        I::extract(layout.clone(), value.clone(), 0),
    );
    let holds = I::icmp(ICmpCond::Eq, T::I32, r("tag"), O::uint(u64::from(code)));
    text.assign(format!("vris{tag}"), holds);
    text.assign(
        format!("vrptr{tag}"),
        I::extract(layout.clone(), value.clone(), 1),
    );
    let source = I::select(r("is"), T::Ptr, r("ptr"), buffer.clone());
    text.assign(format!("vrsrc{tag}"), source);
    emit_copy_elements(text, module, vector, buffer.clone(), r("src"));
    let target = I::select(r("is"), T::Ptr, buffer.clone(), r("ptr"));
    text.assign(format!("vrnew{tag}"), target);
    text.assign(out, I::insert(layout, value.clone(), T::Ptr, r("new"), 1));
    Some(())
}
