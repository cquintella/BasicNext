#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{InstSink as _, LlvmInst as I, LlvmOperand as O};
use crate::layout::typed_llvm;

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
    let typed_args = arguments
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
            (typed_llvm(llvm_ty), O::raw(operand))
        })
        .collect::<Vec<_>>();
    let callee_op = O::reg(format!("v{}", callee.0));
    if is_void_type(return_type) {
        text.emit(I::call_operand(
            crate::ir::LlvmType::Void,
            callee_op,
            typed_args,
        ));
    } else {
        let return_llvm = llvm_type(return_type).expect("validated indirect return");
        text.assign(
            format!("v{}", destination.0),
            I::call_operand(typed_llvm(return_llvm), callee_op, typed_args),
        );
    }
}
