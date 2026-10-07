// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Calls to `BNJson` members: each lowers to one `bn_rt_json_*` call whose
// symbol is the member name in snake case.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::typed_llvm;

/// What a member returns, and so how its `bn_rt` result is packed.
#[derive(Clone, Copy)]
enum Out {
    Status,
    Text,
    Integer,
    Float,
    Boolean,
    Handle,
}

/// The arguments after the receiver (`p` STRING, `i` integer, `f` float,
/// `b` BOOLEAN, `j` child `Json`) and the result of a member, read off its
/// name: `Set<T>`, `Get<T>` and `Append<T>`, keyed by name or, with `At`,
/// by index.
fn signature(member: &str) -> Option<(bool, String, Out)> {
    let fixed = match member {
        "Clone" => Some((true, "", Out::Handle)),
        "Parse" => Some((false, "p", Out::Handle)),
        "Stringify" => Some((true, "", Out::Text)),
        _ => None,
    };
    if let Some((receiver, args, out)) = fixed {
        return Some((receiver, args.to_string(), out));
    }
    let (verb, rest) = ["Set", "Get", "Append"]
        .into_iter()
        .find_map(|verb| member.strip_prefix(verb).map(|rest| (verb, rest)))?;
    let (kind, at) = rest
        .strip_suffix("At")
        .map_or((rest, false), |kind| (kind, true));
    let (value, out) = match kind {
        "String" => ("p", Out::Text),
        "Integer" => ("i", Out::Integer),
        "Float" => ("f", Out::Float),
        "Boolean" => ("b", Out::Boolean),
        "Null" => ("", Out::Status),
        "Json" => ("j", Out::Handle),
        _ => return None,
    };
    let key = if at { "i" } else { "p" };
    Some(match verb {
        "Set" => (true, format!("{key}{value}"), Out::Status),
        "Get" if kind != "Null" => (true, key.to_string(), out),
        "Append" if !at => (true, value.to_string(), Out::Status),
        _ => return None,
    })
}

fn snake(member: &str) -> String {
    let mut name = String::new();
    for c in member.chars() {
        if c.is_ascii_uppercase() && !name.is_empty() {
            name.push('_');
        }
        name.push(c.to_ascii_lowercase());
    }
    name
}

pub(super) fn lower_bnjson_call(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    member: &str,
    arguments: &[ValueId],
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("json{name}{dest}"));
    let v = |id: ValueId| O::reg(format!("v{}", id.0));
    let h = (T::I64, r("h"));
    let symbol = format!("bn_rt_json_{}", snake(member));
    match member {
        "Array" | "Object" => {
            text.assign(format!("jsonnew{dest}"), I::call(T::I64, &symbol, vec![]));
            text.assign(
                format!("v{dest}"),
                I::cast(CastOp::IntToPtr, T::I64, r("new"), T::Ptr),
            );
            return;
        }
        "Kind" | "Has" | "Length" => {
            emit_handle_index(text, analysis, &format!("jsonh{dest}"), arguments[0]);
        }
        _ => {}
    }
    match member {
        "Kind" => text.assign(format!("v{dest}"), I::call(T::Ptr, &symbol, vec![h])),
        "Has" => {
            let args = vec![h, (T::Ptr, v(arguments[1]))];
            text.assign(format!("jsonrc{dest}"), I::call(T::I32, &symbol, args));
            text.assign(
                format!("v{dest}"),
                I::icmp(ICmpCond::Eq, T::I32, r("rc"), O::int(1)),
            );
        }
        "Length" => {
            text.assign(format!("jsonlen{dest}"), I::call(T::I64, &symbol, vec![h]));
            text.assign(
                format!("jsonerr{dest}"),
                I::icmp(ICmpCond::Slt, T::I64, r("len"), O::int(0)),
            );
            emit_scalar_result(text, dest, r("len"));
        }
        _ => {
            let (receiver, shape, out) =
                signature(member).unwrap_or_else(|| panic!("unhandled BNJson member: {member}"));
            lower_table_call(
                text,
                analysis,
                destination,
                &symbol,
                receiver,
                &shape,
                out,
                arguments,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_table_call(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    destination: ValueId,
    symbol: &str,
    receiver: bool,
    shape: &str,
    out: Out,
    arguments: &[ValueId],
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("json{name}{dest}"));
    let mut args = Vec::new();
    let mut operands = arguments.iter().copied();
    if receiver {
        let handle = operands.next().expect("BNJson receiver");
        emit_handle_index(text, analysis, &format!("jsonh{dest}"), handle);
        args.push((T::I64, r("h")));
    }
    // The second integer or float of a call (after an index) gets its own slot.
    let mut tag = "";
    for (kind, operand) in shape.chars().zip(operands) {
        let ty = analysis.values.get(&operand).and_then(llvm_type);
        let own = O::reg(format!("v{}", operand.0));
        args.push(match kind {
            'p' => (T::Ptr, own),
            'i' | 'f' => {
                let (want, slot) = if kind == 'i' {
                    ("i64", format!("jsoni64{tag}{dest}"))
                } else {
                    ("double", format!("jsonf64{tag}{dest}"))
                };
                let ty = ty.unwrap_or(want);
                let op = match (kind, ty) {
                    (_, ty) if ty == want => None,
                    ('i', _) => Some(CastOp::SExt),
                    (_, "float") => Some(CastOp::FPExt),
                    // Integer literal coerced at the call site.
                    _ => Some(CastOp::SIToFP),
                };
                tag = "val";
                let want = typed_llvm(want);
                match op {
                    None => (want, own),
                    Some(op) => {
                        text.assign(&slot, I::cast(op, typed_llvm(ty), own, want.clone()));
                        (want, O::reg(slot))
                    }
                }
            }
            'b' => {
                text.assign(
                    format!("jsonflag{dest}"),
                    I::cast(CastOp::ZExt, T::I1, own, T::I32),
                );
                (T::I32, r("flag"))
            }
            _ => {
                emit_handle_index(text, analysis, &format!("jsonhchild{dest}"), operand);
                (T::I64, r("hchild"))
            }
        });
    }
    let (slot, slot_ty) = if matches!(out, Out::Handle) {
        ("jsonout", T::I64)
    } else {
        ("jsonst", T::I32)
    };
    if !matches!(out, Out::Status) {
        text.assign(format!("{slot}{dest}"), I::alloca(slot_ty.clone()));
        args.push((T::Ptr, O::reg(format!("{slot}{dest}"))));
    }
    let (value, ret) = match out {
        Out::Status | Out::Handle => ("rc", T::I32),
        Out::Text => ("text", T::Ptr),
        Out::Integer => ("val", T::I64),
        Out::Float => ("f", T::Double),
        Out::Boolean => ("bool", T::I32),
    };
    text.assign(format!("json{value}{dest}"), I::call(ret, symbol, args));
    if let Out::Handle = out {
        text.assign(
            format!("jsonhandle{dest}"),
            I::load(T::I64, O::reg(format!("jsonout{dest}"))),
        );
        emit_handle_result(
            text,
            destination,
            format!("%jsonrc{dest}"),
            format!("%jsonhandle{dest}"),
        );
        return;
    }
    if !matches!(out, Out::Status) {
        text.assign(
            format!("jsonrc{dest}"),
            I::load(T::I32, O::reg(format!("jsonst{dest}"))),
        );
    }
    if let Out::Text = out {
        emit_string_result(text, dest);
        return;
    }
    text.assign(
        format!("jsonerr{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, r("rc"), O::int(0)),
    );
    let payload = match out {
        Out::Float => {
            text.assign(
                format!("jsonval{dest}"),
                I::cast(CastOp::BitCast, T::Double, r("f"), T::I64),
            );
            r("val")
        }
        Out::Boolean => {
            text.assign(
                format!("jsonwide{dest}"),
                I::cast(CastOp::ZExt, T::I32, r("bool"), T::I64),
            );
            r("wide")
        }
        Out::Integer => r("val"),
        _ => O::int(0),
    };
    emit_scalar_result(text, dest, payload);
}

/// Writes `%{slot}`, the `bn_rt` table index behind a handle operand
/// (`BNJson.Json`, `BNCrypto` keys). A narrowed value arrives as the
/// `{ i1, ptr, i64 }` aggregate.
pub(super) fn emit_handle_index(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    slot: &str,
    operand: ValueId,
) {
    let own = O::reg(format!("v{}", operand.0));
    let inst = if matches!(analysis.values.get(&operand), Some(Type::Alternative(_))) {
        I::extract(union(), own, 2)
    } else {
        I::cast(CastOp::PtrToInt, T::Ptr, own, T::I64)
    };
    text.assign(slot, inst);
}

fn union() -> T {
    T::struct_of([T::I1, T::Ptr, T::I64])
}

/// Packs `%jsonerr{dest}` and the error record the failure left behind into
/// the `{ i1, ptr, i64 }` head of a `T OR Error` result; returns the record.
fn emit_error_head(text: &mut String, dest: u32, record: &str) -> O {
    let r = |name: &str| O::reg(format!("json{name}{dest}"));
    text.assign(
        format!("jsonagg{dest}"),
        I::insert(union(), O::undef(), T::I1, r("err"), 0),
    );
    text.assign(
        format!("jsonerrint{dest}"),
        I::cast(CastOp::ZExt, T::I1, r("err"), T::I32),
    );
    text.assign(
        format!("json{record}{dest}"),
        I::call(
            T::Ptr,
            "bn_rt_error_take",
            vec![(T::I32, r("errint")), (T::Ptr, O::null())],
        ),
    );
    r(record)
}

/// Finishes the aggregate: slot 1 is the pointer, slot 2 the error code on
/// failure or `payload`. The caller branches on the flag, never on the
/// payload.
fn emit_result_tail(text: &mut String, dest: u32, pointer: O, payload: O) {
    let r = |name: &str| O::reg(format!("json{name}{dest}"));
    text.assign(
        format!("jsonaggp{dest}"),
        I::insert(union(), r("agg"), T::Ptr, pointer, 1),
    );
    text.assign(
        format!("jsoncode{dest}"),
        I::call(T::I64, "bn_rt_error_code", vec![(T::Ptr, r("aggpwrap"))]),
    );
    text.assign(
        format!("jsonpay{dest}"),
        I::select(r("err"), T::I64, r("code"), payload),
    );
    text.assign(
        format!("v{dest}"),
        I::insert(union(), r("aggp"), T::I64, r("pay"), 2),
    );
}

/// A `<scalar> OR Error` result from `%jsonerr{dest}` and `payload`.
fn emit_scalar_result(text: &mut String, dest: u32, payload: O) {
    let record = emit_error_head(text, dest, "aggpwrap");
    emit_result_tail(text, dest, record, payload);
}

/// A `STRING OR Error` result from `%jsontext{dest}` and `%jsonrc{dest}`.
fn emit_string_result(text: &mut String, dest: u32) {
    let r = |name: &str| O::reg(format!("json{name}{dest}"));
    text.assign(
        format!("jsonerr{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, r("rc"), O::int(0)),
    );
    let record = emit_error_head(text, dest, "fail");
    text.assign(
        format!("jsonaggpwrap{dest}"),
        I::select(r("err"), T::Ptr, record, r("text")),
    );
    emit_result_tail(text, dest, r("aggpwrap"), O::int(0));
}
