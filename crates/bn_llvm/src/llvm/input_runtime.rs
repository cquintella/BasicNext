// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// The helper functions a module defines for itself: `bn_input`, the line
// reader behind `INPUT`, and `bn_string_byte_length`, built with the typed
// builder (bucket 0.6.5 T.5.2).

use crate::ir::{
    BinaryOp, CastOp, ICmpCond, LlvmFunction, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};

/// `%name`.
fn reg(name: &str) -> O {
    O::reg(name)
}

/// `phi ty [ value, %label ], ...`.
fn phi(ty: T, incoming: &[(O, &str)]) -> I {
    let incoming = incoming
        .iter()
        .map(|(value, label)| (value.clone(), (*label).to_owned()));
    I::Phi {
        ty,
        incoming: incoming.collect(),
    }
}

/// `icmp <cond> <ty> %left, <right>`.
fn compare(cond: ICmpCond, ty: T, left: &str, right: O) -> I {
    I::icmp(cond, ty, reg(left), right)
}

/// `bn_input(reusable)`: reads one line from stdin into a growing buffer
/// (`realloc`), without its `\n` or `\r`; `@.bn_eof` at end of input; a NUL
/// byte stops the program (`bn_rt_input_nul`).
pub(crate) fn input_function() -> LlvmFunction {
    let mut function =
        LlvmFunction::new_definition("bn_input", T::Ptr, vec![(T::Ptr, "input.reusable".into())]);
    let realloc =
        |buffer: O, size: O| I::call(T::Ptr, "realloc", vec![(T::Ptr, buffer), (T::I64, size)]);
    let entry = function.add_block("entry");
    entry.push_assign("input.initial", realloc(reg("input.reusable"), O::int(64)));
    entry.push(I::br("input.read"));

    let read = function.add_block("input.read");
    // Each loop value comes from the entry, a stored byte, or a skipped `\r`.
    let carriage_or_store = |ty: T, own: &str, store: &str, first: O| {
        let incoming = [
            (first, "entry"),
            (reg(store), "input.store"),
            (reg(own), "input.carriage"),
        ];
        phi(ty, &incoming)
    };
    let buffer = carriage_or_store(
        T::Ptr,
        "input.buffer",
        "input.active.buffer",
        reg("input.initial"),
    );
    read.push_assign("input.buffer", buffer);
    let capacity = carriage_or_store(
        T::I64,
        "input.capacity",
        "input.active.capacity",
        O::int(64),
    );
    read.push_assign("input.capacity", capacity);
    let index = carriage_or_store(T::I64, "input.index", "input.next", O::int(0));
    read.push_assign("input.index", index);
    read.push_assign("input.char", I::call(T::I32, "getchar", vec![]));
    read.push_assign(
        "input.eof",
        compare(ICmpCond::Eq, T::I32, "input.char", O::int(-1)),
    );
    read.push(I::cond_br(
        reg("input.eof"),
        "input.eof.check",
        "input.line.check",
    ));

    let eof_check = function.add_block("input.eof.check");
    eof_check.push_assign(
        "input.empty",
        compare(ICmpCond::Eq, T::I64, "input.index", O::int(0)),
    );
    eof_check.push(I::cond_br(
        reg("input.empty"),
        "input.eof.out",
        "input.done",
    ));

    let line_check = function.add_block("input.line.check");
    line_check.push_assign(
        "input.newline",
        compare(ICmpCond::Eq, T::I32, "input.char", O::int(10)),
    );
    line_check.push_assign(
        "input.cr",
        compare(ICmpCond::Eq, T::I32, "input.char", O::int(13)),
    );
    line_check.push(I::cond_br(
        reg("input.newline"),
        "input.done",
        "input.cr.check",
    ));

    let cr_check = function.add_block("input.cr.check");
    cr_check.push(I::cond_br(
        reg("input.cr"),
        "input.carriage",
        "input.nul.check",
    ));

    let nul_check = function.add_block("input.nul.check");
    nul_check.push_assign(
        "input.nul",
        compare(ICmpCond::Eq, T::I32, "input.char", O::int(0)),
    );
    nul_check.push(I::cond_br(
        reg("input.nul"),
        "input.nul.stop",
        "input.store.check",
    ));

    let nul_stop = function.add_block("input.nul.stop");
    nul_stop.push(I::call(T::Void, "bn_rt_input_nul", vec![]));
    nul_stop.push(I::Unreachable);

    function
        .add_block("input.carriage")
        .push(I::br("input.read"));

    let store_check = function.add_block("input.store.check");
    let required = I::binary(BinaryOp::Add, T::I64, reg("input.index"), O::int(2));
    store_check.push_assign("input.required", required);
    let grow = compare(
        ICmpCond::Ugt,
        T::I64,
        "input.required",
        reg("input.capacity"),
    );
    store_check.push_assign("input.must.grow", grow);
    store_check.push(I::cond_br(
        reg("input.must.grow"),
        "input.grow",
        "input.keep",
    ));

    let grow = function.add_block("input.grow");
    let doubled = I::binary(BinaryOp::Shl, T::I64, reg("input.capacity"), O::int(1));
    grow.push_assign("input.new.capacity", doubled);
    grow.push_assign(
        "input.grown",
        realloc(reg("input.buffer"), reg("input.new.capacity")),
    );
    grow.push(I::br("input.store"));

    function.add_block("input.keep").push(I::br("input.store"));

    let store = function.add_block("input.store");
    let grown_or_kept = |ty: T, grown: &str, kept: &str| {
        phi(ty, &[(reg(grown), "input.grow"), (reg(kept), "input.keep")])
    };
    store.push_assign(
        "input.active.buffer",
        grown_or_kept(T::Ptr, "input.grown", "input.buffer"),
    );
    let active_capacity = grown_or_kept(T::I64, "input.new.capacity", "input.capacity");
    store.push_assign("input.active.capacity", active_capacity);
    store.push_assign(
        "input.byte",
        I::cast(CastOp::Trunc, T::I32, reg("input.char"), T::I8),
    );
    let slot = I::gep(
        T::I8,
        reg("input.active.buffer"),
        vec![(T::I64, reg("input.index"))],
    );
    store.push_assign("input.slot", slot);
    store.push(I::store(T::I8, reg("input.byte"), reg("input.slot")));
    let next = I::binary(BinaryOp::Add, T::I64, reg("input.index"), O::int(1));
    store.push_assign("input.next", next);
    store.push(I::br("input.read"));

    let done = function.add_block("input.done");
    let end = I::gep(
        T::I8,
        reg("input.buffer"),
        vec![(T::I64, reg("input.index"))],
    );
    done.push_assign("input.end", end);
    done.push(I::store(T::I8, O::int(0), reg("input.end")));
    done.push(I::Ret {
        val: Some((T::Ptr, reg("input.buffer"))),
    });

    let eof_out = function.add_block("input.eof.out");
    eof_out.push(I::call(
        T::Void,
        "free",
        vec![(T::Ptr, reg("input.buffer"))],
    ));
    eof_out.push(I::Ret {
        val: Some((T::Ptr, O::global(".bn_eof"))),
    });
    function
}

/// `bn_string_byte_length(text)`: the bytes before the NUL of `text`.
pub(crate) fn string_byte_length_function() -> LlvmFunction {
    let mut function = LlvmFunction::new_definition(
        "bn_string_byte_length",
        T::I64,
        vec![(T::Ptr, "text".into())],
    );
    function.add_block("entry").push(I::br("length.scan"));
    let scan = function.add_block("length.scan");
    let incoming = [(O::int(0), "entry"), (reg("length.next"), "length.more")];
    scan.push_assign("length.index", phi(T::I64, &incoming));
    let slot = I::gep(T::I8, reg("text"), vec![(T::I64, reg("length.index"))]);
    scan.push_assign("length.slot", slot);
    scan.push_assign("length.byte", I::load(T::I8, reg("length.slot")));
    scan.push_assign(
        "length.done",
        compare(ICmpCond::Eq, T::I8, "length.byte", O::int(0)),
    );
    scan.push(I::cond_br(reg("length.done"), "length.out", "length.more"));
    let more = function.add_block("length.more");
    let next = I::binary(BinaryOp::Add, T::I64, reg("length.index"), O::int(1));
    more.push_assign("length.next", next);
    more.push(I::br("length.scan"));
    function.add_block("length.out").push(I::Ret {
        val: Some((T::I64, reg("length.index"))),
    });
    function
}
