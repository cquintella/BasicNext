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
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr null, 1"
    );
    let _ = writeln!(
        text,
        "  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchvalue{d}, 2"
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
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr null, 1"
    );
    let _ = writeln!(
        text,
        "  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchbits{d}, 2"
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
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr %dispatchvalue{d}, 1"
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
    let _ = writeln!(
        text,
        "  %dispatchagg0{d} = insertvalue {{ i1, ptr, i64 }} undef, i1 %dispatcherrorflag{d}, 0"
    );
    let _ = writeln!(
        text,
        "  %dispatchagg1{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg0{d}, ptr null, 1"
    );
    let _ = writeln!(
        text,
        "  %v{d} = insertvalue {{ i1, ptr, i64 }} %dispatchagg1{d}, i64 %dispatchvalue{d}, 2"
    );
}
