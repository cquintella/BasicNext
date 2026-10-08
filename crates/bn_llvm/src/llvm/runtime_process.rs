// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Process, clock and console lowering: HOST.NumProcs, HOST.Exec,
// HOST.Clock and HOST.Console calls into `bn_rt`.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use crate::ir::{
    CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{I1, I32, I64, Ptr},
};
use crate::layout::{handle_result_ty, vector_ty};
use runtime_abi::{
    CLOCK_NOW, CLOCK_TIMER, CONSOLE_BEEP, CONSOLE_CLS, CONSOLE_NUM_COLS, CONSOLE_NUM_ROWS,
    CONSOLE_PRINT_AT, ENV_GET, ENV_HAS, ERROR_TAKE, EXEC_RESULT_CLOSE, EXEC_RESULT_RETURN_CODE,
    EXEC_RESULT_STDERR, EXEC_RESULT_STDOUT, EXEC_RUN, HOST_NUM_PROCS,
};

pub(crate) fn lower_process_call(
    text: &mut String,
    block_id: BlockId,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
    state: &mut EmissionState,
) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    match name {
        "HOST.Env.Get" => {
            text.assign(format!("envout{dest}"), LlvmInst::alloca(Ptr));
            text.assign(
                format!("envrc{dest}"),
                ENV_GET.call([value_reg(arguments[0]), reg(format!("envout{dest}"))]),
            );
            text.assign(
                format!("envval{dest}"),
                LlvmInst::load(Ptr, reg(format!("envout{dest}"))),
            );
            emit_status_result(
                text,
                destination,
                &format!("%envrc{dest}"),
                None,
                &format!("%envval{dest}"),
                "0",
            );
        }
        "HOST.Env.Has" => {
            text.assign(format!("envhasout{dest}"), LlvmInst::alloca(I32));
            text.assign(
                format!("envhasrc{dest}"),
                ENV_HAS.call([value_reg(arguments[0]), reg(format!("envhasout{dest}"))]),
            );
            text.assign(
                format!("envhasval{dest}"),
                LlvmInst::load(I32, reg(format!("envhasout{dest}"))),
            );
            text.assign(
                format!("envhasext{dest}"),
                LlvmInst::cast(CastOp::ZExt, I32, reg(format!("envhasval{dest}")), I64),
            );
            emit_status_result(
                text,
                destination,
                &format!("%envhasrc{dest}"),
                None,
                "null",
                &format!("%envhasext{dest}"),
            );
        }
        "HOST.NumProcs" => {
            text.assign(format!("numprocsout{dest}"), LlvmInst::alloca(I32));
            text.assign(
                format!("numprocsrc{dest}"),
                HOST_NUM_PROCS.call([reg(format!("numprocsout{dest}"))]),
            );
            text.assign(
                format!("numprocsval{dest}"),
                LlvmInst::load(I32, reg(format!("numprocsout{dest}"))),
            );
            text.assign(
                format!("numprocsext{dest}"),
                LlvmInst::cast(CastOp::SExt, I32, reg(format!("numprocsval{dest}")), I64),
            );
            emit_status_result(
                text,
                destination,
                &format!("%numprocsrc{dest}"),
                None,
                "null",
                &format!("%numprocsext{dest}"),
            );
        }
        "HOST.Exec.Run" => {
            let vector = vector_ty();
            text.assign(
                format!("execargs{dest}"),
                LlvmInst::extract(vector.clone(), value_reg(arguments[1]), 0),
            );
            text.assign(
                format!("execargc{dest}"),
                LlvmInst::extract(vector, value_reg(arguments[1]), 1),
            );
            text.assign(format!("execout{dest}"), LlvmInst::alloca(I64));
            text.emit(LlvmInst::store(
                I64,
                LlvmOperand::int(0),
                reg(format!("execout{dest}")),
            ));
            text.assign(
                format!("execrc{dest}"),
                EXEC_RUN.call([
                    value_reg(arguments[0]),
                    reg(format!("execargs{dest}")),
                    reg(format!("execargc{dest}")),
                    reg(format!("execout{dest}")),
                ]),
            );
            text.assign(
                format!("execresult{dest}"),
                LlvmInst::load(I64, reg(format!("execout{dest}"))),
            );
            text.assign(
                format!("execerr{dest}"),
                LlvmInst::icmp(
                    ICmpCond::Ne,
                    I32,
                    reg(format!("execrc{dest}")),
                    LlvmOperand::int(0),
                ),
            );
            text.assign(
                format!("execerrcode{dest}"),
                LlvmInst::cast(CastOp::SExt, I32, reg(format!("execrc{dest}")), I64),
            );
            text.assign(
                format!("execpayload{dest}"),
                LlvmInst::select(
                    reg(format!("execerr{dest}")),
                    I64,
                    reg(format!("execerrcode{dest}")),
                    reg(format!("execresult{dest}")),
                ),
            );
            text.assign(
                format!("execagg{dest}"),
                LlvmInst::insert(
                    handle_result_ty(),
                    LlvmOperand::undef(),
                    I1,
                    reg(format!("execerr{dest}")),
                    0,
                ),
            );
            text.assign(
                format!("execerrint{dest}"),
                LlvmInst::cast(CastOp::ZExt, I1, reg(format!("execerr{dest}")), I32),
            );
            text.assign(
                format!("execaggpwrap{dest}"),
                ERROR_TAKE.call([reg(format!("execerrint{dest}")), LlvmOperand::null()]),
            );
            text.assign(
                format!("execaggp{dest}"),
                LlvmInst::insert(
                    handle_result_ty(),
                    reg(format!("execagg{dest}")),
                    Ptr,
                    reg(format!("execaggpwrap{dest}")),
                    1,
                ),
            );
            text.assign(
                format!("v{dest}"),
                LlvmInst::insert(
                    handle_result_ty(),
                    reg(format!("execaggp{dest}")),
                    I64,
                    reg(format!("execpayload{dest}")),
                    2,
                ),
            );
        }
        "HOST.Exec.Result.ReturnCode"
        | "HOST.Exec.Result.Stdout"
        | "HOST.Exec.Result.Stderr"
        | "HOST.Exec.Result.Close" => {
            let payload = format!("execpayload{}", arguments[0].0);
            text.assign(
                payload.clone(),
                LlvmInst::extract(handle_result_ty(), value_reg(arguments[0]), 2),
            );
            let handle = [reg(payload)];
            let call = match name {
                "HOST.Exec.Result.ReturnCode" => EXEC_RESULT_RETURN_CODE.call(handle),
                "HOST.Exec.Result.Stdout" => EXEC_RESULT_STDOUT.call(handle),
                "HOST.Exec.Result.Stderr" => EXEC_RESULT_STDERR.call(handle),
                _ => EXEC_RESULT_CLOSE.call(handle),
            };
            text.assign(format!("v{dest}"), call);
        }
        "HOST.Clock.Now" => text.assign(format!("v{dest}"), CLOCK_NOW.call([])),
        "HOST.Clock.Timer" => text.assign(format!("v{dest}"), CLOCK_TIMER.call([])),
        "HOST.Console.Cls" | "HOST.Console.Beep" | "HOST.Console.PrintAt" => {
            let call = match name {
                "HOST.Console.Cls" => CONSOLE_CLS.call([]),
                "HOST.Console.Beep" => CONSOLE_BEEP.call([]),
                _ => {
                    let column = extend_to_i32(
                        text,
                        arguments[0],
                        analysis
                            .values
                            .get(&arguments[0])
                            .expect("validated column"),
                    );
                    let row = extend_to_i32(
                        text,
                        arguments[1],
                        analysis.values.get(&arguments[1]).expect("validated row"),
                    );
                    CONSOLE_PRINT_AT.call([
                        LlvmOperand::raw(column),
                        LlvmOperand::raw(row),
                        value_reg(arguments[2]),
                    ])
                }
            };
            emit_checked_i32_eq_zero(text, block_id, destination, call, CONSOLE_FAILURES, state);
        }
        "HOST.Console.NumCols" | "HOST.Console.NumRows" => {
            let call = if name.ends_with("NumCols") {
                CONSOLE_NUM_COLS.call([])
            } else {
                CONSOLE_NUM_ROWS.call([])
            };
            // The count is the value itself; a negative one is the failure.
            emit_checked_i32(
                text,
                block_id,
                destination,
                call,
                &format!("v{dest}"),
                ICmpCond::Slt,
                CONSOLE_FAILURES,
                state,
            );
        }
        _ => unreachable!("validated bn_rt host call"),
    }
}

/// The identities a `HOST.Console` runtime call can record
/// (`ConsoleError::failure`, plus the call-boundary policy re-check).
pub(crate) const CONSOLE_FAILURES: &[bn_diag::DiagId] = &[
    bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
    bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
    bn_diag::DiagId::OUTPUT_ERROR,
    bn_diag::DiagId::NUMERIC_OVERFLOW,
    bn_diag::DiagId::EXECUTION_POLICY_DENIED,
];
