// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Bounds checks of BN indices under `bnc`. An index of any integer type is
//! checked whole against the length before the `i32` the element address
//! takes is used, so a wide index never wraps onto another element
//! (0.6.md: an index outside `0 .. LEN - 1` is `INDEX_OUT_OF_BOUNDS`).
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};

/// An index lowered for a bounds check: `at` is the `i32` the address uses,
/// `exact` the index's own value as `i128` (the diagnostic's fact), and
/// `fits` whether `at` is that value; `wide` says the type can hold an index
/// `at` cannot (more than 32 bits, or `UINT32`).
pub(crate) struct LoweredIndex {
    pub(crate) at: O,
    pub(crate) wide: bool,
    exact: O,
    fits: O,
}

pub(crate) fn lower_index(
    text: &mut crate::ir::LlvmModule,
    value: ValueId,
    ty: &Type,
) -> LoweredIndex {
    let (layout, bits) = llvm_type(ty)
        .and_then(|layout| layout.integer_bits().map(|bits| (layout, bits)))
        .expect("validated index type");
    let own = O::reg(format!("v{}", value.0));
    let tag = format!("{}_{}", value.0, text.fresh_tag());
    let widen = if is_unsigned(ty) {
        CastOp::ZExt
    } else {
        CastOp::SExt
    };
    let exact = I::cast(widen, layout.clone(), own.clone(), T::I128);
    text.assign(format!("ixexact{tag}"), exact);
    let at = match bits.cmp(&32) {
        std::cmp::Ordering::Equal => own,
        order => {
            let op = if order.is_lt() { widen } else { CastOp::Trunc };
            text.assign(format!("ixat{tag}"), I::cast(op, layout, own, T::I32));
            O::reg(format!("ixat{tag}"))
        }
    };
    // `at` stands for the index when reading it back gives the exact value.
    let back = I::cast(CastOp::SExt, T::I32, at.clone(), T::I128);
    text.assign(format!("ixback{tag}"), back);
    let r = |name: &str| O::reg(format!("ix{name}{tag}"));
    text.assign(
        format!("ixfits{tag}"),
        I::icmp(ICmpCond::Eq, T::I128, r("exact"), r("back")),
    );
    LoweredIndex {
        at,
        wide: bits > 32 || (bits == 32 && widen == CastOp::ZExt),
        exact: r("exact"),
        fits: r("fits"),
    }
}

/// Traps with `INDEX_OUT_OF_BOUNDS` unless `index` lies in `0 .. length`
/// (`length` an `i32`); `context` names the indexed thing as `bni` does.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_index_check(
    text: &mut crate::ir::LlvmModule,
    block_id: BlockId,
    state: &mut EmissionState,
    index: &LoweredIndex,
    length: &O,
    ok: String,
    context: &'static str,
) {
    let tag = state.continuation_count;
    let r = |name: &str| O::reg(format!("ixck{name}{tag}"));
    // Unsigned: a negative `at` compares above every length.
    let outside = I::icmp(ICmpCond::Uge, T::I32, index.at.clone(), length.clone());
    text.assign(format!("ixckoutside{tag}"), outside);
    let misfit = I::binary(BinaryOp::Xor, T::I1, index.fits.clone(), O::bool(true));
    text.assign(format!("ixckmisfit{tag}"), misfit);
    let bad = I::binary(BinaryOp::Or, T::I1, r("outside"), r("misfit"));
    text.assign(format!("ixckbad{tag}"), bad);
    let bound = I::cast(CastOp::SExt, T::I32, length.clone(), T::I128);
    text.assign(format!("ixckbound{tag}"), bound);
    emit_trap(
        text,
        block_id,
        state,
        &r("bad"),
        ok,
        bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
        vec![
            ("index", Fact::Runtime("{}", index.exact.clone())),
            ("bound", Fact::Runtime("{}", r("bound"))),
            ("context", Fact::Text(context.into())),
        ],
    );
}
