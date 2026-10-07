// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native `Error` values: HOST call results (`{ i1, ptr, i64 }` or `Error`
// built from a `bn_rt` status and its recorded message) and the declarations
// of the `bn_rt` error-record functions.
#![allow(clippy::wildcard_imports)]
use super::*;

use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};

/// Builds a HOST `{ i1, ptr, i64 }` result from a `bn_rt` status `rc`:
/// status 0 is the value (`value_ptr`, `payload`); `eof_status`, when given,
/// is a successful `EOF` (the `@.bn_eof` sentinel); any other status is an
/// `Error` with `Code` 1 and the message the runtime recorded for the call
/// (`bn_rt_error_take`), as in the interpreter.
pub(crate) fn emit_status_result(
    text: &mut String,
    destination: ValueId,
    rc: &str,
    eof_status: Option<u32>,
    value_ptr: &str,
    payload: &str,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    text.assign(
        format!("stfail{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, O::raw(rc), O::int(0)),
    );
    let (error, pointer) = if let Some(eof) = eof_status {
        text.assign(
            format!("steof{dest}"),
            I::icmp(ICmpCond::Eq, T::I32, O::raw(rc), O::int(i64::from(eof))),
        );
        text.assign(
            format!("stnoteof{dest}"),
            I::binary(BinaryOp::Xor, T::I1, r("steof"), O::bool(true)),
        );
        text.assign(
            format!("sterr{dest}"),
            I::binary(BinaryOp::And, T::I1, r("stfail"), r("stnoteof")),
        );
        text.assign(
            format!("stvalue{dest}"),
            I::select(r("steof"), T::Ptr, O::global(".bn_eof"), O::raw(value_ptr)),
        );
        (r("sterr"), r("stvalue"))
    } else {
        (r("stfail"), O::raw(value_ptr))
    };
    text.assign(
        format!("sterrint{dest}"),
        I::cast(CastOp::ZExt, T::I1, error.clone(), T::I32),
    );
    text.assign(
        format!("stmsg{dest}"),
        I::call(
            T::Ptr,
            "bn_rt_error_take",
            vec![(T::I32, r("sterrint")), (T::Ptr, O::null())],
        ),
    );
    text.assign(
        format!("stptr{dest}"),
        I::select(error.clone(), T::Ptr, r("stmsg"), pointer),
    );
    text.assign(
        format!("stcode{dest}"),
        I::call(T::I64, "bn_rt_error_code", vec![(T::Ptr, r("stmsg"))]),
    );
    text.assign(
        format!("stpayload{dest}"),
        I::select(error.clone(), T::I64, r("stcode"), O::raw(payload)),
    );
    let status_ty = T::struct_of([T::I1, T::Ptr, T::I64]);
    text.assign(
        format!("stagg0{dest}"),
        I::insert(status_ty.clone(), O::undef(), T::I1, error, 0),
    );
    text.assign(
        format!("stagg1{dest}"),
        I::insert(status_ty.clone(), r("stagg0"), T::Ptr, r("stptr"), 1),
    );
    text.assign(
        format!("v{dest}"),
        I::insert(status_ty, r("stagg1"), T::I64, r("stpayload"), 2),
    );
}

pub(crate) fn emit_handle_result(
    text: &mut String,
    destination: ValueId,
    rc: impl AsRef<str>,
    handle: impl AsRef<str>,
) {
    emit_status_result(
        text,
        destination,
        rc.as_ref(),
        None,
        "null",
        handle.as_ref(),
    );
}

pub(crate) fn emit_void_result(text: &mut String, destination: ValueId, rc: impl AsRef<str>) {
    let dest = destination.0;
    let rc_str = rc.as_ref();
    let rc_reg = if rc_str.starts_with('%') {
        rc_str.to_string()
    } else {
        let tag = format!("netrc{dest}");
        let _ = writeln!(text, "  %{tag} = {rc_str}");
        format!("%{tag}")
    };
    emit_status_result(text, destination, &rc_reg, None, "null", "0");
}

/// Declares the `bn_rt` error-record functions (`error_abi`) the module
/// calls; a module without `Error` values declares none.
pub(crate) fn declare_error_abi(text: &mut String) {
    for (symbol, declaration) in [
        (
            "@bn_rt_error_take(",
            "declare ptr @bn_rt_error_take(i32, ptr)\n",
        ),
        (
            "@bn_rt_error_wrap(",
            "declare ptr @bn_rt_error_wrap(i1, ptr, ptr)\n",
        ),
        (
            "@bn_rt_error_field(",
            "declare ptr @bn_rt_error_field(ptr, i32)\n",
        ),
        ("@bn_rt_error_code(", "declare i64 @bn_rt_error_code(ptr)\n"),
        (
            "@bn_rt_error_print(",
            "declare void @bn_rt_error_print(i64, ptr)\n",
        ),
    ] {
        if text.contains(symbol) {
            text.push_str(declaration);
        }
    }
}

/// Traps (as an index out of bounds does) when `length` is negative or
/// larger than the BN buffer `buffer` (`{ ptr, i32 }`): the runtime writes
/// or reads `length` bytes through the buffer's pointer, and the
/// interpreter rejects the same call before any I/O.
pub(crate) fn emit_buffer_bound(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    buffer: ValueId,
    length: &str,
    message: &'static str,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    let buffer_ty = T::struct_of([T::Ptr, T::I32]);
    text.assign(
        format!("bufcap{dest}"),
        I::extract(buffer_ty, O::reg(format!("v{}", buffer.0)), 1),
    );
    text.assign(
        format!("bufneg{dest}"),
        I::icmp(ICmpCond::Slt, T::I32, O::raw(length), O::int(0)),
    );
    text.assign(
        format!("bufover{dest}"),
        I::icmp(ICmpCond::Sgt, T::I32, O::raw(length), r("bufcap")),
    );
    text.assign(
        format!("bufbad{dest}"),
        I::binary(BinaryOp::Or, T::I1, r("bufneg"), r("bufover")),
    );
    let ok = take_continuation(block_id, state);
    emit_trap(
        text,
        block_id,
        state,
        &format!("%bufbad{dest}"),
        ok,
        bn_diag::DiagId::LIMIT,
        vec![("message", Fact::Text(message.into()))],
    );
}

/// Builds an `Endpoint OR Error` (`{ i1, ptr, i32 }`) from a `bn_rt` status:
/// status 0 is the endpoint (`address`, `port`); any other status is an
/// `Error` whose pointer is the recorded report (`bn_rt_error_take`) and
/// whose payload is its code, as `emit_status_result` builds for handles.
pub(crate) fn emit_endpoint_result(
    text: &mut String,
    destination: ValueId,
    rc: &str,
    address: &str,
    port: &str,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    text.assign(
        format!("eperr{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, O::raw(rc), O::int(0)),
    );
    text.assign(
        format!("eperrint{dest}"),
        I::cast(CastOp::ZExt, T::I1, r("eperr"), T::I32),
    );
    text.assign(
        format!("epmsg{dest}"),
        I::call(
            T::Ptr,
            "bn_rt_error_take",
            vec![(T::I32, r("eperrint")), (T::Ptr, O::null())],
        ),
    );
    text.assign(
        format!("epptr{dest}"),
        I::select(r("eperr"), T::Ptr, r("epmsg"), O::raw(address)),
    );
    text.assign(
        format!("epcode{dest}"),
        I::call(T::I64, "bn_rt_error_code", vec![(T::Ptr, r("epmsg"))]),
    );
    text.assign(
        format!("epcode32{dest}"),
        I::cast(CastOp::Trunc, T::I64, r("epcode"), T::I32),
    );
    text.assign(
        format!("epport{dest}"),
        I::select(r("eperr"), T::I32, r("epcode32"), O::raw(port)),
    );
    let ep_ty = T::struct_of([T::I1, T::Ptr, T::I32]);
    text.assign(
        format!("epagg0{dest}"),
        I::insert(ep_ty.clone(), O::undef(), T::I1, r("eperr"), 0),
    );
    text.assign(
        format!("epagg1{dest}"),
        I::insert(ep_ty.clone(), r("epagg0"), T::Ptr, r("epptr"), 1),
    );
    text.assign(
        format!("v{dest}"),
        I::insert(ep_ty, r("epagg1"), T::I32, r("epport"), 2),
    );
}
