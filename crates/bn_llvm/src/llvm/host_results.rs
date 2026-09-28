// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// HOST call results in native code: the `{ i1, ptr, i64 }` value or `Error`
// built from a `bn_rt` status, with the runtime's recorded message.
#![allow(clippy::wildcard_imports)]
use super::*;

/// Builds a HOST `{ i1, ptr, i64 }` result from a `bn_rt` status `rc`:
/// status 0 is the value (`value_ptr`, `payload`); `eof_status`, when given,
/// is a successful `EOF` (the `@.bn_eof` sentinel); any other status is an
/// `Error` with `Code` 1 and the message the runtime recorded for the call
/// (`bn_rt_error_message`), as in the interpreter.
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
        "  %stmsg{dest} = call ptr @bn_rt_error_message(i32 %sterrint{dest})"
    );
    let _ = writeln!(
        text,
        "  %stptr{dest} = select i1 {error}, ptr %stmsg{dest}, ptr {pointer}"
    );
    let _ = writeln!(
        text,
        "  %stpayload{dest} = select i1 {error}, i64 1, i64 {payload}"
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
    let _ = writeln!(text, "  %netrc{dest} = {}", rc.as_ref());
    emit_status_result(
        text,
        destination,
        &format!("%netrc{dest}"),
        None,
        "null",
        "0",
    );
}
