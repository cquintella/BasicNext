#![allow(clippy::wildcard_imports, clippy::too_many_lines)]
use super::*;
use crate::ir::{
    BinaryOp, CastOp, ICmpCond, InstSink as _, LlvmInst as I, LlvmOperand as O, LlvmType as T,
};
use crate::layout::handle_result_ty;

pub(crate) fn lower_dispatch_emission(
    text: &mut String,
    instruction: &Instruction,
    analysis: &LoweringAnalysis<'_>,
    _state: &mut EmissionState,
) -> bool {
    match instruction {
        Instruction::DispatchSubmit {
            destination,
            queue,
            task,
            arguments,
            ..
        } => {
            let d = destination.0;
            let r = |name: &str| O::reg(format!("dispatch{name}{d}"));
            let task_name = analysis
                .functions
                .get(task)
                .copied()
                .expect("validated async task target");
            let queue = I::extract(handle_result_ty(), O::reg(format!("v{}", queue.0)), 2);
            text.assign(format!("dispatchqueue{d}"), queue);
            text.assign(format!("dispatchticket{d}"), I::alloca(T::I64));
            let (arg_ptr, arg_count) = if arguments.is_empty() {
                (O::null(), 0)
            } else {
                let block = T::Array(arguments.len() * 16, Box::new(T::I8));
                text.assign(format!("dispatchargs{d}"), I::alloca(block.clone()));
                let zero = vec![(T::I64, O::int(0)), (T::I64, O::int(0))];
                text.assign(
                    format!("dispatchargbase{d}"),
                    I::gep(block, r("args"), zero),
                );
                for (index, argument) in arguments.iter().enumerate() {
                    let a = |name: &str| O::reg(format!("dispatcharg{name}{d}_{index}"));
                    let argument_ty = analysis
                        .values
                        .get(argument)
                        .expect("validated dispatch argument");
                    let offset = vec![(T::I64, O::uint(index as u64 * 16))];
                    text.assign(
                        format!("dispatcharg{d}_{index}"),
                        I::gep(T::I8, r("argbase"), offset),
                    );
                    let kind = match argument_ty {
                        Type::Boolean => 1,
                        Type::Integer(_) | Type::IntegerLiteral(_) => 2,
                        Type::Float(_) | Type::FloatLiteral => 3,
                        Type::String => 4,
                        _ => unreachable!("validated dispatch scalar argument"),
                    };
                    text.emit(I::store(T::I32, O::int(kind), a("")));
                    let payload = I::gep(T::I8, a(""), vec![(T::I64, O::int(8))]);
                    text.assign(format!("dispatchargpayload{d}_{index}"), payload);
                    let own = O::reg(format!("v{}", argument.0));
                    let (ty, value) = match argument_ty {
                        Type::Boolean => {
                            let wide = I::cast(CastOp::ZExt, T::I1, own, T::I64);
                            text.assign(format!("dispatchargbool{d}_{index}"), wide);
                            (T::I64, a("bool"))
                        }
                        Type::Float(_) | Type::FloatLiteral => {
                            let wide = Type::Float(FloatType::Float64);
                            let value = coerce_to_type(text, *argument, argument_ty, &wide);
                            (T::Double, O::raw(value))
                        }
                        Type::String => (T::Ptr, own),
                        _ => {
                            let wide = Type::Integer(IntegerType::Int64);
                            let value = coerce_to_type(text, *argument, argument_ty, &wide);
                            (T::I64, O::raw(value))
                        }
                    };
                    text.emit(I::store(ty, value, a("payload")));
                }
                (r("argbase"), arguments.len())
            };
            let args = vec![
                (T::I64, r("queue")),
                (T::Ptr, O::global(dispatch_trampoline_symbol(task_name))),
                (T::Ptr, O::null()),
                (T::Ptr, arg_ptr),
                (T::I32, O::uint(arg_count as u64)),
                (T::Ptr, r("ticket")),
            ];
            let submit = I::call(T::I32, "bn_rt_dispatch_submit", args);
            text.assign(format!("dispatchrc{d}"), submit);
            text.assign(format!("dispatchhandle{d}"), I::load(T::I64, r("ticket")));
            emit_handle_result(
                text,
                *destination,
                format!("%dispatchrc{d}"),
                format!("%dispatchhandle{d}"),
            );
        }
        Instruction::DispatchAwait {
            destination,
            ticket,
            timeout,
            ty,
            ..
        } => {
            let d = destination.0;
            let r = |name: &str| O::reg(format!("dispatch{name}{d}"));
            let ticket = I::extract(handle_result_ty(), O::reg(format!("v{}", ticket.0)), 2);
            text.assign(format!("dispatchticket{d}"), ticket);
            let timeout_ty = analysis
                .values
                .get(timeout)
                .expect("validated timeout type");
            let timeout = extend_to_i64(text, *timeout, timeout_ty);
            let result = T::Array(32, Box::new(T::I8));
            text.assign(format!("dispatchresult{d}"), I::alloca(result));
            text.assign(
                format!("dispatcherror{d}"),
                I::alloca(T::Array(24, Box::new(T::I8))),
            );
            let args = vec![
                (T::I64, r("ticket")),
                (T::I64, O::raw(timeout)),
                (T::Ptr, r("result")),
                (T::Ptr, r("error")),
            ];
            let call = I::call(T::I32, "bn_rt_dispatch_await", args);
            let alternatives = match ty {
                Type::Alternative(alternatives) if llvm_type(ty) == Some("{ i1, ptr, i64 }") => {
                    alternatives.as_slice()
                }
                _ => &[],
            };
            // The `bn_rt` value kind of the result and the type of its payload.
            let kind = if integer_or_error(alternatives) {
                Some((2, T::I64))
            } else if float_or_error(alternatives) {
                Some((3, T::Double))
            } else if string_or_error(alternatives) {
                Some((4, T::Ptr))
            } else if boolean_or_error(alternatives) {
                Some((1, T::I64))
            } else {
                None
            };
            match kind {
                Some((kind, payload)) => emit_dispatch_result(text, d, call, kind, &payload),
                None => emit_void_result(text, *destination, call.to_string()),
            }
        }
        _ => return false,
    }
    true
}

/// The `T OR Error` of an `AWAIT`: `%dispatchresult{d}` holds the value of
/// `kind` 8 bytes in. On failure slot 1 is the `Error` the runtime recorded
/// and slot 2 its code, as `emit_status_result` builds them.
fn emit_dispatch_result(text: &mut String, d: u32, call: I, kind: i64, payload: &T) {
    let r = |name: &str| O::reg(format!("dispatch{name}{d}"));
    let union = handle_result_ty();
    text.assign(format!("dispatchrc{d}"), call);
    text.assign(format!("dispatchkind{d}"), I::load(T::I32, r("result")));
    text.assign(
        format!("dispatchok{d}"),
        I::icmp(ICmpCond::Eq, T::I32, r("kind"), O::int(kind)),
    );
    text.assign(
        format!("dispatchrcok{d}"),
        I::icmp(ICmpCond::Eq, T::I32, r("rc"), O::int(0)),
    );
    text.assign(
        format!("dispatchgood{d}"),
        I::binary(BinaryOp::And, T::I1, r("ok"), r("rcok")),
    );
    let at = vec![(T::I64, O::int(8))];
    text.assign(
        format!("dispatchpayload{d}"),
        I::gep(T::I8, r("result"), at),
    );
    text.assign(
        format!("dispatchvalue{d}"),
        I::load(payload.clone(), r("payload")),
    );
    let value = if *payload == T::Double {
        let bits = I::cast(CastOp::BitCast, T::Double, r("value"), T::I64);
        text.assign(format!("dispatchbits{d}"), bits);
        r("bits")
    } else {
        r("value")
    };
    let flag = I::binary(BinaryOp::Xor, T::I1, r("good"), O::bool(true));
    text.assign(format!("dispatcherrorflag{d}"), flag);
    let wide = I::cast(CastOp::ZExt, T::I1, r("errorflag"), T::I32);
    text.assign(format!("dispatcherrint{d}"), wide);
    let args = vec![(T::I32, r("errint")), (T::Ptr, O::null())];
    text.assign(
        format!("dispatchfail{d}"),
        I::call(T::Ptr, "bn_rt_error_take", args),
    );
    let args = vec![(T::Ptr, r("fail"))];
    text.assign(
        format!("dispatchcode{d}"),
        I::call(T::I64, "bn_rt_error_code", args),
    );
    let head = I::insert(union.clone(), O::undef(), T::I1, r("errorflag"), 0);
    text.assign(format!("dispatchagg0{d}"), head);
    // A STRING travels in slot 1; the other kinds in slot 2.
    let (wrap, ok_pointer, ok_payload) = if *payload == T::Ptr {
        (O::reg(format!("vwrap{d}")), value, O::int(0))
    } else {
        (r("agg1wrap"), O::null(), value)
    };
    let pointer = I::select(r("errorflag"), T::Ptr, r("fail"), ok_pointer);
    text.assign(wrap.to_string(), pointer);
    text.assign(
        format!("dispatchagg1{d}"),
        I::insert(union.clone(), r("agg0"), T::Ptr, wrap, 1),
    );
    let slot = I::select(r("errorflag"), T::I64, r("code"), ok_payload);
    text.assign(format!("dispatchslot{d}"), slot);
    text.assign(
        format!("v{d}"),
        I::insert(union, r("agg1"), T::I64, r("slot"), 2),
    );
}
