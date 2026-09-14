#![allow(clippy::wildcard_imports)]
use super::*;

pub(crate) fn lower_indirect_call(
    text: &mut String,
    destination: ValueId,
    callee: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let Type::Function {
        parameters,
        return_type,
    } = analysis
        .values
        .get(&callee)
        .expect("validated indirect callee")
    else {
        unreachable!("validated indirect function type");
    };
    let operands = arguments
        .iter()
        .zip(parameters)
        .map(|(argument, parameter)| {
            let llvm_ty = llvm_type(parameter).expect("validated indirect parameter");
            let operand = coerce_to_type(
                text,
                *argument,
                analysis.values.get(argument).expect("validated argument"),
                parameter,
            );
            format!("{llvm_ty} {operand}")
        })
        .collect::<Vec<_>>()
        .join(", ");
    if is_void_type(return_type) {
        emit_void_result(
            text,
            destination,
            format!("call void %v{}({operands})", callee.0),
        );
    } else {
        let return_llvm = llvm_type(return_type).expect("validated indirect return");
        let _ = writeln!(
            text,
            "  %v{} = call {return_llvm} %v{}({operands})",
            destination.0, callee.0
        );
    }
}
