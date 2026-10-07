// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native `Load` of a local binding: use-after-release checks, conversions
// between the slot's stored representation and the loaded type, and constant
// propagation from the binding's known value.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{BinaryOp, CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::{typed_llvm, vector_ty};

/// Emits `%v<destination> = load` of `symbol` as `ty`; `checked` traps a
/// use of a binding a `RELEASE` ended (a `Take` reads it unchecked).
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(crate) fn lower_load(
    text: &mut String,
    block_id: BlockId,
    function: &Function,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    destination: ValueId,
    symbol: SymbolId,
    ty: &Type,
    checked: bool,
    state: &mut EmissionState,
) {
    let destination = &destination;
    let symbol = &symbol;
    let dest_ty = analysis
        .values
        .get(destination)
        .expect("validated loaded type");
    let slot_ty = analysis.symbols.get(symbol).unwrap_or(dest_ty);
    let dest_llvm = llvm_type(dest_ty).expect("validated load LLVM type");
    let slot_llvm = llvm_type(slot_ty).expect("validated slot LLVM type");
    let dest = destination.0;
    let slot = O::reg(format!("s{}", symbols[symbol]));
    let r = |name: &str| O::reg(format!("{name}{dest}"));
    if checked && analysis.released_symbols.contains(symbol) {
        // A second RELEASE is diagnosed by its `EndBinding`; a load is a use.
        let (id, detail) = (bn_diag::DiagId::USE_AFTER_RELEASE, "binding was released");
        text.assign(
            format!("loadlive{dest}"),
            I::load(T::I1, O::raw(live_flag(*symbol))),
        );
        let released = I::binary(BinaryOp::Xor, T::I1, r("loadlive"), O::bool(true));
        text.assign(format!("loadreleased{dest}"), released);
        let live = take_continuation(block_id, state);
        emit_trap(
            text,
            block_id,
            state,
            &format!("%loadreleased{dest}"),
            live,
            id,
            vec![("detail", Fact::Text(detail.into()))],
        );
    }
    if function.weak_symbols.contains(symbol) {
        // A weak binding stores the core id; it reads the object while it
        // lives, else NULL.
        block_state.constants.remove(destination);
        text.assign(format!("weakslot{dest}"), I::load(T::Ptr, slot));
        arc_ops::weak_read(text, &format!("v{dest}"), &format!("%weakslot{dest}"));
        return;
    }
    let own = format!("v{dest}");
    if slot_llvm == "{ i1, double }" && matches!(dest_llvm, "float" | "double") {
        let optional = T::struct_of([T::I1, T::Double]);
        text.assign(format!("optload{dest}"), I::load(optional.clone(), slot));
        let value = I::extract(optional, r("optload"), 1);
        if dest_llvm == "float" {
            text.assign(format!("optdbl{dest}"), value);
            text.assign(
                own,
                I::cast(CastOp::FPTrunc, T::Double, r("optdbl"), T::Float),
            );
        } else {
            text.assign(own, value);
        }
    } else if narrows(slot_ty, dest_ty) {
        emit_narrowed_load(text, *destination, symbols[symbol], Some(slot_ty), dest_ty);
    } else if slot_llvm != dest_llvm
        && slot_llvm == "{ i1, ptr, i32 }"
        && dest_llvm == "{ ptr, i32 }"
    {
        let endpoint = T::struct_of([T::I1, T::Ptr, T::I32]);
        text.assign(format!("netload{dest}"), I::load(endpoint.clone(), slot));
        text.assign(
            format!("netloadp{dest}"),
            I::extract(endpoint.clone(), r("netload"), 1),
        );
        text.assign(
            format!("netloadport{dest}"),
            I::extract(endpoint, r("netload"), 2),
        );
        let head = I::insert(vector_ty(), O::undef(), T::Ptr, r("netloadp"), 0);
        text.assign(format!("netloadagg{dest}"), head);
        let full = I::insert(vector_ty(), r("netloadagg"), T::I32, r("netloadport"), 1);
        text.assign(own, full);
    } else if slot_llvm != dest_llvm
        && matches!(slot_llvm, "i8" | "i16" | "i32" | "i64")
        && matches!(dest_llvm, "i8" | "i16" | "i32" | "i64")
    {
        let width = |llvm: &str| match llvm {
            "i8" => 8u8,
            "i16" => 16,
            "i32" => 32,
            _ => 64,
        };
        let (from, to) = (typed_llvm(slot_llvm), typed_llvm(dest_llvm));
        text.assign(format!("slotload{dest}"), I::load(from.clone(), slot));
        let op = if width(slot_llvm) >= width(dest_llvm) {
            CastOp::Trunc
        } else if is_unsigned(slot_ty) {
            CastOp::ZExt
        } else {
            CastOp::SExt
        };
        text.assign(own, I::cast(op, from, r("slotload"), to));
    } else {
        text.assign(own, I::load(typed_llvm(dest_llvm), slot));
    }
    if let Some(value) = block_state.bindings.get(symbol).cloned() {
        block_state
            .constants
            .insert(*destination, typed_constant(value, ty));
    } else {
        block_state.constants.remove(destination);
    }
}
