// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The general alternative layout `{ i32 tag, ptr, i64 }` (value-memory-abi.md,
// "Alternative values"): the tag is `bn_types::alternatives::member_code` of
// the member held, so member order does not matter and widening one general
// alternative into another keeps the value unchanged. A `STRING` or the
// `Error` record is the pointer; an integer, `BOOLEAN`, `DATE`, `TIME`, or
// float (as the bits of a `double`) is the `i64`, with the `Error` code there
// too. `NULL`, `NA`, and `EOF` are the tag alone.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    CastOp, FCmpCond, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{self, Double, Float, I1, I32, I64, Ptr},
};
use bn_types::alternatives::member_code;
use bn_types::literals::{NumericClass, numeric_alternative};

pub(crate) const GENERAL_LAYOUT: &str = "{ i32, ptr, i64 }";

pub(crate) fn layout() -> LlvmType {
    LlvmType::struct_of([I32, Ptr, I64])
}

/// Whether the general layout represents every member: one tag per member,
/// and payloads without an ownership protocol (strings and `Error` records
/// live as long as the program). Objects and vectors are stage 2 (bucket
/// typed-llvm-emitter, Sprint 6 phase C).
pub(crate) fn general_members(members: &[Type]) -> bool {
    let mut codes = members.iter().map(member_code).collect::<Option<Vec<_>>>();
    let distinct = codes.as_mut().is_some_and(|codes| {
        let count = codes.len();
        codes.sort_unstable();
        codes.dedup();
        codes.len() == count
    });
    // `STRING OR EOF` is the `INPUT` result: a `ptr` with the `@.bn_eof`
    // sentinel, whose slot also owns the line buffer. It keeps that form.
    let input_result =
        members.len() == 2 && members.contains(&Type::String) && members.contains(&Type::EndOfFile);
    distinct
        && !input_result
        && members.iter().all(|member| {
            stage_one_member(member) || pointer_member(member) || vector_member(member)
        })
}

pub(crate) fn vector_member(member: &Type) -> bool {
    matches!(member, Type::Vector { .. }) && llvm_type(member) == Some("{ ptr, i32 }")
}

/// A class instance member (a `ptr` to the object). Two of them in one
/// alternative share the `OBJECT` code, so `general_members` rejects that;
/// whether the name is a class and not a `STRUCT` needs the module, so
/// `general_alternative_supported` checks it.
pub(crate) fn pointer_member(member: &Type) -> bool {
    // Only a class of this program, a plain name: a qualified or imported
    // name (`Exec.Result`, `Json.Json`, `#3.Box`) may be a HOST or module
    // handle that `bn_rt` returns in its own form.
    matches!(member, Type::Named(name) if !name.contains(['.', '#']))
        && !is_error_type(member)
        && !matches!(member, Type::Named(name) if name == "DATE" || name == "TIME")
        && llvm_type(member) == Some("ptr")
}

fn stage_one_member(member: &Type) -> bool {
    matches!(
        member,
        Type::Integer(_)
            | Type::Float(_)
            | Type::Boolean
            | Type::String
            | Type::Null
            | Type::NotAvailable
            | Type::EndOfFile
    ) || is_error_type(member)
        || matches!(member, Type::Named(name) if name == "DATE" || name == "TIME")
}

/// The members of `ty` when it has the general layout.
pub(crate) fn general_alternative(ty: &Type) -> Option<&[Type]> {
    match ty {
        Type::Alternative(members) if llvm_type(ty) == Some(GENERAL_LAYOUT) => Some(members),
        _ => None,
    }
}

/// The members of `ty` when it is an alternative that can be compared with
/// a member value.
pub(crate) fn comparable_alternative(ty: &Type) -> Option<&[Type]> {
    match ty {
        Type::Alternative(members)
            if llvm_type(ty) == Some(GENERAL_LAYOUT)
                || llvm_type(ty) == Some("{ i1, ptr, i64 }") =>
        {
            Some(members)
        }
        _ => None,
    }
}

pub(crate) fn tag(member: &Type) -> LlvmOperand {
    LlvmOperand::int(i64::from(
        member_code(member).expect("general alternative member has a code"),
    ))
}

/// The member a value of type `from` is stored as: itself, or the member a
/// numeric literal takes (0.6.md, "Numeric literals in alternative types").
pub(crate) fn stored_member<'a>(from: &'a Type, members: &'a [Type]) -> &'a Type {
    NumericClass::of_literal(from).map_or(from, |class| {
        numeric_alternative(members, class).expect("validated literal member")
    })
}

/// Converts `operand`, of type `from`, into the general alternative `to`.
/// Registers are named `{name}_…`; the result is the converted operand.
pub(crate) fn wrap(
    text: &mut String,
    name: &str,
    operand: LlvmOperand,
    from: &Type,
    to: &[Type],
) -> LlvmOperand {
    if general_alternative(from).is_some() {
        // Codes are stable, so one general alternative widens into another
        // unchanged.
        return operand;
    }
    if let Type::Alternative(members) = from {
        return widen_dedicated(text, name, operand, from, members);
    }
    let member = stored_member(from, to);
    let reg = |suffix: &str| format!("{name}_{suffix}");
    let source = llvm_type(from).map(crate::layout::typed_llvm);
    let (pointer, payload) = match member {
        Type::String => (operand, LlvmOperand::int(0)),
        _ if pointer_member(member) => (operand, LlvmOperand::int(0)),
        _ if vector_member(member) => {
            let vec_layout = LlvmType::struct_of([Ptr, I32]);
            text.assign(
                reg("vecptr"),
                LlvmInst::extract(vec_layout.clone(), operand.clone(), 0),
            );
            text.assign(reg("veclen"), LlvmInst::extract(vec_layout, operand, 1));
            text.assign(
                reg("veclen64"),
                LlvmInst::cast(CastOp::ZExt, I32, LlvmOperand::reg(reg("veclen")), I64),
            );
            (
                LlvmOperand::reg(reg("vecptr")),
                LlvmOperand::reg(reg("veclen64")),
            )
        }
        _ if is_error_type(member) => {
            let record = source.expect("validated Error layout");
            text.assign(
                reg("record"),
                LlvmInst::extract(record.clone(), operand.clone(), 1),
            );
            text.assign(reg("code"), LlvmInst::extract(record, operand, 2));
            (
                LlvmOperand::reg(reg("record")),
                LlvmOperand::reg(reg("code")),
            )
        }
        Type::Float(kind) => {
            let mut value = operand;
            let mut width = source.expect("validated float layout");
            if *kind == FloatType::Float32 && width == Double {
                // A FLOAT32 member holds a FLOAT32 value: round first.
                text.assign(
                    reg("single"),
                    LlvmInst::cast(CastOp::FPTrunc, Double, value, Float),
                );
                value = LlvmOperand::reg(reg("single"));
                width = Float;
            }
            if width == Float {
                text.assign(
                    reg("double"),
                    LlvmInst::cast(CastOp::FPExt, Float, value, Double),
                );
                value = LlvmOperand::reg(reg("double"));
            }
            text.assign(
                reg("bits"),
                LlvmInst::cast(CastOp::BitCast, Double, value, I64),
            );
            (LlvmOperand::null(), LlvmOperand::reg(reg("bits")))
        }
        Type::Integer(_) | Type::Boolean | Type::Named(_) => {
            let mut value = operand;
            let mut width = source.expect("validated scalar layout");
            let member_width =
                crate::layout::typed_llvm(llvm_type(member).expect("validated member layout"));
            if width != member_width {
                // An integer literal (i64) takes the member's width first.
                text.assign(
                    reg("narrow"),
                    LlvmInst::cast(CastOp::Trunc, width, value, member_width.clone()),
                );
                value = LlvmOperand::reg(reg("narrow"));
                width = member_width;
            }
            let payload = if width == I64 {
                value
            } else {
                let extend = if is_unsigned(member) || width == I1 {
                    CastOp::ZExt
                } else {
                    CastOp::SExt
                };
                text.assign(reg("wide"), LlvmInst::cast(extend, width, value, I64));
                LlvmOperand::reg(reg("wide"))
            };
            (LlvmOperand::null(), payload)
        }
        // NULL, NA, EOF: the tag is the value.
        _ => (LlvmOperand::null(), LlvmOperand::int(0)),
    };
    let mut tag_value = tag(member);
    if *member == Type::String && to.contains(&Type::EndOfFile) {
        // A string read by `INPUT` is the `@.bn_eof` sentinel at end of
        // input; no other string has that address (a private constant
        // without `unnamed_addr` is never merged).
        text.assign(
            reg("eof"),
            LlvmInst::icmp(
                ICmpCond::Eq,
                Ptr,
                pointer.clone(),
                LlvmOperand::global(".bn_eof"),
            ),
        );
        text.assign(
            reg("eoftag"),
            LlvmInst::select(
                LlvmOperand::reg(reg("eof")),
                I32,
                tag(&Type::EndOfFile),
                tag_value,
            ),
        );
        tag_value = LlvmOperand::reg(reg("eoftag"));
    }
    text.assign(
        reg("tag"),
        LlvmInst::insert(layout(), LlvmOperand::undef(), I32, tag_value, 0),
    );
    text.assign(
        reg("ptr"),
        LlvmInst::insert(layout(), LlvmOperand::reg(reg("tag")), Ptr, pointer, 1),
    );
    text.assign(
        name,
        LlvmInst::insert(layout(), LlvmOperand::reg(reg("ptr")), I64, payload, 2),
    );
    LlvmOperand::reg(name)
}

/// `%v{destination}` = the default of a general alternative: the default of
/// its first member, as `bni` gives it (zero, `FALSE`, `""`).
pub(crate) fn emit_default(text: &mut String, destination: ValueId, members: &[Type]) {
    let first = members.first().expect("alternative has members");
    let name = format!("v{}", destination.0);
    let reg = |suffix: &str| format!("{name}_{suffix}");
    let pointer = if *first == Type::String {
        LlvmOperand::global(".bn_empty")
    } else {
        LlvmOperand::null()
    };
    text.assign(
        reg("tag"),
        LlvmInst::insert(layout(), LlvmOperand::undef(), I32, tag(first), 0),
    );
    text.assign(
        reg("ptr"),
        LlvmInst::insert(layout(), LlvmOperand::reg(reg("tag")), Ptr, pointer, 1),
    );
    // Zero is also the bits of `0.0` and `FALSE`.
    text.assign(
        name.clone(),
        LlvmInst::insert(
            layout(),
            LlvmOperand::reg(reg("ptr")),
            I64,
            LlvmOperand::int(0),
            2,
        ),
    );
}

/// Writes `%{name}` = a general alternative holding `unit` when `flag` is
/// set, else `value` with payload `bits` (a runtime result such as
/// `BNMath.MODE`, `FLOAT OR NA`).
pub(crate) fn from_flag(
    text: &mut String,
    name: &str,
    flag: LlvmOperand,
    unit: &Type,
    value: &Type,
    bits: LlvmOperand,
) {
    let reg = |suffix: &str| format!("{name}_{suffix}");
    text.assign(
        reg("tagvalue"),
        LlvmInst::select(flag, I32, tag(unit), tag(value)),
    );
    text.assign(
        reg("tag"),
        LlvmInst::insert(
            layout(),
            LlvmOperand::undef(),
            I32,
            LlvmOperand::reg(reg("tagvalue")),
            0,
        ),
    );
    text.assign(
        reg("ptr"),
        LlvmInst::insert(
            layout(),
            LlvmOperand::reg(reg("tag")),
            Ptr,
            LlvmOperand::null(),
            1,
        ),
    );
    text.assign(
        name,
        LlvmInst::insert(layout(), LlvmOperand::reg(reg("ptr")), I64, bits, 2),
    );
}

/// Widening (0.6.md, "Alternative types: identity and assignment", rule 3)
/// from an alternative with a dedicated layout a general alternative can
/// contain. Each keeps a payload where the general layout keeps it, so only
/// the tag is computed:
/// - `Class OR NULL` (`ptr`): the null pointer means `NULL`, else the object.
/// - `T OR Error` (`{ i1 error, ptr, i64 }`, the form `bn_rt` returns): the
///   flag means `Error`, the `NA` / `EOF` sentinel pointer means that
///   member, and otherwise the value member is held (the unit member when
///   there is none, as in `NULL OR Error`).
fn widen_dedicated(
    text: &mut String,
    name: &str,
    operand: LlvmOperand,
    from: &Type,
    members: &[Type],
) -> LlvmOperand {
    let reg = |suffix: &str| format!("{name}_{suffix}");
    let source = crate::layout::typed_llvm(llvm_type(from).expect("validated dedicated layout"));
    if source == Ptr {
        let object = members
            .iter()
            .find(|member| pointer_member(member))
            .expect("`Class OR NULL` has a class member");
        text.assign(
            reg("null"),
            LlvmInst::icmp(ICmpCond::Eq, Ptr, operand.clone(), LlvmOperand::null()),
        );
        text.assign(
            reg("tag"),
            LlvmInst::select(
                LlvmOperand::reg(reg("null")),
                I32,
                tag(&Type::Null),
                tag(object),
            ),
        );
        text.assign(
            reg("withtag"),
            LlvmInst::insert(
                layout(),
                LlvmOperand::undef(),
                I32,
                LlvmOperand::reg(reg("tag")),
                0,
            ),
        );
        text.assign(
            reg("withptr"),
            LlvmInst::insert(layout(), LlvmOperand::reg(reg("withtag")), Ptr, operand, 1),
        );
        text.assign(
            name,
            LlvmInst::insert(
                layout(),
                LlvmOperand::reg(reg("withptr")),
                I64,
                LlvmOperand::int(0),
                2,
            ),
        );
        return LlvmOperand::reg(name);
    }
    assert_eq!(
        source,
        LlvmType::struct_of([I1, Ptr, I64]),
        "a general alternative contains only `Class OR NULL` or `T OR Error`"
    );
    let is_unit =
        |member: &&Type| matches!(member, Type::Null | Type::NotAvailable | Type::EndOfFile);
    let unit_member = members.iter().find(is_unit);
    let value_member = members
        .iter()
        .find(|member| !is_error_type(member) && !is_unit(member))
        .or(unit_member)
        .expect("dedicated layout has a member besides Error");
    let error = members
        .iter()
        .find(|member| is_error_type(member))
        .expect("Error member");
    text.assign(
        reg("error"),
        LlvmInst::extract(source.clone(), operand.clone(), 0),
    );
    text.assign(
        reg("pointer"),
        LlvmInst::extract(source.clone(), operand.clone(), 1),
    );
    text.assign(reg("payload"), LlvmInst::extract(source, operand, 2));
    let mut value_tag = tag(value_member);
    if let Some(sentinel) = Sentinel::of(members) {
        let (global, _) = sentinel.global();
        text.assign(
            reg("sentinel"),
            LlvmInst::icmp(
                ICmpCond::Eq,
                Ptr,
                LlvmOperand::reg(reg("pointer")),
                LlvmOperand::global(global.trim_start_matches('@')),
            ),
        );
        text.assign(
            reg("valuetag"),
            LlvmInst::select(
                LlvmOperand::reg(reg("sentinel")),
                I32,
                tag(unit_member.expect("sentinel member")),
                value_tag,
            ),
        );
        value_tag = LlvmOperand::reg(reg("valuetag"));
    }
    text.assign(
        reg("tag"),
        LlvmInst::select(LlvmOperand::reg(reg("error")), I32, tag(error), value_tag),
    );
    text.assign(
        reg("withtag"),
        LlvmInst::insert(
            layout(),
            LlvmOperand::undef(),
            I32,
            LlvmOperand::reg(reg("tag")),
            0,
        ),
    );
    text.assign(
        reg("withptr"),
        LlvmInst::insert(
            layout(),
            LlvmOperand::reg(reg("withtag")),
            Ptr,
            LlvmOperand::reg(reg("pointer")),
            1,
        ),
    );
    text.assign(
        name,
        LlvmInst::insert(
            layout(),
            LlvmOperand::reg(reg("withptr")),
            I64,
            LlvmOperand::reg(reg("payload")),
            2,
        ),
    );
    LlvmOperand::reg(name)
}

/// Widening from a general alternative into one with the dedicated
/// `{ i1 error, ptr, i64 }` layout (the only dedicated layout with general
/// subsets): the flag is the `Error` tag, the pointer is the `NA` / `EOF`
/// sentinel when the tag is that member, and the payloads carry over.
pub(crate) fn narrow_layout(
    text: &mut String,
    name: &str,
    operand: LlvmOperand,
    to: &[Type],
) -> LlvmOperand {
    let reg = |suffix: &str| format!("{name}_{suffix}");
    let target = LlvmType::struct_of([I1, Ptr, I64]);
    text.assign(reg("tag"), LlvmInst::extract(layout(), operand.clone(), 0));
    text.assign(
        reg("pointer"),
        LlvmInst::extract(layout(), operand.clone(), 1),
    );
    text.assign(reg("payload"), LlvmInst::extract(layout(), operand, 2));
    let error = to
        .iter()
        .find(|member| is_error_type(member))
        .expect("Error member");
    text.assign(
        reg("error"),
        LlvmInst::icmp(ICmpCond::Eq, I32, LlvmOperand::reg(reg("tag")), tag(error)),
    );
    let mut pointer = LlvmOperand::reg(reg("pointer"));
    if let Some(sentinel) = Sentinel::of(to) {
        let unit = to
            .iter()
            .find(|member| matches!(member, Type::NotAvailable | Type::EndOfFile))
            .expect("sentinel member");
        let (global, _) = sentinel.global();
        text.assign(
            reg("isunit"),
            LlvmInst::icmp(ICmpCond::Eq, I32, LlvmOperand::reg(reg("tag")), tag(unit)),
        );
        text.assign(
            reg("withsentinel"),
            LlvmInst::select(
                LlvmOperand::reg(reg("isunit")),
                Ptr,
                LlvmOperand::global(global.trim_start_matches('@')),
                pointer,
            ),
        );
        pointer = LlvmOperand::reg(reg("withsentinel"));
    }
    text.assign(
        reg("withflag"),
        LlvmInst::insert(
            target.clone(),
            LlvmOperand::undef(),
            I1,
            LlvmOperand::reg(reg("error")),
            0,
        ),
    );
    text.assign(
        reg("withptr"),
        LlvmInst::insert(
            target.clone(),
            LlvmOperand::reg(reg("withflag")),
            Ptr,
            pointer,
            1,
        ),
    );
    text.assign(
        name,
        LlvmInst::insert(
            target,
            LlvmOperand::reg(reg("withptr")),
            I64,
            LlvmOperand::reg(reg("payload")),
            2,
        ),
    );
    LlvmOperand::reg(name)
}

/// Writes `%{dest}` = the `member` a general alternative holds; the IR has
/// already proven which member that is (an `IS` test or the last store).
pub(crate) fn extract(text: &mut String, dest: &str, operand: &LlvmOperand, member: &Type) {
    let reg = |suffix: &str| format!("{dest}_{suffix}");
    let payload = || LlvmInst::extract(layout(), operand.clone(), 2);
    match member {
        Type::String => text.assign(dest, LlvmInst::extract(layout(), operand.clone(), 1)),
        _ if pointer_member(member) => {
            text.assign(dest, LlvmInst::extract(layout(), operand.clone(), 1));
        }
        _ if vector_member(member) => {
            let vec_layout = LlvmType::struct_of([Ptr, I32]);
            text.assign(
                reg("vecptr"),
                LlvmInst::extract(layout(), operand.clone(), 1),
            );
            text.assign(reg("veclen64"), payload());
            text.assign(
                reg("veclen"),
                LlvmInst::cast(CastOp::Trunc, I64, LlvmOperand::reg(reg("veclen64")), I32),
            );
            text.assign(
                reg("withptr"),
                LlvmInst::insert(
                    vec_layout.clone(),
                    LlvmOperand::undef(),
                    Ptr,
                    LlvmOperand::reg(reg("vecptr")),
                    0,
                ),
            );
            text.assign(
                dest,
                LlvmInst::insert(
                    vec_layout,
                    LlvmOperand::reg(reg("withptr")),
                    I32,
                    LlvmOperand::reg(reg("veclen")),
                    1,
                ),
            );
        }
        _ if is_error_type(member) => {
            let error = LlvmType::struct_of([I1, Ptr, I64]);
            text.assign(
                reg("record"),
                LlvmInst::extract(layout(), operand.clone(), 1),
            );
            text.assign(reg("code"), payload());
            text.assign(
                reg("flag"),
                LlvmInst::insert(
                    error.clone(),
                    LlvmOperand::undef(),
                    I1,
                    LlvmOperand::bool(true),
                    0,
                ),
            );
            text.assign(
                reg("with_record"),
                LlvmInst::insert(
                    error.clone(),
                    LlvmOperand::reg(reg("flag")),
                    Ptr,
                    LlvmOperand::reg(reg("record")),
                    1,
                ),
            );
            text.assign(
                dest,
                LlvmInst::insert(
                    error,
                    LlvmOperand::reg(reg("with_record")),
                    I64,
                    LlvmOperand::reg(reg("code")),
                    2,
                ),
            );
        }
        Type::Float(kind) => {
            text.assign(reg("bits"), payload());
            let double = if *kind == FloatType::Float32 {
                reg("double")
            } else {
                dest.to_owned()
            };
            text.assign(
                double.clone(),
                LlvmInst::cast(CastOp::BitCast, I64, LlvmOperand::reg(reg("bits")), Double),
            );
            if *kind == FloatType::Float32 {
                text.assign(
                    dest,
                    LlvmInst::cast(CastOp::FPTrunc, Double, LlvmOperand::reg(double), Float),
                );
            }
        }
        Type::Boolean => {
            text.assign(reg("bits"), payload());
            text.assign(
                dest,
                LlvmInst::icmp(
                    ICmpCond::Ne,
                    I64,
                    LlvmOperand::reg(reg("bits")),
                    LlvmOperand::int(0),
                ),
            );
        }
        Type::Integer(_) | Type::Named(_) => {
            let width = crate::layout::typed_llvm(llvm_type(member).expect("validated member"));
            if width == I64 {
                text.assign(dest, payload());
            } else {
                text.assign(reg("bits"), payload());
                text.assign(
                    dest,
                    LlvmInst::cast(CastOp::Trunc, I64, LlvmOperand::reg(reg("bits")), width),
                );
            }
        }
        Type::NotAvailable => text.assign(
            dest,
            LlvmInst::insert(
                LlvmType::struct_of([I1, Double]),
                LlvmOperand::undef(),
                I1,
                LlvmOperand::bool(true),
                0,
            ),
        ),
        Type::EndOfFile => text.assign(
            dest,
            LlvmInst::GetElementPtr {
                inbounds: false,
                elem_ty: crate::ir::LlvmType::I8,
                ptr: LlvmOperand::global(".bn_eof"),
                indices: vec![(I64, LlvmOperand::int(0))],
            },
        ),
        // NULL: the null pointer.
        _ => text.assign(
            dest,
            LlvmInst::cast(CastOp::IntToPtr, I64, LlvmOperand::int(0), Ptr),
        ),
    }
}

/// The member `IS test` names, if the alternative has it.
fn tested_member<'a>(members: &'a [Type], test: &str, test_ty: &Type) -> Option<&'a Type> {
    members.iter().find(|member| {
        *member == test_ty
            || alternative_is(member, test)
            || (is_error_type(member) && test == "Error")
    })
}

/// `%v{destination} = left IS test` on a general alternative.
pub(crate) fn emit_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    members: &[Type],
    test_name: &str,
    test_ty: &Type,
) {
    let id = destination.0;
    let reg = |suffix: &str| format!("genis{id}_{suffix}");
    text.assign(reg("tag"), LlvmInst::extract(layout(), value_reg(left), 0));
    if let Some(condition) = match test_name {
        "NAN" => Some((FCmpCond::Uno, 0.0)),
        "INF" => Some((FCmpCond::Oeq, f64::INFINITY)),
        "-INF" => Some((FCmpCond::Oeq, f64::NEG_INFINITY)),
        _ => None,
    } {
        // `IS NAN` / `IS INF`: a float member holding that value.
        text.assign(reg("bits"), LlvmInst::extract(layout(), value_reg(left), 2));
        text.assign(
            reg("double"),
            LlvmInst::cast(CastOp::BitCast, I64, LlvmOperand::reg(reg("bits")), Double),
        );
        text.assign(
            reg("value"),
            LlvmInst::fcmp(
                condition.0,
                Double,
                LlvmOperand::reg(reg("double")),
                LlvmOperand::float(condition.1),
            ),
        );
        let mut held = LlvmOperand::bool(false);
        for (index, member) in members
            .iter()
            .filter(|member| matches!(member, Type::Float(_)))
            .enumerate()
        {
            text.assign(
                reg(&format!("float{index}")),
                LlvmInst::icmp(ICmpCond::Eq, I32, LlvmOperand::reg(reg("tag")), tag(member)),
            );
            text.assign(
                reg(&format!("any{index}")),
                LlvmInst::binary(
                    crate::ir::BinaryOp::Or,
                    I1,
                    held,
                    LlvmOperand::reg(reg(&format!("float{index}"))),
                ),
            );
            held = LlvmOperand::reg(reg(&format!("any{index}")));
        }
        text.assign(
            format!("v{id}"),
            LlvmInst::binary(
                crate::ir::BinaryOp::And,
                I1,
                held,
                LlvmOperand::reg(reg("value")),
            ),
        );
        return;
    }
    let tested = tested_member(members, test_name, test_ty).map_or(LlvmOperand::int(-1), tag);
    text.assign(
        format!("v{id}"),
        LlvmInst::icmp(ICmpCond::Eq, I32, LlvmOperand::reg(reg("tag")), tested),
    );
}

pub(crate) use general_alternative_equality::{EqualsOperands, emit_equals};

/// `PRINT` of a general alternative: one branch per member, each printing
/// the member as `PRINT` prints a value of that type.
pub(crate) fn emit_print(
    text: &mut String,
    value: ValueId,
    members: &[Type],
    state: &mut EmissionState,
) {
    let n = state.continuation_count;
    state.continuation_count += 1;
    let label = |suffix: &str| format!("genprint{n}_{suffix}");
    text.assign(
        label("tag"),
        LlvmInst::extract(layout(), value_reg(value), 0),
    );
    text.emit(LlvmInst::br(label("test0")));
    for (index, member) in members.iter().enumerate() {
        state
            .control_flow
            .label(text, label(&format!("test{index}")));
        let matched = label(&format!("is{index}"));
        text.assign(
            matched.clone(),
            LlvmInst::icmp(
                ICmpCond::Eq,
                I32,
                LlvmOperand::reg(label("tag")),
                tag(member),
            ),
        );
        let next = if index + 1 == members.len() {
            label("done")
        } else {
            label(&format!("test{}", index + 1))
        };
        text.emit(LlvmInst::CondBr {
            cond: LlvmOperand::reg(matched),
            true_dest: label(&format!("print{index}")),
            false_dest: next,
        });
        state
            .control_flow
            .label(text, label(&format!("print{index}")));
        // ponytail: a synthetic value id names the extracted member, so the
        // existing per-type `PRINT` lowering prints it; real ids are dense
        // from 0 and never reach this range.
        let held = ValueId(0x8000_0000 + u32::try_from(n * 64 + index).expect("print id"));
        let unit_text = match member {
            Type::Null => Some(".bn_null"),
            Type::NotAvailable => Some(".bn_na"),
            Type::EndOfFile => Some(".bn_eof"),
            _ => None,
        };
        if let Some(global) = unit_text {
            // A unit member prints its name, as text.
            text.assign(
                format!("v{}", held.0),
                LlvmInst::GetElementPtr {
                    inbounds: false,
                    elem_ty: crate::ir::LlvmType::I8,
                    ptr: LlvmOperand::global(global),
                    indices: vec![(I64, LlvmOperand::int(0))],
                },
            );
            lower_print_value(text, held, &Type::String, state);
        } else {
            extract(text, &format!("v{}", held.0), &value_reg(value), member);
            lower_print_value(text, held, member, state);
        }
        text.emit(LlvmInst::Br {
            dest: label("done"),
        });
    }
    state.control_flow.label(text, label("done"));
}
