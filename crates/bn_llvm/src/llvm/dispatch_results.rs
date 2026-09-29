#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn emit_integer_dispatch_result(text: &mut String, destination: ValueId, call: &str) {
    let d = destination.0;
    let _ = writeln!(text, "  %dispatchrc{d} = {call}");
    let _ = writeln!(
        text,
        "  %dispatchkind{d} = load i32, ptr %dispatchresult{d}"
    );
    let _ = writeln!(text, "  %dispatchok{d} = icmp eq i32 %dispatchkind{d}, 2");
    let _ = writeln!(text, "  %dispatchrcok{d} = icmp eq i32 %dispatchrc{d}, 0");
    let _ = writeln!(
        text,
        "  %dispatchgood{d} = and i1 %dispatchok{d}, %dispatchrcok{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatchpayload{d} = getelementptr i8, ptr %dispatchresult{d}, i64 8"
    );
    let _ = writeln!(
        text,
        "  %dispatchvalue{d} = load i64, ptr %dispatchpayload{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatcherrorflag{d} = xor i1 %dispatchgood{d}, true"
    );
    emit_dispatch_failure(text, d);
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %dispatchagg1wrap{d} = select i1 %dispatcherrorflag{d}, ptr %dispatchfail{d}, ptr null\n  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr %dispatchagg1wrap{d}, 1"
    );
    let _ = writeln!(
        text,
        "  %dispatchslot{d} = select i1 %dispatcherrorflag{d}, i64 %dispatchcode{d}, i64 %dispatchvalue{d}\n  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchslot{d}, 2"
    );
}

pub(crate) fn emit_float_dispatch_result(text: &mut String, destination: ValueId, call: &str) {
    let d = destination.0;
    let _ = writeln!(text, "  %dispatchrc{d} = {call}");
    let _ = writeln!(
        text,
        "  %dispatchkind{d} = load i32, ptr %dispatchresult{d}"
    );
    let _ = writeln!(text, "  %dispatchok{d} = icmp eq i32 %dispatchkind{d}, 3");
    let _ = writeln!(text, "  %dispatchrcok{d} = icmp eq i32 %dispatchrc{d}, 0");
    let _ = writeln!(
        text,
        "  %dispatchgood{d} = and i1 %dispatchok{d}, %dispatchrcok{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatchpayload{d} = getelementptr i8, ptr %dispatchresult{d}, i64 8"
    );
    let _ = writeln!(
        text,
        "  %dispatchvalue{d} = load double, ptr %dispatchpayload{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatchbits{d} = bitcast double %dispatchvalue{d} to i64"
    );
    let _ = writeln!(
        text,
        "  %dispatcherrorflag{d} = xor i1 %dispatchgood{d}, true"
    );
    emit_dispatch_failure(text, d);
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %dispatchagg1wrap{d} = select i1 %dispatcherrorflag{d}, ptr %dispatchfail{d}, ptr null\n  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr %dispatchagg1wrap{d}, 1"
    );
    let _ = writeln!(
        text,
        "  %dispatchslot{d} = select i1 %dispatcherrorflag{d}, i64 %dispatchcode{d}, i64 %dispatchbits{d}\n  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchslot{d}, 2"
    );
}

pub(crate) fn emit_string_dispatch_result(text: &mut String, destination: ValueId, call: &str) {
    let d = destination.0;
    let _ = writeln!(text, "  %dispatchrc{d} = {call}");
    let _ = writeln!(
        text,
        "  %dispatchkind{d} = load i32, ptr %dispatchresult{d}"
    );
    let _ = writeln!(text, "  %dispatchok{d} = icmp eq i32 %dispatchkind{d}, 4");
    let _ = writeln!(text, "  %dispatchrcok{d} = icmp eq i32 %dispatchrc{d}, 0");
    let _ = writeln!(
        text,
        "  %dispatchgood{d} = and i1 %dispatchok{d}, %dispatchrcok{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatchpayload{d} = getelementptr i8, ptr %dispatchresult{d}, i64 8"
    );
    let _ = writeln!(
        text,
        "  %dispatchvalue{d} = load ptr, ptr %dispatchpayload{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatcherrorflag{d} = xor i1 %dispatchgood{d}, true"
    );
    emit_dispatch_failure(text, d);
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %vwrap{d} = select i1 %dispatcherrorflag{d}, ptr %dispatchfail{d}, ptr %dispatchvalue{d}\n  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr %vwrap{d}, 1\n  %dispatchslot{d} = select i1 %dispatcherrorflag{d}, i64 %dispatchcode{d}, i64 0\n  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchslot{d}, 2"
    );
}

pub(crate) fn emit_boolean_dispatch_result(text: &mut String, destination: ValueId, call: &str) {
    let d = destination.0;
    let _ = writeln!(text, "  %dispatchrc{d} = {call}");
    let _ = writeln!(
        text,
        "  %dispatchkind{d} = load i32, ptr %dispatchresult{d}"
    );
    let _ = writeln!(text, "  %dispatchok{d} = icmp eq i32 %dispatchkind{d}, 1");
    let _ = writeln!(text, "  %dispatchrcok{d} = icmp eq i32 %dispatchrc{d}, 0");
    let _ = writeln!(
        text,
        "  %dispatchgood{d} = and i1 %dispatchok{d}, %dispatchrcok{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatchpayload{d} = getelementptr i8, ptr %dispatchresult{d}, i64 8"
    );
    let _ = writeln!(
        text,
        "  %dispatchvalue{d} = load i64, ptr %dispatchpayload{d}"
    );
    let _ = writeln!(
        text,
        "  %dispatcherrorflag{d} = xor i1 %dispatchgood{d}, true"
    );
    emit_dispatch_failure(text, d);
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %dispatchagg1wrap{d} = select i1 %dispatcherrorflag{d}, ptr %dispatchfail{d}, ptr null\n  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr %dispatchagg1wrap{d}, 1"
    );
    let _ = writeln!(
        text,
        "  %dispatchslot{d} = select i1 %dispatcherrorflag{d}, i64 %dispatchcode{d}, i64 %dispatchvalue{d}\n  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchslot{d}, 2"
    );
}

/// `%dispatchfail{d}`, the recorded `Error` of a failed `AWAIT` (null on
/// success), and `%dispatchcode{d}`, its code: slot 1 and slot 2 of the
/// `{ i1, ptr, i64 }` result, as `emit_status_result` builds them.
fn emit_dispatch_failure(text: &mut String, d: u32) {
    let _ = writeln!(
        text,
        "  %dispatcherrint{d} = zext i1 %dispatcherrorflag{d} to i32\n  %dispatchfail{d} = call ptr @bn_rt_error_take(i32 %dispatcherrint{d}, ptr null)\n  %dispatchcode{d} = call i64 @bn_rt_error_code(ptr %dispatchfail{d})"
    );
}
