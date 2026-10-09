#![allow(clippy::wildcard_imports)]
use super::bndata_columns::emit_handle_operand;
use super::*;
use crate::ir::{CastOp, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use crate::layout::vector_ty;

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

/// `%{slot}`: the `DataFrame` handle of `operand`, also when the receiver is
/// still typed `DataFrame OR Error` (narrowed by an `IS Error` branch).
fn emit_pointer_handle(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    slot: String,
    operand: ValueId,
) -> O {
    emit_handle_operand(text, analysis, slot.clone(), operand);
    O::reg(slot)
}

/// A `bn_rt` call that writes a new handle through its last argument: the
/// `DataFrame OR Error` result. Registers are `%{prefix}<out|rc|value>{dest}`.
fn emit_new_frame_call(
    text: &mut String,
    destination: ValueId,
    prefix: &str,
    symbol: &str,
    mut args: Vec<(T, O)>,
) {
    let dest = destination.0;
    let out = O::reg(format!("{prefix}out{dest}"));
    text.assign(format!("{prefix}out{dest}"), I::alloca(T::I64));
    args.push((T::Ptr, out.clone()));
    text.assign(format!("{prefix}rc{dest}"), I::call(T::I32, symbol, args));
    text.assign(format!("{prefix}value{dest}"), I::load(T::I64, out));
    emit_handle_result(
        text,
        destination,
        format!("%{prefix}rc{dest}"),
        format!("%{prefix}value{dest}"),
    );
}

pub(crate) fn lower_bndata_set_label(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let handle = emit_pointer_handle(text, analysis, format!("dflabelhandle{dest}"), arguments[0]);
    let args = vec![
        (T::I64, handle),
        (T::Ptr, v(arguments[1])),
        (T::Ptr, v(arguments[2])),
    ];
    let call = I::call(T::I32, "bn_rt_dataframe_set_label", args);
    text.assign(format!("dflabelrc{dest}"), call);
    emit_void_result(text, destination, format!("%dflabelrc{dest}"));
}

pub(crate) fn lower_bndata_reduce(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    operation: u32,
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("dfred{name}{dest}"));
    emit_handle_operand(text, analysis, format!("dfredhandle{dest}"), arguments[0]);
    text.assign(format!("dfredout{dest}"), I::alloca(T::Double));
    text.assign(format!("dfredna{dest}"), I::alloca(T::I8));
    let args = vec![
        (T::I64, r("handle")),
        (T::Ptr, v(arguments[1])),
        (T::I32, O::uint(u64::from(operation))),
        (T::Ptr, r("out")),
        (T::Ptr, r("na")),
    ];
    text.assign(
        format!("dfredrc{dest}"),
        I::call(T::I32, "bn_rt_dataframe_reduce", args),
    );
    text.assign(format!("dfredval{dest}"), I::load(T::Double, r("out")));
    let bits = I::cast(CastOp::BitCast, T::Double, r("val"), T::I64);
    text.assign(format!("dfredbits{dest}"), bits);
    emit_status_result(
        text,
        destination,
        &format!("%dfredrc{dest}"),
        None,
        "null",
        &format!("%dfredbits{dest}"),
    );
}

pub(crate) fn lower_bndata_zscore(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    emit_handle_operand(text, analysis, format!("dfzhandle{dest}"), arguments[0]);
    let args = vec![
        (T::I64, O::reg(format!("dfzhandle{dest}"))),
        (T::Ptr, v(arguments[1])),
    ];
    emit_new_frame_call(text, destination, "dfz", "bn_rt_dataframe_zscore", args);
}

pub(crate) fn lower_bndata_copy(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    symbol: &str,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("dfcopy{name}{dest}"));
    let length = match analysis.values.get(&arguments[2]) {
        Some(Type::Pointer {
            length: bn_types::PointerLength::Fixed(length),
            ..
        }) => *length,
        _ => 0,
    };
    emit_handle_operand(text, analysis, format!("dfcopyhandle{dest}"), arguments[0]);
    let target_ty = analysis
        .values
        .get(&arguments[2])
        .expect("validated copy target");
    let target = if llvm_type(target_ty) == Some("{ ptr, i32 }") {
        let fat = v(arguments[2]);
        text.assign(
            format!("dfcopytarget{dest}"),
            I::extract(vector_ty(), fat.clone(), 0),
        );
        text.assign(format!("dfcopylen{dest}"), I::extract(vector_ty(), fat, 1));
        r("target")
    } else {
        v(arguments[2])
    };
    let length = if length == 0 {
        r("len")
    } else {
        O::uint(length)
    };
    let args = vec![
        (T::I64, r("handle")),
        (T::Ptr, v(arguments[1])),
        (T::Ptr, target),
        (T::I32, length),
    ];
    text.assign(format!("dfcopyrc{dest}"), I::call(T::I32, symbol, args));
    emit_void_result(text, destination, format!("%dfcopyrc{dest}"));
}

pub(crate) fn lower_bndata_select(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let r = |name: &str| O::reg(format!("dfsel{name}{dest}"));
    let handle = emit_pointer_handle(text, analysis, format!("dfselhandle{dest}"), arguments[0]);
    for (name, operand) in [("rows", arguments[1]), ("cols", arguments[2])] {
        let length = if name == "rows" { "rowlen" } else { "collen" };
        text.assign(
            format!("dfsel{name}{dest}"),
            I::extract(vector_ty(), v(operand), 0),
        );
        text.assign(
            format!("dfsel{length}{dest}"),
            I::extract(vector_ty(), v(operand), 1),
        );
    }
    let args = vec![
        (T::I64, handle),
        (T::Ptr, r("rows")),
        (T::I32, r("rowlen")),
        (T::Ptr, r("cols")),
        (T::I32, r("collen")),
    ];
    emit_new_frame_call(text, destination, "dfsel", "bn_rt_dataframe_select", args);
}

pub(crate) fn lower_bndata_transform(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
    analysis: &LoweringAnalysis<'_>,
) {
    let handle = emit_pointer_handle(
        text,
        analysis,
        format!("dftrhandle{}", destination.0),
        arguments[0],
    );
    emit_new_frame_call(text, destination, "dftr", symbol, vec![(T::I64, handle)]);
}

pub(crate) fn lower_bndata_binary_transform(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let left = emit_pointer_handle(text, analysis, format!("dfbinleft{dest}"), arguments[0]);
    let right = emit_pointer_handle(text, analysis, format!("dfbinright{dest}"), arguments[1]);
    let args = vec![(T::I64, left), (T::I64, right)];
    emit_new_frame_call(text, destination, "dfbin", symbol, args);
}

pub(crate) fn lower_bndata_join(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    kind: u32,
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let left = emit_pointer_handle(text, analysis, format!("dfjoinleft{dest}"), arguments[0]);
    let right = emit_pointer_handle(text, analysis, format!("dfjoinright{dest}"), arguments[1]);
    let args = vec![
        (T::I64, left),
        (T::I64, right),
        (T::Ptr, v(arguments[2])),
        (T::Ptr, v(arguments[3])),
        (T::I32, O::uint(u64::from(kind))),
    ];
    emit_new_frame_call(text, destination, "dfjoin", "bn_rt_dataframe_join", args);
}
