#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_for_condition(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let current = arguments[0];
    let end = arguments[1];
    let step = arguments[2];
    let current_type = analysis
        .values
        .get(&current)
        .expect("validated FOR current type");
    let step_type = analysis.values.get(&step).expect("validated FOR step type");
    let current_i64 = extend_to_i64(text, current, current_type);
    let end_i64 = extend_to_i64(
        text,
        end,
        analysis.values.get(&end).expect("validated FOR end type"),
    );
    let positive_opcode = integer_compare_opcode("Greater", step_type);
    let ascending_opcode = if is_unsigned(current_type) {
        "icmp ule"
    } else {
        "icmp sle"
    };
    let descending_opcode = if is_unsigned(current_type) {
        "icmp uge"
    } else {
        "icmp sge"
    };
    let step_llvm_ty = llvm_type(step_type).expect("validated FOR step type");
    let _ = writeln!(
        text,
        "  %for_step_positive{} = {positive_opcode} {step_llvm_ty} %v{}, 0",
        destination.0, step.0
    );
    let _ = writeln!(
        text,
        "  %for_ascending{} = {ascending_opcode} i64 {current_i64}, {end_i64}",
        destination.0
    );
    let _ = writeln!(
        text,
        "  %for_descending{} = {descending_opcode} i64 {current_i64}, {end_i64}",
        destination.0
    );
    let _ = writeln!(
        text,
        "  %v{} = select i1 %for_step_positive{}, i1 %for_ascending{}, i1 %for_descending{}",
        destination.0, destination.0, destination.0, destination.0
    );
}
