#![allow(clippy::wildcard_imports)]
use super::bndata_columns::emit_handle_operand;
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(crate) fn lower_bndata_convert(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    emit_handle_operand(text, analysis, format!("dfconvhandle{dest}"), arguments[0]);
    let args = vec![
        (T::I64, O::reg(format!("dfconvhandle{dest}"))),
        (T::Ptr, O::reg(format!("v{}", arguments[1].0))),
    ];
    text.assign(format!("dfconvrc{dest}"), I::call(T::I32, symbol, args));
    emit_void_result(text, destination, format!("%dfconvrc{dest}"));
}

pub(crate) fn lower_bndata_slice(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("df{name}{dest}"));
    let frame = O::reg(format!("v{}", arguments[0].0));
    text.assign(
        format!("dfslicehandle{dest}"),
        I::cast(CastOp::PtrToInt, T::Ptr, frame, T::I64),
    );
    let mut args = vec![(T::I64, r("slicehandle"))];
    for argument in &arguments[1..] {
        let ty = analysis
            .values
            .get(argument)
            .expect("validated slice bound");
        let bound = coerce_to_type(text, *argument, ty, &Type::Integer(IntegerType::Int32));
        args.push((T::I32, O::raw(bound)));
    }
    text.assign(format!("dfsliceout{dest}"), I::alloca(T::I64));
    args.push((T::Ptr, r("sliceout")));
    text.assign(
        format!("dfsrc{dest}"),
        I::call(T::I32, "bn_rt_dataframe_slice", args),
    );
    text.assign(
        format!("dfslicevalue{dest}"),
        I::load(T::I64, r("sliceout")),
    );
    emit_handle_result(
        text,
        destination,
        format!("%dfsrc{dest}"),
        format!("%dfslicevalue{dest}"),
    );
}
