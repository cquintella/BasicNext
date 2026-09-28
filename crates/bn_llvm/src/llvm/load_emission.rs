// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native `Load` of a local binding: use-after-release checks, conversions
// between the slot's stored representation and the loaded type, and constant
// propagation from the binding's known value.
#![allow(clippy::wildcard_imports)]
use super::*;

/// Emits `%v<destination> = load` of `symbol` as `ty`.
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
    if analysis.released_symbols.contains(symbol) {
        let tag = destination.0;
        // A load that feeds RELEASE is the second RELEASE; any other is a use.
        let (id, detail) = if function
            .blocks
            .iter()
            .flat_map(|block| &block.instructions)
            .any(|instruction| matches!(instruction, Instruction::Release { value, .. } if value == destination))
        {
            (bn_diag::DiagId::DOUBLE_RELEASE, "binding was already released")
        } else {
            (bn_diag::DiagId::USE_AFTER_RELEASE, "binding was released")
        };
        let _ = writeln!(
            text,
            "  %loadlive{tag} = load i1, ptr %slive{}\n  %loadreleased{tag} = xor i1 %loadlive{tag}, true",
            symbols[symbol]
        );
        let live = take_continuation(block_id, state);
        emit_trap(
            text,
            block_id,
            state,
            &format!("%loadreleased{tag}"),
            live,
            id,
            vec![("detail", Fact::Text(detail.into()))],
        );
    }
    if slot_llvm == "{ i1, double }" && matches!(dest_llvm, "float" | "double") {
        let _ = writeln!(
            text,
            "  %optload{} = load {{ i1, double }}, ptr %s{}",
            destination.0, symbols[symbol]
        );
        if dest_llvm == "float" {
            let _ = writeln!(
                text,
                "  %optdbl{} = extractvalue {{ i1, double }} %optload{}, 1",
                destination.0, destination.0
            );
            let _ = writeln!(
                text,
                "  %v{} = fptrunc double %optdbl{} to float",
                destination.0, destination.0
            );
        } else {
            let _ = writeln!(
                text,
                "  %v{} = extractvalue {{ i1, double }} %optload{}, 1",
                destination.0, destination.0
            );
        }
    } else if narrows(slot_ty, dest_ty) {
        emit_narrowed_load(text, *destination, symbols[symbol], Some(slot_ty), dest_ty);
    } else if slot_llvm != dest_llvm
        && slot_llvm == "{ i1, ptr, i32 }"
        && dest_llvm == "{ ptr, i32 }"
    {
        let dest = destination.0;
        let _ = writeln!(
            text,
            "  %netload{dest} = load {{ i1, ptr, i32 }}, ptr %s{}",
            symbols[symbol]
        );
        let _ = writeln!(
            text,
            "  %netloadp{dest} = extractvalue {{ i1, ptr, i32 }} %netload{dest}, 1"
        );
        let _ = writeln!(
            text,
            "  %netloadport{dest} = extractvalue {{ i1, ptr, i32 }} %netload{dest}, 2"
        );
        let _ = writeln!(
            text,
            "  %netloadagg{dest} = insertvalue {{ ptr, i32 }} undef, ptr %netloadp{dest}, 0"
        );
        let _ = writeln!(
            text,
            "  %v{dest} = insertvalue {{ ptr, i32 }} %netloadagg{dest}, i32 %netloadport{dest}, 1"
        );
    } else if slot_llvm != dest_llvm
        && matches!(slot_llvm, "i8" | "i16" | "i32" | "i64")
        && matches!(dest_llvm, "i8" | "i16" | "i32" | "i64")
    {
        let _ = writeln!(
            text,
            "  %slotload{} = load {slot_llvm}, ptr %s{}",
            destination.0, symbols[symbol]
        );
        let slot_w = match slot_llvm {
            "i8" => 8u8,
            "i16" => 16,
            "i32" => 32,
            _ => 64,
        };
        let dest_w = match dest_llvm {
            "i8" => 8u8,
            "i16" => 16,
            "i32" => 32,
            _ => 64,
        };
        let opcode = if slot_w < dest_w {
            if is_unsigned(slot_ty) { "zext" } else { "sext" }
        } else {
            "trunc"
        };
        let _ = writeln!(
            text,
            "  %v{} = {opcode} {slot_llvm} %slotload{} to {dest_llvm}",
            destination.0, destination.0
        );
    } else {
        let _ = writeln!(
            text,
            "  %v{} = load {dest_llvm}, ptr %s{}",
            destination.0, symbols[symbol]
        );
    }
    if let Some(value) = block_state.bindings.get(symbol).cloned() {
        block_state
            .constants
            .insert(*destination, typed_constant(value, ty));
    } else {
        block_state.constants.remove(destination);
    }
}
