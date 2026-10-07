#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::{handle_result_ty, vector_ty};

fn v(id: ValueId) -> O {
    O::reg(format!("v{}", id.0))
}

/// Writes `%{slot}`, the `bn_rt` index of a handle operand (`DataFrame`,
/// `BNSqlite.Connection`): slot 2 of a `T OR Error` aggregate, or the pointer
/// bits of a plain one.
pub(crate) fn emit_handle_operand(
    text: &mut String,
    analysis: &LoweringAnalysis<'_>,
    slot: String,
    operand: ValueId,
) {
    let ty = analysis
        .values
        .get(&operand)
        .expect("validated handle operand");
    let inst = if llvm_type(ty) == Some("{ i1, ptr, i64 }") {
        I::extract(handle_result_ty(), v(operand), 2)
    } else {
        I::cast(CastOp::PtrToInt, T::Ptr, v(operand), T::I64)
    };
    text.assign(slot, inst);
}

/// The `{ i1, ptr, i64 }` of a `T OR Error` from the flag `error`: on
/// failure the error record `bn_rt` left and its code, else `ok` and 0.
/// Registers are `%{prefix}<name>{dest}`; the pointer select is `pointer`.
fn emit_flag_result(text: &mut String, prefix: &str, dest: u32, error: &O, ok: O, pointer: &str) {
    let r = |name: &str| O::reg(format!("{prefix}{name}{dest}"));
    let union = handle_result_ty();
    let wide = I::cast(CastOp::ZExt, T::I1, error.clone(), T::I32);
    text.assign(format!("{prefix}errint{dest}"), wide);
    let args = vec![(T::I32, r("errint")), (T::Ptr, O::null())];
    text.assign(
        format!("{prefix}msg{dest}"),
        I::call(T::Ptr, "bn_rt_error_take", args),
    );
    let args = vec![(T::Ptr, r("msg"))];
    text.assign(
        format!("{prefix}code{dest}"),
        I::call(T::I64, "bn_rt_error_code", args),
    );
    let chosen = I::select(error.clone(), T::Ptr, r("msg"), ok);
    text.assign(format!("{prefix}{pointer}{dest}"), chosen);
    let payload = I::select(error.clone(), T::I64, r("code"), O::int(0));
    text.assign(format!("{prefix}payload{dest}"), payload);
    let head = I::insert(union.clone(), O::undef(), T::I1, error.clone(), 0);
    text.assign(format!("{prefix}agg0{dest}"), head);
    let message = I::insert(union.clone(), r("agg0"), T::Ptr, r(pointer), 1);
    text.assign(format!("{prefix}agg1{dest}"), message);
    let full = I::insert(union, r("agg1"), T::I64, r("payload"), 2);
    text.assign(format!("v{dest}"), full);
}

pub(crate) fn lower_bndata_count(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    arguments: &[ValueId],
    symbol: &str,
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let receiver = arguments.first().expect("validated DataFrame receiver");
    let continuation = take_continuation(block_id, state);
    let r = |name: &str| O::reg(format!("dfcount_{name}{dest}"));
    emit_handle_operand(text, analysis, format!("dfcount_handle{dest}"), *receiver);
    text.assign(format!("dfcount_out{dest}"), I::alloca(T::I32));
    let args = vec![(T::I64, r("handle")), (T::Ptr, r("out"))];
    text.assign(format!("dfcount_rc{dest}"), I::call(T::I32, symbol, args));
    let ok = I::icmp(ICmpCond::Eq, T::I32, r("rc"), O::int(0));
    text.assign(format!("dfcount_ok{dest}"), ok);
    text.emit(I::CondBr {
        cond: r("ok"),
        true_dest: continuation.clone(),
        false_dest: "trap_bn_rt".into(),
    });
    state.control_flow.label(text, continuation);
    text.assign(format!("v{dest}"), I::load(T::I32, r("out")));
    state.needs_bn_rt_trap = true;
}

fn vector_length(analysis: &LoweringAnalysis<'_>, vector: ValueId) -> u64 {
    match analysis.values.get(&vector) {
        Some(Type::Vector { dimensions, .. }) => dimensions.first().copied().unwrap_or(0),
        _ => 0,
    }
}

pub(crate) fn lower_bndata_add_integer_column(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let [receiver, name, vector] = [arguments[0], arguments[1], arguments[2]];
    let length = vector_length(analysis, vector);
    let r = |name: &str| O::reg(format!("dfadd{name}{dest}"));
    let handle = I::cast(CastOp::PtrToInt, T::Ptr, v(receiver), T::I64);
    text.assign(format!("dfaddhandle{dest}"), handle);
    text.assign(
        format!("dfadddata{dest}"),
        I::extract(vector_ty(), v(vector), 0),
    );
    let args = vec![
        (T::I64, r("handle")),
        (T::Ptr, v(name)),
        (T::I32, O::uint(length)),
    ];
    let start = I::call(T::I32, "bn_rt_dataframe_add_integer_start", args);
    text.assign(format!("dfaddcolumn{dest}"), start);
    let failed = I::icmp(ICmpCond::Slt, T::I32, r("column"), O::int(0));
    text.assign(format!("dfadderror0_{dest}"), failed);
    let mut previous = O::reg(format!("dfadderror0_{dest}"));
    for index in 0..length {
        let c = |name: &str| O::reg(format!("dfadd{name}{dest}_{index}"));
        let at = vec![(T::I64, O::uint(index))];
        text.assign(
            format!("dfaddptr{dest}_{index}"),
            I::gep(T::I32, r("data"), at),
        );
        text.assign(
            format!("dfaddvalue{dest}_{index}"),
            I::load(T::I32, c("ptr")),
        );
        let wide = I::cast(CastOp::SExt, T::I32, c("value"), T::I64);
        text.assign(format!("dfaddvalue64_{dest}_{index}"), wide);
        let args = vec![
            (T::I64, r("handle")),
            (T::I32, r("column")),
            (T::I32, O::uint(index)),
            (T::I64, O::reg(format!("dfaddvalue64_{dest}_{index}"))),
        ];
        let set = I::call(T::I32, "bn_rt_dataframe_set_integer_cell", args);
        text.assign(format!("dfaddrc{dest}_{index}"), set);
        let bad = I::icmp(ICmpCond::Ne, T::I32, c("rc"), O::int(0));
        text.assign(format!("dfaddbad{dest}_{index}"), bad);
        let next = format!("dfadderror{}_{dest}", index + 1);
        text.assign(&next, I::binary(BinaryOp::Or, T::I1, previous, c("bad")));
        previous = O::reg(next);
    }
    emit_flag_result(text, "dfadd", dest, &previous, O::null(), "ptr");
}

pub(crate) fn lower_bndata_add_simple_column(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    symbol: &str,
) {
    let dest = destination.0;
    let [receiver, name, vector] = [arguments[0], arguments[1], arguments[2]];
    let length = vector_length(analysis, vector);
    let r = |name: &str| O::reg(format!("dfsimple{name}{dest}"));
    let handle = I::cast(CastOp::PtrToInt, T::Ptr, v(receiver), T::I64);
    text.assign(format!("dfsimplehandle{dest}"), handle);
    text.assign(
        format!("dfsimpledata{dest}"),
        I::extract(vector_ty(), v(vector), 0),
    );
    let args = vec![
        (T::I64, r("handle")),
        (T::Ptr, v(name)),
        (T::Ptr, r("data")),
        (T::I32, O::uint(length)),
    ];
    text.assign(format!("dfsimplerc{dest}"), I::call(T::I32, symbol, args));
    emit_void_result(text, destination, format!("%dfsimplerc{dest}"));
}

pub(crate) fn lower_bndata_column_name(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let index_value = arguments[1];
    let index = coerce_to_type(
        text,
        index_value,
        analysis
            .values
            .get(&index_value)
            .expect("validated column index"),
        &Type::Integer(IntegerType::Int32),
    );
    let r = |name: &str| O::reg(format!("dfname{name}{dest}"));
    emit_handle_operand(text, analysis, format!("dfnamehandle{dest}"), arguments[0]);
    let args = vec![(T::I64, r("handle")), (T::I32, O::raw(index))];
    let call = I::call(T::Ptr, "bn_rt_dataframe_column_name_owned", args);
    text.assign(format!("dfnameptr{dest}"), call);
    let missing = I::icmp(ICmpCond::Eq, T::Ptr, r("ptr"), O::null());
    text.assign(format!("dfnameerror{dest}"), missing);
    emit_flag_result(text, "dfname", dest, &r("error"), r("ptr"), "ptr_res");
}

pub(crate) fn lower_bndata_status_call(
    text: &mut String,
    destination: ValueId,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    symbol: &str,
) {
    let dest = destination.0;
    let receiver = arguments[0];
    let first = coerce_to_type(
        text,
        arguments[1],
        analysis
            .values
            .get(&arguments[1])
            .expect("validated DataFrame argument"),
        &Type::Integer(IntegerType::Int32),
    );
    let r = |name: &str| O::reg(format!("dfget{name}{dest}"));
    let name = arguments[2];
    let name_ty = analysis
        .values
        .get(&name)
        .expect("validated DataFrame column name");
    let name_operand = if llvm_type(name_ty) == Some("{ i1, ptr, i64 }") {
        let text_pointer = I::extract(handle_result_ty(), v(name), 1);
        text.assign(format!("dfgetname{dest}"), text_pointer);
        r("name")
    } else {
        v(name)
    };
    emit_handle_operand(text, analysis, format!("dfgethandle{dest}"), receiver);
    text.assign(format!("dfgetout{dest}"), I::alloca(T::I64));
    text.assign(format!("dfgetna{dest}"), I::alloca(T::I8));
    text.emit(I::store(T::I64, O::int(0), r("out")));
    text.emit(I::store(T::I8, O::int(0), r("na")));
    let args = vec![
        (T::I64, r("handle")),
        (T::I32, O::raw(first)),
        (T::Ptr, name_operand),
        (T::Ptr, r("out")),
        (T::Ptr, r("na")),
    ];
    text.assign(format!("dfgetrc{dest}"), I::call(T::I32, symbol, args));
    text.assign(format!("dfgetpayload{dest}"), I::load(T::I64, r("out")));
    text.assign(format!("dfgetnatag{dest}"), I::load(T::I8, r("na")));
    let is_na = I::icmp(ICmpCond::Ne, T::I8, r("natag"), O::int(0));
    text.assign(format!("dfgetisna{dest}"), is_na);
    let value_pointer = if symbol == "bn_rt_dataframe_get_string" {
        let pointer = I::cast(CastOp::IntToPtr, T::I64, r("payload"), T::Ptr);
        text.assign(format!("dfgetstring{dest}"), pointer);
        r("string")
    } else {
        O::null()
    };
    let marker = T::Array(3, Box::new(T::I8));
    let zero = vec![(T::I64, O::int(0)), (T::I64, O::int(0))];
    text.assign(
        format!("dfgetnaptr{dest}"),
        I::gep(marker, O::global(".bn_na"), zero),
    );
    let chosen = I::select(r("isna"), T::Ptr, r("naptr"), value_pointer);
    text.assign(format!("dfgetvalueptr{dest}"), chosen);
    emit_status_result(
        text,
        destination,
        &format!("%dfgetrc{dest}"),
        None,
        &format!("%dfgetvalueptr{dest}"),
        &format!("%dfgetpayload{dest}"),
    );
}
