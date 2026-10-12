#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::LlvmType;
use crate::ir::{InstSink as _, LlvmInst as I, LlvmOperand as O};

pub(crate) fn lower_indirect_call(
    text: &mut crate::ir::LlvmModule,
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
            (llvm_ty, operand)
        })
        .collect::<Vec<_>>();
    let callee_op = O::reg(format!("v{}", callee.0));
    if is_void_type(return_type) {
        text.emit(I::call_operand(LlvmType::Void, callee_op, typed_args));
    } else {
        let return_llvm = llvm_type(return_type).expect("validated indirect return");
        text.assign(
            format!("v{}", destination.0),
            I::call_operand(return_llvm, callee_op, typed_args),
        );
    }
}
