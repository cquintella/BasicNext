// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native `Error` values: HOST call results (`{ i1, ptr, i64 }` or `Error`
// built from a `bn_rt` status and its recorded message) and the declarations
// of the `bn_rt` error-record functions.
#![allow(clippy::wildcard_imports)]
use super::*;

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
    let _ = writeln!(text, "  %stfail{dest} = icmp ne i32 {rc}, 0");
    let (error, pointer) = if let Some(eof) = eof_status {
        let _ = writeln!(text, "  %steof{dest} = icmp eq i32 {rc}, {eof}");
        let _ = writeln!(text, "  %stnoteof{dest} = xor i1 %steof{dest}, true");
        let _ = writeln!(
            text,
            "  %sterr{dest} = and i1 %stfail{dest}, %stnoteof{dest}"
        );
        let _ = writeln!(
            text,
            "  %stvalue{dest} = select i1 %steof{dest}, ptr @.bn_eof, ptr {value_ptr}"
        );
        (format!("%sterr{dest}"), format!("%stvalue{dest}"))
    } else {
        (format!("%stfail{dest}"), value_ptr.to_string())
    };
    let _ = writeln!(text, "  %sterrint{dest} = zext i1 {error} to i32");
    let _ = writeln!(
        text,
        "  %stmsg{dest} = call ptr @bn_rt_error_take(i32 %sterrint{dest}, ptr null)"
    );
    let _ = writeln!(
        text,
        "  %stptr{dest} = select i1 {error}, ptr %stmsg{dest}, ptr {pointer}"
    );
    let _ = writeln!(
        text,
        "  %stcode{dest} = call i64 @bn_rt_error_code(ptr %stmsg{dest})\n  %stpayload{dest} = select i1 {error}, i64 %stcode{dest}, i64 {payload}"
    );
    let _ = writeln!(
        text,
        "  %stagg0{dest} = insertvalue {{ i1, ptr, i64 }} undef, i1 {error}, 0"
    );
    let _ = writeln!(
        text,
        "  %stagg1{dest} = insertvalue {{ i1, ptr, i64 }} %stagg0{dest}, ptr %stptr{dest}, 1"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = insertvalue {{ i1, ptr, i64 }} %stagg1{dest}, i64 %stpayload{dest}, 2"
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
        let _ = writeln!(text, "  %netrc{dest} = {rc_str}");
        format!("%netrc{dest}")
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
    let _ = writeln!(
        text,
        "  %bufcap{dest} = extractvalue {{ ptr, i32 }} %v{}, 1",
        buffer.0
    );
    let _ = writeln!(text, "  %bufneg{dest} = icmp slt i32 {length}, 0");
    let _ = writeln!(
        text,
        "  %bufover{dest} = icmp sgt i32 {length}, %bufcap{dest}"
    );
    let _ = writeln!(
        text,
        "  %bufbad{dest} = or i1 %bufneg{dest}, %bufover{dest}"
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
    let _ = writeln!(text, "  %eperr{dest} = icmp ne i32 {rc}, 0");
    let _ = writeln!(text, "  %eperrint{dest} = zext i1 %eperr{dest} to i32");
    let _ = writeln!(
        text,
        "  %epmsg{dest} = call ptr @bn_rt_error_take(i32 %eperrint{dest}, ptr null)"
    );
    let _ = writeln!(
        text,
        "  %epptr{dest} = select i1 %eperr{dest}, ptr %epmsg{dest}, ptr {address}"
    );
    let _ = writeln!(
        text,
        "  %epcode{dest} = call i64 @bn_rt_error_code(ptr %epmsg{dest})\n  %epcode32{dest} = trunc i64 %epcode{dest} to i32\n  %epport{dest} = select i1 %eperr{dest}, i32 %epcode32{dest}, i32 {port}"
    );
    let _ = writeln!(
        text,
        "  %epagg0{dest} = insertvalue {{ i1, ptr, i32 }} undef, i1 %eperr{dest}, 0\n  %epagg1{dest} = insertvalue {{ i1, ptr, i32 }} %epagg0{dest}, ptr %epptr{dest}, 1\n  %v{dest} = insertvalue {{ i1, ptr, i32 }} %epagg1{dest}, i32 %epport{dest}, 2"
    );
}
