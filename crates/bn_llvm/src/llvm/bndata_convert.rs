#![allow(clippy::wildcard_imports)]
use super::bndata_columns::emit_handle_operand;
use super::*;
use crate::ir::{InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};

pub(crate) fn lower_bndata_convert(
    text: &mut crate::ir::LlvmModule,
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
    emit_void_result(text, destination, &O::reg(format!("dfconvrc{dest}")));
}

pub(crate) fn lower_bndata_slice(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("df{name}{dest}"));
    super::bndata_columns::emit_handle_operand(
        text,
        analysis,
        format!("dfslicehandle{dest}"),
        arguments[0],
    );
    let mut args = vec![(T::I64, r("slicehandle"))];
    for argument in &arguments[1..] {
        let ty = analysis
            .values
            .get(argument)
            .expect("validated slice bound");
        let bound = coerce_to_type(text, *argument, ty, &Type::Integer(IntegerType::Int32));
        args.push((T::I32, bound));
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
        &O::reg(format!("dfsrc{dest}")),
        &O::reg(format!("dfslicevalue{dest}")),
    );
}

/// `DataFrame.CountNA(name)` (bndata.md, "Missing values"): an
/// `INTEGER OR Error`, the count carried in the result's payload.
pub(crate) fn lower_bndata_count_na(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("dfcountna{name}{dest}"));
    emit_handle_operand(
        text,
        analysis,
        format!("dfcountnahandle{dest}"),
        arguments[0],
    );
    text.assign(format!("dfcountnaout{dest}"), I::alloca(T::I64));
    text.emit(I::store(T::I64, O::int(0), r("out")));
    let args = vec![
        (T::I64, r("handle")),
        (T::Ptr, O::reg(format!("v{}", arguments[1].0))),
        (T::Ptr, r("out")),
    ];
    text.assign(
        format!("dfcountnarc{dest}"),
        I::call(T::I32, "bn_rt_dataframe_count_na", args),
    );
    text.assign(format!("dfcountnavalue{dest}"), I::load(T::I64, r("out")));
    emit_status_result(text, destination, &r("rc"), None, &O::null(), &r("value"));
}

/// `DataFrame.FillNA(name, value)`: a `VOID OR Error`; `value` crosses as
/// a `FLOAT`.
pub(crate) fn lower_bndata_fill_na(
    text: &mut crate::ir::LlvmModule,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    emit_handle_operand(
        text,
        analysis,
        format!("dffillnahandle{dest}"),
        arguments[0],
    );
    let value = coerce_to_type(
        text,
        arguments[2],
        analysis
            .values
            .get(&arguments[2])
            .expect("validated FillNA value"),
        &Type::Float(FloatType::Float64),
    );
    let args = vec![
        (T::I64, O::reg(format!("dffillnahandle{dest}"))),
        (T::Ptr, O::reg(format!("v{}", arguments[1].0))),
        (T::Double, value),
    ];
    text.assign(
        format!("dffillnarc{dest}"),
        I::call(T::I32, "bn_rt_dataframe_fill_na", args),
    );
    emit_void_result(text, destination, &O::reg(format!("dffillnarc{dest}")));
}
