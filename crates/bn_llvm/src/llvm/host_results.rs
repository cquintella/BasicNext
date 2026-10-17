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
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    rc: &O,
    eof_status: Option<u32>,
    value_ptr: &O,
    payload: &O,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    text.assign(
        format!("stfail{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, rc.clone(), O::int(0)),
    );
    let (error, pointer) = if let Some(eof) = eof_status {
        text.assign(
            format!("steof{dest}"),
            I::icmp(ICmpCond::Eq, T::I32, rc.clone(), O::int(i64::from(eof))),
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
            I::select(
                r("steof"),
                T::Ptr,
                crate::layout::string_payload(".bn_eof"),
                value_ptr.clone(),
            ),
        );
        (r("sterr"), r("stvalue"))
    } else {
        (r("stfail"), value_ptr.clone())
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
        I::select(error.clone(), T::I64, r("stcode"), payload.clone()),
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
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    rc: &O,
    handle: &O,
) {
    emit_status_result(text, destination, rc, None, &O::null(), handle);
}

/// A `VOID OR Error` result from the status `rc` a runtime call returned.
pub(crate) fn emit_void_result(text: &mut crate::ir::LlvmModule, destination: ValueId, rc: &O) {
    emit_status_result(text, destination, rc, None, &O::null(), &O::int(0));
}

/// A `VOID OR Error` result from the runtime `call` that returns the status.
pub(crate) fn emit_void_call_result(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    call: crate::ir::LlvmInst,
) {
    let status = format!("netrc{}", destination.0);
    text.assign(&status, call);
    emit_void_result(text, destination, &O::reg(status));
}

/// Declares the `bn_rt` error-record functions (`error_abi`) the module
/// calls; a module without `Error` values declares none.
pub(crate) fn declare_error_abi(text: &mut crate::ir::LlvmModule) {
    use crate::runtime_abi::{
        ERROR_CODE, ERROR_FIELD, ERROR_PRINT, ERROR_SET_REPORT, ERROR_TAKE, ERROR_WRAP,
    };
    let group: [&dyn crate::ir::Declaration; 6] = [
        &ERROR_TAKE,
        &ERROR_WRAP,
        &ERROR_FIELD,
        &ERROR_CODE,
        &ERROR_PRINT,
        &ERROR_SET_REPORT,
    ];
    let globals = text.referenced_globals();
    let called: Vec<&dyn crate::ir::Declaration> = group
        .into_iter()
        .filter(|signature| globals.contains(signature.name()))
        .collect();
    crate::ir::write_declarations(text, &called);
}

/// Traps (as an index out of bounds does) when `length` is negative or
/// larger than the BN buffer `buffer` (`{ ptr, i32 }`): the runtime writes
/// or reads `length` bytes through the buffer's pointer, and the
/// interpreter rejects the same call before any I/O.
pub(crate) fn emit_buffer_bound(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    destination: ValueId,
    buffer: ValueId,
    length: &crate::ir::LlvmOperand,
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
        I::icmp(ICmpCond::Slt, T::I32, length.clone(), O::int(0)),
    );
    text.assign(
        format!("bufover{dest}"),
        I::icmp(ICmpCond::Sgt, T::I32, length.clone(), r("bufcap")),
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
        &O::reg(format!("bufbad{dest}")),
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
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    rc: &O,
    address: &O,
    port: &O,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    text.assign(
        format!("eperr{dest}"),
        I::icmp(ICmpCond::Ne, T::I32, rc.clone(), O::int(0)),
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
        I::select(r("eperr"), T::Ptr, r("epmsg"), address.clone()),
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
        I::select(r("eperr"), T::I32, r("epcode32"), port.clone()),
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
