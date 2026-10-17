// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native alternatives stored as `{ i1 error, ptr, i64 }`: the `NA` / `EOF`
// sentinels in the pointer field, `IS` tests on them, `IS T` on the value
// side of `T OR Error`, and loads of a STRING narrowed by `IS`.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::{AlternativeLayout, handle_result_ty, vector_ty};

/// The pointer a `{ i1, ptr, i64 }` alternative stores for a value that is
/// neither the payload nor an `Error`: `NA` or `EOF`.
#[derive(Clone, Copy)]
pub(crate) enum Sentinel {
    NotAvailable,
    EndOfFile,
}

impl Sentinel {
    pub(crate) fn of(alternatives: &[Type]) -> Option<Self> {
        if string_na_or_error(alternatives)
            || scalar_na_or_error(alternatives)
            || error_or_na(alternatives)
        {
            Some(Self::NotAvailable)
        } else if string_eof_or_error(alternatives) || integer_eof_or_error(alternatives) {
            Some(Self::EndOfFile)
        } else {
            None
        }
    }

    /// The STRING payload of this sentinel.
    pub(crate) fn pointer(self) -> O {
        match self {
            Self::NotAvailable => crate::layout::string_payload(".bn_na"),
            Self::EndOfFile => crate::layout::string_payload(".bn_eof"),
        }
    }
}

/// `FS.File.ReadLine`: the string, the `@.bn_eof` sentinel, or an `Error`.
pub(crate) fn string_eof_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 3
        && alternatives.iter().any(|ty| matches!(ty, Type::String))
        && alternatives.iter().any(|ty| matches!(ty, Type::EndOfFile))
        && alternatives.iter().any(is_error_type)
}

/// The type a `Load` of `stored` yields when the IR asks for `ty`: a slot
/// keeps its stored representation, except a STRING narrowed by `IS`.
pub(crate) fn loaded_type(stored: Option<&Type>, ty: &Type) -> Type {
    stored
        .filter(|stored| llvm_type(stored).is_some())
        .filter(|stored| llvm_type(stored) != llvm_type(ty) && !narrows(stored, ty))
        .cloned()
        .unwrap_or_else(|| ty.clone())
}

/// A `{ i1, ptr, i64 }` alternative loaded as one of its value types after
/// an `IS` narrowing: STRING is the pointer field; an integer, BOOLEAN, or
/// float is the `i64` payload.
pub(crate) fn narrows(stored: &Type, loaded: &Type) -> bool {
    if let Some(members) = general_alternative(stored) {
        return members.contains(loaded);
    }
    // `INT32 OR NULL` / `FLOAT OR NA`: the value is field 1.
    let from_payload = matches!(
        loaded,
        Type::String | Type::Integer(_) | Type::Boolean | Type::Float(_)
    ) && llvm_type(stored) == Some(crate::layout::handle_result_ty())
        && matches!(stored, Type::Alternative(alternatives) if alternatives.contains(loaded));
    from_payload || narrows_to_error(stored, loaded) || narrows_to_endpoint(stored, loaded)
}

/// `HOST.Net.Endpoint` narrowed out of `Endpoint OR Error` (`{ i1, ptr, i32 }`):
/// the address and port fields (known issue K36).
fn narrows_to_endpoint(stored: &Type, loaded: &Type) -> bool {
    llvm_type(stored) == Some(crate::layout::status_endpoint_ty())
        && llvm_type(loaded) == Some(crate::layout::vector_ty())
        && matches!(stored, Type::Alternative(alternatives) if alternatives.contains(loaded))
}

/// An `Error` narrowed out of an alternative whose layout is not the
/// `Error` layout (`Net.Endpoint OR Error` is `{ i1, ptr, i32 }`): the
/// pointer is the runtime record, which carries the code.
fn narrows_to_error(stored: &Type, loaded: &Type) -> bool {
    is_error_type(loaded)
        && llvm_type(stored).is_some_and(|ty| {
            crate::layout::is_one_of(
                &ty,
                &[
                    crate::layout::status_pointer_ty(),
                    crate::layout::status_endpoint_ty(),
                ],
            )
        })
        && matches!(stored, Type::Alternative(alternatives) if alternatives.iter().any(is_error_type))
}

/// Whether `IS test` names `value_ty`, the non-`Error` side of `T OR Error`.
pub(crate) fn alternative_is(value_ty: &Type, test: &str) -> bool {
    match value_ty {
        Type::Named(name) => name == test,
        // Lowering emits the simple name for a standard module (`Logger`)
        // and `#<module>.<Name>` otherwise (`control_flow.rs`). An alias
        // (`Log.Logger`) or another qualifier (`Foo.Logger`) is not either.
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            bn_types::import_test_matches(*module, name, test)
        }
        Type::Null => test == "NULL",
        Type::NotAvailable => test == "NA",
        Type::EndOfFile => test == "EOF",
        // `ELEMENT[n]...`, the name lowering gives a vector test.
        Type::Vector {
            element,
            dimensions,
        } => test.split_once('[').is_some_and(|(name, rest)| {
            let tested = rest
                .trim_end_matches(']')
                .split("][")
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>();
            tested.as_ref() == Ok(dimensions) && alternative_is(element, name)
        }),
        _ => bn_types::scalar_test_type(test).as_ref() == Some(value_ty),
    }
}

/// Loads a value narrowed by `IS` (see [`narrows`]) from a
/// `{ i1, ptr, i64 }` slot.
pub(crate) fn emit_narrowed_load(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    slot: usize,
    stored: Option<&Type>,
    loaded: &Type,
) {
    let dest = destination.0;
    if stored.and_then(general_alternative).is_some() {
        let held = format!("narrowload{dest}");
        text.assign(
            held.clone(),
            crate::ir::LlvmInst::load(
                crate::general_alternative::layout(),
                O::reg(format!("s{slot}")),
            ),
        );
        general_alternative::extract(text, &format!("v{dest}"), &O::reg(held), loaded);
        return;
    }
    let union = T::struct_of([T::I1, T::Ptr, T::I64]);
    let reg = |name: &str| O::reg(format!("{name}{dest}"));
    if stored.is_some_and(|stored| narrows_to_endpoint(stored, loaded)) {
        let status = T::struct_of([T::I1, T::Ptr, T::I32]);
        text.assign(
            format!("narrowload{dest}"),
            I::load(status.clone(), O::reg(format!("s{slot}"))),
        );
        text.assign(
            format!("narrowaddress{dest}"),
            I::extract(status.clone(), reg("narrowload"), 1),
        );
        text.assign(
            format!("narrowport{dest}"),
            I::extract(status, reg("narrowload"), 2),
        );
        let head = I::insert(vector_ty(), O::undef(), T::Ptr, reg("narrowaddress"), 0);
        text.assign(format!("narrowendpoint{dest}"), head);
        let endpoint = I::insert(
            vector_ty(),
            reg("narrowendpoint"),
            T::I32,
            reg("narrowport"),
            1,
        );
        text.assign(format!("v{dest}"), endpoint);
        return;
    }
    if let Some(stored) = stored.filter(|stored| narrows_to_error(stored, loaded)) {
        let layout = llvm_type(stored).expect("validated error layout");
        text.assign(
            format!("narrowload{dest}"),
            I::load(layout.clone(), O::reg(format!("s{slot}"))),
        );
        text.assign(
            format!("narrowrecord{dest}"),
            I::extract(layout, reg("narrowload"), 1),
        );
        let record = vec![(T::Ptr, reg("narrowrecord"))];
        text.assign(
            format!("narrowcode{dest}"),
            I::call(T::I64, "bn_rt_error_code", record),
        );
        text.assign(
            format!("narrowerror{dest}"),
            I::insert(union.clone(), O::undef(), T::I1, O::bool(true), 0),
        );
        text.assign(
            format!("narrowerrorptr{dest}"),
            I::insert(
                union.clone(),
                reg("narrowerror"),
                T::Ptr,
                reg("narrowrecord"),
                1,
            ),
        );
        text.assign(
            format!("v{dest}"),
            I::insert(union, reg("narrowerrorptr"), T::I64, reg("narrowcode"), 2),
        );
        return;
    }
    text.assign(
        format!("narrowload{dest}"),
        I::load(union.clone(), O::reg(format!("s{slot}"))),
    );
    if *loaded == Type::String {
        text.assign(format!("v{dest}"), I::extract(union, reg("narrowload"), 1));
        return;
    }
    text.assign(
        format!("narrowpayload{dest}"),
        I::extract(union, reg("narrowload"), 2),
    );
    let payload = reg("narrowpayload");
    let conversion = match (loaded, llvm_type(loaded)) {
        (Type::Boolean, _) => I::icmp(ICmpCond::Ne, T::I64, payload, O::int(0)),
        (Type::Float(FloatType::Float64), _) => {
            I::cast(CastOp::BitCast, T::I64, payload, T::Double)
        }
        (Type::Float(FloatType::Float32), _) => {
            text.assign(
                format!("narrowdouble{dest}"),
                I::cast(CastOp::BitCast, T::I64, payload, T::Double),
            );
            I::cast(CastOp::FPTrunc, T::Double, reg("narrowdouble"), T::Float)
        }
        (_, Some(T::I64)) => I::binary(BinaryOp::Add, T::I64, O::int(0), payload),
        (_, Some(width)) => I::cast(CastOp::Trunc, T::I64, payload, width),
        (_, None) => unreachable!("validated narrowed type"),
    };
    text.assign(format!("v{dest}"), conversion);
}

/// `IS` on an alternative with a sentinel (`STRING OR NA OR Error`,
/// `STRING OR EOF OR Error`, ...). Returns false when `left_ty` has none.
pub(crate) fn emit_sentinel_is(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    let Type::Alternative(alternatives) = left_ty else {
        return false;
    };
    let Some(sentinel) = Sentinel::of(alternatives) else {
        return false;
    };
    let id = destination.0;
    let reg = |name: &str| O::reg(format!("{name}{id}"));
    let sentinel_name = match sentinel {
        Sentinel::NotAvailable => "NA",
        Sentinel::EndOfFile => "EOF",
    };
    let union = T::struct_of([T::I1, T::Ptr, T::I64]);
    let value = O::reg(format!("v{}", left.0));
    text.assign(
        format!("cellerror{id}"),
        I::extract(union.clone(), value.clone(), 0),
    );
    text.assign(format!("cellptr{id}"), I::extract(union, value, 1));
    text.assign(
        format!("cellnaptr{id}"),
        I::select(
            O::bool(true),
            T::Ptr,
            sentinel.pointer(),
            sentinel.pointer(),
        ),
    );
    text.assign(
        format!("cellna{id}"),
        I::icmp(ICmpCond::Eq, T::Ptr, reg("cellptr"), reg("cellnaptr")),
    );
    if test_name == "Error" {
        text.assign(
            format!("v{id}"),
            I::binary(BinaryOp::Or, T::I1, O::bool(false), reg("cellerror")),
        );
    } else if test_name == sentinel_name || matches!(right_ty, Type::EndOfFile | Type::NotAvailable)
    {
        text.assign(
            format!("cellok{id}"),
            I::binary(BinaryOp::Xor, T::I1, reg("cellerror"), O::bool(true)),
        );
        text.assign(
            format!("v{id}"),
            I::binary(BinaryOp::And, T::I1, reg("cellok"), reg("cellna")),
        );
    } else {
        let matches = alternatives
            .iter()
            .any(|ty| ty == right_ty || alternative_is(ty, test_name));
        text.assign(
            format!("cellabsent{id}"),
            I::binary(BinaryOp::Or, T::I1, reg("cellerror"), reg("cellna")),
        );
        text.assign(
            format!("cellpresent{id}"),
            I::binary(BinaryOp::Xor, T::I1, reg("cellabsent"), O::bool(true)),
        );
        text.assign(
            format!("v{id}"),
            I::binary(
                BinaryOp::And,
                T::I1,
                reg("cellpresent"),
                O::int(i64::from(matches)),
            ),
        );
    }
    true
}

/// The alternatives with dedicated layouts in `llvm_type`.
pub(crate) fn imported_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::Named(name) if name == "Error"))
        && alternatives.iter().any(|ty| {
            matches!(
                ty,
                Type::ImportedNamed { .. } | Type::ImportedTypeName { .. }
            )
        })
}

/// A HOST or module handle `OR Error`, the form `bn_rt` returns. A program
/// class (`Box OR Error`) is not one: it has the general layout.
pub(crate) fn opaque_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 2
        && alternatives
            .iter()
            .any(|ty| matches!(ty, Type::Named(name) if name == "Error"))
        && alternatives.iter().any(|ty| {
            !is_error_type(ty)
                && !general_alternative::pointer_member(ty)
                && matches!(
                    ty,
                    Type::Named(_) | Type::ImportedNamed { .. } | Type::ImportedTypeName { .. }
                )
        })
}

/// Converts `value`, of type `from`, into the alternative `to`: the one
/// construction helper. `None` when `to` needs no conversion from `from`.
pub(crate) fn construct(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    from: &Type,
    to: &Type,
) -> Option<crate::ir::LlvmOperand> {
    let Type::Alternative(members) = to else {
        return None;
    };
    let v = value.0;
    let operand = O::reg(format!("v{v}"));
    match AlternativeLayout::of(members)? {
        AlternativeLayout::General => {
            // `fresh_tag` keeps names unique when one value converts twice.
            let name = format!("genwrap{v}_{}", text.fresh_tag());
            Some(general_alternative::wrap(
                text, &name, operand, from, members,
            ))
        }
        AlternativeLayout::Status if general_alternative(from).is_some() => {
            let name = format!("genwiden{v}_{}", text.fresh_tag());
            let widened = general_alternative::narrow_layout(text, &name, operand, members);
            Some(widened)
        }
        AlternativeLayout::Status => {
            let sentinel = Sentinel::of(members).filter(|sentinel| {
                matches!(
                    (sentinel, from),
                    (Sentinel::NotAvailable, Type::NotAvailable)
                        | (Sentinel::EndOfFile, Type::EndOfFile)
                )
            });
            if let Some(sentinel) = sentinel {
                // `NA` / `EOF` is its sentinel pointer, not an error.
                let name = format!("nasentinel{v}_{}", text.fresh_tag());
                let pointer = sentinel.pointer();
                return Some(emit_status(text, &name, O::bool(false), pointer, O::int(0)));
            }
            let from_llvm = llvm_type(from)?;
            // `STRING OR EOF` widens as its pointer: the line, or the
            // `@.bn_eof` sentinel the wider layout also reads as `EOF`.
            let string_or_eof = matches!(from, Type::Alternative(members)
                if members.len() == 2 && members.contains(&Type::String)
                    && members.contains(&Type::EndOfFile));
            let scalar = matches!(
                from_llvm,
                T::I1 | T::I8 | T::I16 | T::I32 | T::I64 | T::Float | T::Double
            );
            (scalar || *from == Type::String || string_or_eof)
                .then(|| wrap_success(text, value, from, from_llvm))
        }
        // An `Error` into `Endpoint OR Error` or `Addresses OR Error`: the
        // error flag and the runtime record, which carries the code (the
        // inverse of the narrowing in `narrow_member`; known issue K36).
        layout @ (AlternativeLayout::StatusEndpoint | AlternativeLayout::StatusPointer)
            if is_error_type(from) =>
        {
            let name = format!("errwiden{v}_{}", text.fresh_tag());
            let record = O::reg(format!("{name}_record"));
            text.assign(
                format!("{name}_record"),
                I::extract(handle_result_ty(), operand, 1),
            );
            let ty = layout.llvm();
            let flag = I::insert(ty.clone(), O::undef(), T::I1, O::bool(true), 0);
            text.assign(format!("{name}_flag"), flag);
            let flagged = O::reg(format!("{name}_flag"));
            if layout == AlternativeLayout::StatusPointer {
                text.assign(&name, I::insert(ty, flagged, T::Ptr, record, 1));
            } else {
                text.assign(
                    format!("{name}_ptr"),
                    I::insert(ty.clone(), flagged, T::Ptr, record, 1),
                );
                let with_record = O::reg(format!("{name}_ptr"));
                text.assign(&name, I::insert(ty, with_record, T::I32, O::int(0), 2));
            }
            Some(O::reg(&name))
        }
        AlternativeLayout::Region if matches!(from, Type::Null) => {
            let name = format!("regionnull{v}_{}", text.fresh_tag());
            Some(null_region(text, &name))
        }
        _ => None,
    }
}

/// `IS` on `POINTER TO E OR NULL` (`{ ptr, i32 }`). `NULL` is a null data
/// pointer. An exact member, and `IS POINTER TO VOID`, are any other pointer.
/// A length the member does not declare (`E[n]` on `E[]`) compares the stored
/// count in field 1. The element is the member's element: this slot has no
/// runtime element code (BDFL decision, 2026-10-16).
pub(crate) fn emit_region_is(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    let Type::Alternative(members) = left_ty else {
        return false;
    };
    let Some(pointer) = members
        .iter()
        .find(|member| matches!(member, Type::Pointer { .. }))
    else {
        return false;
    };
    if test_name == "NULL" || matches!(right_ty, Type::Null) {
        emit_region_pointer_cmp(text, destination, left, ICmpCond::Eq);
        return true;
    }
    if bn_types::pointers::pointer_member_matches(pointer, test_name) {
        emit_region_pointer_cmp(text, destination, left, ICmpCond::Ne);
        return true;
    }
    let Some(spec) = bn_types::pointers::parse_pointer_is(test_name) else {
        return false;
    };
    if spec.void {
        emit_region_pointer_cmp(text, destination, left, ICmpCond::Ne);
        return true;
    }
    if !bn_types::pointers::pointer_element_matches(pointer, test_name) {
        emit_region_bit(text, destination, O::bool(false));
        return true;
    }
    let expected = match spec.length {
        bn_types::PointerLength::Fixed(length) => Some(length),
        bn_types::PointerLength::One => Some(1),
        bn_types::PointerLength::Dynamic => None,
    };
    let Some(length) = expected else {
        emit_region_pointer_cmp(text, destination, left, ICmpCond::Ne);
        return true;
    };
    emit_region_length(text, destination, left, length);
    true
}

fn emit_region_pointer_cmp(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    condition: ICmpCond,
) {
    let id = destination.0;
    text.assign(
        format!("regionptr{id}"),
        I::extract(vector_ty(), O::reg(format!("v{}", left.0)), 0),
    );
    text.assign(
        format!("v{id}"),
        I::icmp(
            condition,
            T::Ptr,
            O::reg(format!("regionptr{id}")),
            O::null(),
        ),
    );
}

fn emit_region_bit(text: &mut crate::ir::LlvmModule, destination: ValueId, bit: O) {
    text.assign(
        format!("v{}", destination.0),
        I::binary(BinaryOp::Or, T::I1, O::bool(false), bit),
    );
}

/// Non-null data pointer and stored count equal to `length`.
fn emit_region_length(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    length: u64,
) {
    let id = destination.0;
    let slot = O::reg(format!("v{}", left.0));
    text.assign(
        format!("regionptr{id}"),
        I::extract(vector_ty(), slot.clone(), 0),
    );
    text.assign(
        format!("regionnn{id}"),
        I::icmp(
            ICmpCond::Ne,
            T::Ptr,
            O::reg(format!("regionptr{id}")),
            O::null(),
        ),
    );
    text.assign(format!("regionlen{id}"), I::extract(vector_ty(), slot, 1));
    let wide = i64::try_from(length).unwrap_or(i64::MAX);
    text.assign(
        format!("regionlenok{id}"),
        I::icmp(
            ICmpCond::Eq,
            T::I32,
            O::reg(format!("regionlen{id}")),
            O::int(wide),
        ),
    );
    text.assign(
        format!("regionboth{id}"),
        I::binary(
            BinaryOp::And,
            T::I1,
            O::reg(format!("regionnn{id}")),
            O::reg(format!("regionlenok{id}")),
        ),
    );
    emit_region_bit(text, destination, O::reg(format!("regionboth{id}")));
}

/// `{ null, 0 }`: the `NULL` member of a `POINTER TO T OR NULL` slot.
pub(crate) fn null_region(text: &mut crate::ir::LlvmModule, name: &str) -> O {
    let ty = vector_ty();
    text.assign(
        format!("{name}_0"),
        I::insert(ty.clone(), O::undef(), T::Ptr, O::null(), 0),
    );
    text.assign(
        name,
        I::insert(ty, O::reg(format!("{name}_0")), T::I32, O::int(0), 1),
    );
    O::reg(name)
}

/// `{ i1 error, ptr, i64 }` from its three fields, as `%{name}`; the
/// intermediate registers are `%{name}_flag` and `%{name}_ptr`.
fn emit_status(
    text: &mut crate::ir::LlvmModule,
    name: &str,
    error: O,
    pointer: O,
    payload: O,
) -> crate::ir::LlvmOperand {
    let union = handle_result_ty();
    let flag = I::insert(union.clone(), O::undef(), T::I1, error, 0);
    text.assign(format!("{name}_flag"), flag);
    let with_pointer = I::insert(
        union.clone(),
        O::reg(format!("{name}_flag")),
        T::Ptr,
        pointer,
        1,
    );
    text.assign(format!("{name}_ptr"), with_pointer);
    let full = I::insert(union, O::reg(format!("{name}_ptr")), T::I64, payload, 2);
    text.assign(name, full);
    O::reg(name)
}

/// A scalar or STRING as the success of a `T OR Error` / `T OR EOF OR …`
/// value: a STRING in the pointer, any other scalar in the `i64` payload
/// (floats as their `double` bits), as `RETURN` builds it.
fn wrap_success(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    from: &Type,
    from_llvm: T,
) -> crate::ir::LlvmOperand {
    let v = value.0;
    let own = O::reg(format!("v{v}"));
    let bits = O::reg(format!("wrapbits{v}"));
    let (pointer, payload) = match from_llvm {
        T::Ptr => (own, O::int(0)),
        T::I64 => (O::null(), own),
        floating if floating.is_floating() => {
            let wide = if floating == T::Float {
                let fpext = I::cast(CastOp::FPExt, T::Float, own, T::Double);
                text.assign(format!("wrapwide{v}"), fpext);
                O::reg(format!("wrapwide{v}"))
            } else {
                own
            };
            let cast = I::cast(CastOp::BitCast, T::Double, wide, T::I64);
            text.assign(format!("wrapbits{v}"), cast);
            (O::null(), bits)
        }
        narrow => {
            // `i1` is BOOLEAN, 0 or 1.
            let extend = if narrow != T::I1 && !is_unsigned(from) {
                CastOp::SExt
            } else {
                CastOp::ZExt
            };
            let cast = I::cast(extend, narrow, own, T::I64);
            text.assign(format!("wrapbits{v}"), cast);
            (O::null(), bits)
        }
    };
    let union = handle_result_ty();
    let tag = I::insert(union.clone(), O::undef(), T::I1, O::bool(false), 0);
    text.assign(format!("wraptag{v}"), tag);
    let with_pointer = I::insert(
        union.clone(),
        O::reg(format!("wraptag{v}")),
        T::Ptr,
        pointer,
        1,
    );
    text.assign(format!("wrapptr{v}"), with_pointer);
    let full = I::insert(union, O::reg(format!("wrapptr{v}")), T::I64, payload, 2);
    text.assign(format!("wrap{v}"), full);
    O::reg(format!("wrap{v}"))
}
