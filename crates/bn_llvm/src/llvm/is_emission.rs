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
        Some(AlternativeLayout::Region) => {
            alternatives::emit_region_is(text, destination, left, left_ty, right_ty, test_name)
        }
        None => false,
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
    text.assign(
        format!("eofptr{dest}"),
        I::select(
            O::bool(true),
            T::Ptr,
            crate::layout::string_payload(".bn_eof"),
            crate::layout::string_payload(".bn_eof"),
        ),
    );
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
    // non-NULL pointer (`0.6.md`). Any other `IS POINTER TO T` reads the
    // allocation: the element code (field 2 of a `POINTER TO VOID`, the
    // static element of a typed pointer) and the length (K55).
    if matches!(left_ty, Type::Pointer { .. }) {
        return emit_pointer_is(text, destination, left, left_ty, test_name);
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
        let not_eof = I::icmp(
            ICmpCond::Ne,
            T::Ptr,
            r("isptr"),
            crate::layout::string_payload(".bn_eof"),
        );
        text.assign(format!("unionnoteof{dest}"), not_eof);
        let both = I::binary(BinaryOp::And, T::I1, r("noterror"), r("noteof"));
        text.assign(format!("v{dest}"), both);
    } else {
        emit_is_constant(text, destination, r("noterror"));
    }
    true
}

/// `%v{destination}` for `IS POINTER TO` on a pointer value.
fn emit_pointer_is(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    test_name: &str,
) -> bool {
    let Type::Pointer { element, .. } = left_ty else {
        return false;
    };
    let Some(layout) = llvm_type(left_ty) else {
        return false;
    };
    let void_value = layout == crate::layout::region_ty();
    let typed = layout == crate::layout::vector_ty();
    if !void_value && !typed {
        return false;
    }
    let id = destination.0;
    let pointer = format!("pisptr{id}");
    text.assign(pointer.clone(), I::extract(layout.clone(), v(left), 0));
    let non_null = format!("pisnn{id}");
    text.assign(
        non_null.clone(),
        I::icmp(ICmpCond::Ne, T::Ptr, O::reg(&pointer), O::null()),
    );
    let void_test = test_name == bn_catalog::text("llvm", "type.pointer_to_void")
        || bn_types::pointers::parse_pointer_is(test_name).is_some_and(|spec| spec.void);
    if void_test {
        emit_is_constant(text, destination, O::reg(non_null));
        return true;
    }
    let Some(spec) = bn_types::pointers::parse_pointer_is(test_name) else {
        return false;
    };
    let expected = bn_types::alternatives::member_code(&spec.element).unwrap_or(u32::MAX);
    let code = if void_value {
        let name = format!("piscode{id}");
        text.assign(name.clone(), I::extract(layout, v(left), 2));
        O::reg(name)
    } else {
        let static_code = bn_types::alternatives::member_code(element).unwrap_or(u32::MAX);
        O::int(i64::from(static_code))
    };
    let code_ok = format!("piscodeok{id}");
    text.assign(
        code_ok.clone(),
        I::icmp(ICmpCond::Eq, T::I32, code, O::int(i64::from(expected))),
    );
    let mut held = format!("pisboth{id}");
    text.assign(
        held.clone(),
        I::binary(BinaryOp::And, T::I1, O::reg(non_null), O::reg(&code_ok)),
    );
    let expected_len = match spec.length {
        bn_types::PointerLength::Fixed(length) => Some(length),
        bn_types::PointerLength::One => Some(1),
        bn_types::PointerLength::Dynamic => None,
    };
    if let Some(length) = expected_len {
        let len = format!("pislen{id}");
        let len_layout = if void_value {
            crate::layout::region_ty()
        } else {
            crate::layout::vector_ty()
        };
        text.assign(len.clone(), I::extract(len_layout, v(left), 1));
        let len_ok = format!("pislenok{id}");
        let wide = i64::try_from(length).unwrap_or(i64::MAX);
        text.assign(
            len_ok.clone(),
            I::icmp(ICmpCond::Eq, T::I32, O::reg(len), O::int(wide)),
        );
        let all = format!("pisall{id}");
        text.assign(
            all.clone(),
            I::binary(BinaryOp::And, T::I1, O::reg(&held), O::reg(&len_ok)),
        );
        held = all;
    }
    emit_is_constant(text, destination, O::reg(held));
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
    let compare = I::icmp(
        cond,
        T::Ptr,
        v(left),
        crate::layout::string_payload(".bn_eof"),
    );
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

    #[test]
    fn nullable_region_length_test_reads_the_stored_count() {
        let mut llvm = crate::ir::LlvmModule::new();
        let region = Type::Alternative(vec![
            Type::Pointer {
                element: Box::new(Type::Integer(IntegerType::Int32)),
                length: bn_types::PointerLength::Dynamic,
            },
            Type::Null,
        ]);
        assert!(emit_is(
            &mut llvm,
            ValueId(2),
            ValueId(1),
            &region,
            &Type::TypeName("POINTER TO INTEGER[3]".into()),
        ));
        let rendered = llvm.render();
        assert!(rendered.contains("extractvalue"), "{rendered}");
        assert!(rendered.contains("icmp eq i32"), "{rendered}");
        assert!(rendered.contains(", 3"), "{rendered}");
        assert!(rendered.contains("and i1"), "{rendered}");
    }

    #[test]
    fn nullable_region_void_is_a_non_null_pointer() {
        let mut llvm = crate::ir::LlvmModule::new();
        let region = Type::Alternative(vec![
            Type::Pointer {
                element: Box::new(Type::Integer(IntegerType::Int32)),
                length: bn_types::PointerLength::Dynamic,
            },
            Type::Null,
        ]);
        assert!(emit_is(
            &mut llvm,
            ValueId(2),
            ValueId(1),
            &region,
            &Type::TypeName("POINTER TO VOID".into()),
        ));
        let rendered = llvm.render();
        assert!(rendered.contains("icmp ne ptr"), "{rendered}");
        assert!(
            !rendered.contains("extractvalue { ptr, i32 } %v1, 1"),
            "{rendered}"
        );
    }

    #[test]
    fn imported_alternative_matches_only_the_names_lowering_emits() {
        let member = Type::ImportedNamed {
            module: bn_types::ModuleId(0),
            name: "Logger".into(),
        };
        assert!(crate::alternatives::alternative_is(&member, "Logger"));
        assert!(crate::alternatives::alternative_is(&member, "#0.Logger"));
        assert!(!crate::alternatives::alternative_is(&member, "Log.Logger"));
        assert!(!crate::alternatives::alternative_is(&member, "Foo.Logger"));
        assert!(!crate::alternatives::alternative_is(&member, "#1.Logger"));
    }
}
