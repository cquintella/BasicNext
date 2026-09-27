#![allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn lower_scalar_instruction_tail(
    text: &mut String,
    module: &Module,
    function: &Function,
    block_id: BlockId,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    symbols: &HashMap<SymbolId, usize>,
    block_state: &mut BlockState,
    state: &mut EmissionState,
) -> Result<(), String> {
    if lower_dispatch_emission(text, instruction, analysis, state) {
        return Ok(());
    }
    if lower_value_emission(
        text,
        module,
        function,
        block_id,
        instruction,
        analysis,
        symbols,
        block_state,
        state,
    ) {
        return Ok(());
    }
    if lower_ownership_emission(
        text,
        module,
        function,
        block_id,
        instruction,
        analysis,
        symbols,
        block_state,
        state,
    ) {
        return Ok(());
    }
    if lower_access_emission(
        text,
        module,
        function,
        block_id,
        instruction,
        analysis,
        symbols,
        block_state,
        state,
    ) {
        return Ok(());
    }
    if lower_print_emission(text, instruction, analysis, block_state, state) {
        return Ok(());
    }
    match instruction {
        Instruction::Call { .. } => {
            lower_call_instruction(
                text,
                module,
                function,
                block_id,
                instruction,
                analysis,
                symbols,
                block_state,
                state,
            )?;
        }
        _ => unreachable!(
            "all instructions must be validated before emission: {}",
            instruction_name(instruction)
        ),
    }
    Ok(())
}
