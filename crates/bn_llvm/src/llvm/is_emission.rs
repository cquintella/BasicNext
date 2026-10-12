// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `IS` type tests: a value against a type name, per layout (sentinel
// pointers, status aggregates, `EOF`, general alternatives, static answers).
#![allow(clippy::wildcard_imports, clippy::too_many_arguments)]
use super::*;
use crate::ir::{FCmpCond, ICmpCond, LlvmOperand as O, LlvmType as T};

/// `%v{dest} = xor i1 <flag>, true`: the negation of a flag in slot 0.
fn emit_not_flag(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    flag: &str,
    aggregate: T,
    left: ValueId,
) {
    let dest = destination.0;
    text.assign(format!("{flag}{dest}"), I::extract(aggregate, v(left), 0));
    let flag = O::reg(format!("{flag}{dest}"));
    let not = I::binary(BinaryOp::Xor, T::I1, flag, O::bool(true));
    text.assign(format!("v{dest}"), not);
}

/// `%v{dest} = or i1 false, <bit>`: a constant `IS` result.
fn emit_is_constant(text: &mut crate::ir::LlvmModule, destination: ValueId, bit: O) {
    let inst = I::binary(BinaryOp::Or, T::I1, O::bool(false), bit);
    text.assign(format!("v{}", destination.0), inst);
}

/// `IS`, by the layout of the tested value (`AlternativeLayout`). Returns
/// false for a form without an `IS` lowering, which the caller refuses with a
/// support diagnostic rather than fold to a constant.
pub(crate) fn emit_is(
    text: &mut crate::ir::LlvmModule,
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
    text: &mut crate::ir::LlvmModule,
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
    text: &mut crate::ir::LlvmModule,
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
    let aggregate = llvm_type(left_ty).expect("status layout");
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
fn emit_eof_is(text: &mut crate::ir::LlvmModule, destination: ValueId, left: ValueId) {
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
    text: &mut crate::ir::LlvmModule,
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
    if left_llvm == Some(T::Ptr) && (test_name == "NULL" || matches!(right_ty, Type::Null)) {
        text.assign(own, I::icmp(ICmpCond::Eq, T::Ptr, v(left), O::null()));
        return true;
    }
    if let Some(aggregate) = left_llvm.clone().filter(|ty| {
        crate::layout::is_one_of(
            ty,
            &[
                crate::layout::status_pointer_ty(),
                crate::layout::status_endpoint_ty(),
                crate::layout::handle_result_ty(),
            ],
        )
    }) && test_name == "Error"
    {
        text.assign(own, I::extract(aggregate, v(left), 0));
        return true;
    }
    if left_llvm == Some(crate::layout::handle_result_ty())
        && (test_name == "EOF" || matches!(right_ty, Type::EndOfFile))
    {
        emit_eof_is(text, destination, left);
        return true;
    }
    let optional = T::struct_of([T::I1, T::Double]);
    if left_llvm == Some(crate::layout::optional_float_ty()) && test_name == "NA" {
        text.assign(own, I::extract(optional, v(left), 0));
        return true;
    }
    if left_llvm == Some(crate::layout::optional_float_ty())
        && matches!(test_name, "FLOAT" | "FLOAT64")
    {
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
            "NAN" => Some((FCmpCond::Uno, 0.0)),
            "INF" => Some((FCmpCond::Oeq, f64::INFINITY)),
            "-INF" => Some((FCmpCond::Oeq, f64::NEG_INFINITY)),
            _ => None,
        };
        if let Some((cond, constant)) = check {
            text.assign(own, I::fcmp(cond, llvm_ty, v(left), O::float(constant)));
            return true;
        }
    }
    // On a pointer value the only valid tests are pointer tests (semantic
    // analysis rejects the rest). `IS POINTER TO VOID` holds for every
    // non-NULL pointer, typed, region or opaque (`0.6.md`; K51): its pointer
    // sits at field 0 of every pointer layout. Other `IS POINTER TO T` tests
    // wait for decision K55 and are refused.
    if matches!(left_ty, Type::Pointer { .. }) {
        let Some(layout) = left_llvm else {
            return false;
        };
        if test_name != bn_catalog::text("llvm", "type.pointer_to_void") {
            return false;
        }
        let data = format!("isvoiddata{}", destination.0);
        text.assign(data.clone(), I::extract(layout, v(left), 0));
        text.assign(own, I::icmp(ICmpCond::Ne, T::Ptr, O::reg(data), O::null()));
        return true;
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
    text: &mut crate::ir::LlvmModule,
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
    text: &mut crate::ir::LlvmModule,
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

#[cfg(test)]
mod tests {
    use super::emit_is;
    use bn_ir::ValueId;
    use bn_types::{IntegerType, Type};

    #[test]
    fn eof_type_name_uses_the_eof_marker_for_alternative_values() {
        let mut llvm = crate::ir::LlvmModule::new();
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
        let llvm = llvm.render();
        assert!(llvm.contains("icmp eq ptr"));
        assert!(llvm.contains("@.bn_eof"));
    }
}
