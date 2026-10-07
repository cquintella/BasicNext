// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// BNDispatch lowering: queues, tickets and their results through `bn_rt`.
#![allow(
    clippy::wildcard_imports,
    clippy::match_same_arms,
    clippy::too_many_lines
)]
use super::*;
use crate::ir::{
    CastOp, ICmpCond, InstSink, LlvmInst, LlvmOperand,
    LlvmType::{self, I1, I8, I32, I64, Ptr},
};
use crate::layout::handle_result_ty;
use runtime_abi::{
    DISPATCH_AWAIT, DISPATCH_BARRIER_CREATE, DISPATCH_BARRIER_WAIT, DISPATCH_GROUP_CREATE,
    DISPATCH_GROUP_ENTER, DISPATCH_GROUP_LEAVE, DISPATCH_GROUP_WAIT, DISPATCH_MUTEX_CREATE,
    DISPATCH_MUTEX_LOCK, DISPATCH_MUTEX_UNLOCK, DISPATCH_QUEUE_CLOSE, DISPATCH_QUEUE_CREATE,
    DISPATCH_QUEUE_CREATE_AUTO, DISPATCH_QUEUE_JOIN, DISPATCH_SEMAPHORE_ACQUIRE,
    DISPATCH_SEMAPHORE_CREATE, DISPATCH_SEMAPHORE_RELEASE, DISPATCH_TICKET_CANCEL,
    DISPATCH_TICKET_CLOSE, DISPATCH_TICKET_ERROR, DISPATCH_TICKET_ID, DISPATCH_TICKET_IS_DONE,
    DISPATCH_TICKET_STATUS,
};

pub(crate) fn lower_bn_dispatch_call(
    text: &mut String,
    destination: ValueId,
    name: &str,
    arguments: &[ValueId],
    analysis: &LoweringAnalysis<'_>,
) {
    let dest = destination.0;
    let reg = LlvmOperand::reg;
    let argument_i64 = |text: &mut String, value: ValueId, what: &str| {
        LlvmOperand::raw(extend_to_i64(
            text,
            value,
            analysis.values.get(&value).expect(what),
        ))
    };
    if name.ends_with(".Queue.Concurrent")
        || name.ends_with(".Queue.Serial")
        || name.ends_with(".Queue.Auto")
    {
        let workers = if name.ends_with(".Queue.Concurrent") {
            let count = arguments.first().copied().expect("validated worker count");
            argument_i64(text, count, "validated worker type")
        } else {
            LlvmOperand::int(1)
        };
        let queue = reg(format!("dispatchqueue{dest}"));
        text.assign(format!("dispatchqueue{dest}"), LlvmInst::alloca(I64));
        let create = if name.ends_with(".Queue.Auto") {
            DISPATCH_QUEUE_CREATE_AUTO.call([queue.clone()])
        } else {
            DISPATCH_QUEUE_CREATE.call([workers, queue.clone()])
        };
        text.assign(format!("dispatchrc{dest}"), create);
        text.assign(format!("dispatchhandle{dest}"), LlvmInst::load(I64, queue));
        emit_handle_result(
            text,
            destination,
            format!("%dispatchrc{dest}"),
            format!("%dispatchhandle{dest}"),
        );
        return;
    }
    if [".Group.New", ".Mutex.New", ".Barrier.New", ".Semaphore.New"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        let out = reg(format!("dispatchout{dest}"));
        text.assign(format!("dispatchout{dest}"), LlvmInst::alloca(I64));
        let create = if name.ends_with(".Group.New") {
            DISPATCH_GROUP_CREATE.call([out.clone()])
        } else if name.ends_with(".Mutex.New") {
            DISPATCH_MUTEX_CREATE.call([out.clone()])
        } else {
            let value = argument_i64(text, arguments[0], "validated constructor argument");
            if name.ends_with(".Barrier.New") {
                DISPATCH_BARRIER_CREATE.call([value, out.clone()])
            } else {
                DISPATCH_SEMAPHORE_CREATE.call([value, out.clone()])
            }
        };
        text.assign(format!("dispatchrc{dest}"), create);
        text.assign(format!("dispatchcreated{dest}"), LlvmInst::load(I64, out));
        emit_handle_result(
            text,
            destination,
            format!("%dispatchrc{dest}"),
            format!("%dispatchcreated{dest}"),
        );
        return;
    }
    let handle = reg(format!("dispatchhandle{dest}"));
    text.assign(
        format!("dispatchhandle{dest}"),
        LlvmInst::extract(
            handle_result_ty(),
            value_reg(*arguments.first().expect("validated dispatch handle")),
            2,
        ),
    );
    let timeout = arguments.get(1).map_or_else(
        || LlvmOperand::int(0),
        |value| argument_i64(text, *value, "validated timeout type"),
    );
    if name.ends_with(".Ticket.Id") {
        text.assign(
            format!("dispatchid{dest}"),
            DISPATCH_TICKET_ID.call([handle]),
        );
        text.assign(
            format!("v{dest}"),
            LlvmInst::cast(CastOp::Trunc, I64, reg(format!("dispatchid{dest}")), I32),
        );
        return;
    }
    if name.ends_with(".Ticket.Status") {
        text.assign(format!("v{dest}"), DISPATCH_TICKET_STATUS.call([handle]));
        return;
    }
    if name.ends_with(".Ticket.IsDone") {
        text.assign(
            format!("dispatchdone{dest}"),
            DISPATCH_TICKET_IS_DONE.call([handle]),
        );
        text.assign(
            format!("v{dest}"),
            LlvmInst::icmp(
                ICmpCond::Ne,
                I32,
                reg(format!("dispatchdone{dest}")),
                LlvmOperand::int(0),
            ),
        );
        return;
    }
    if name.ends_with(".Ticket.Cancel") {
        let out = reg(format!("dispatchcancelout{dest}"));
        text.assign(format!("dispatchcancelout{dest}"), LlvmInst::alloca(I32));
        text.assign(
            format!("dispatchcancelrc{dest}"),
            DISPATCH_TICKET_CANCEL.call([handle, out.clone()]),
        );
        text.assign(format!("dispatchcancelval{dest}"), LlvmInst::load(I32, out));
        text.assign(
            format!("dispatchcancelsext{dest}"),
            LlvmInst::cast(
                CastOp::ZExt,
                I32,
                reg(format!("dispatchcancelval{dest}")),
                I64,
            ),
        );
        emit_status_result(
            text,
            destination,
            &format!("%dispatchcancelrc{dest}"),
            None,
            "null",
            &format!("%dispatchcancelsext{dest}"),
        );
        return;
    }
    if name.ends_with(".Ticket.Error") {
        lower_ticket_error(text, dest, handle);
        return;
    }
    if name.ends_with(".Ticket.Wait") {
        let call = DISPATCH_AWAIT.call([handle, timeout, LlvmOperand::null(), LlvmOperand::null()]);
        emit_void_result(text, destination, call.to_string());
        return;
    }
    if name.ends_with(".Barrier.Wait") {
        // `TRUE` for the last caller to arrive, through an out flag.
        let last = reg(format!("dispatchlast{dest}"));
        text.assign(format!("dispatchlast{dest}"), LlvmInst::alloca(I32));
        text.assign(
            format!("dispatchrc{dest}"),
            DISPATCH_BARRIER_WAIT.call([handle, timeout, last.clone()]),
        );
        text.assign(format!("dispatchlastv{dest}"), LlvmInst::load(I32, last));
        text.assign(
            format!("dispatchlastw{dest}"),
            LlvmInst::cast(CastOp::ZExt, I32, reg(format!("dispatchlastv{dest}")), I64),
        );
        emit_handle_result(
            text,
            destination,
            format!("%dispatchrc{dest}"),
            format!("%dispatchlastw{dest}"),
        );
        return;
    }
    // Waiting operations take the timeout; the others only the handle.
    let call = if name.ends_with(".Queue.Join") {
        DISPATCH_QUEUE_JOIN.call([handle, timeout])
    } else if name.ends_with(".Queue.Close") {
        DISPATCH_QUEUE_CLOSE.call([handle, timeout])
    } else if name.ends_with(".Group.Wait") {
        DISPATCH_GROUP_WAIT.call([handle, timeout])
    } else if name.ends_with(".Group.Enter") {
        DISPATCH_GROUP_ENTER.call([handle])
    } else if name.ends_with(".Group.Leave") {
        DISPATCH_GROUP_LEAVE.call([handle])
    } else if name.ends_with(".Semaphore.Acquire") {
        DISPATCH_SEMAPHORE_ACQUIRE.call([handle, timeout])
    } else if name.ends_with(".Semaphore.Release") {
        DISPATCH_SEMAPHORE_RELEASE.call([handle])
    } else if name.ends_with(".Mutex.Lock") {
        DISPATCH_MUTEX_LOCK.call([handle, timeout])
    } else if name.ends_with(".Mutex.Unlock") {
        DISPATCH_MUTEX_UNLOCK.call([handle])
    } else {
        DISPATCH_TICKET_CLOSE.call([handle])
    };
    emit_void_result(text, destination, call.to_string());
}

/// `Ticket.Error`: the failure message and code a ticket recorded, or
/// `N/A` and 0 when it has none.
fn lower_ticket_error(text: &mut String, dest: u32, handle: LlvmOperand) {
    let reg = LlvmOperand::reg;
    let result = handle_result_ty();
    text.assign(format!("dispatcherrmsg{dest}"), LlvmInst::alloca(Ptr));
    text.assign(format!("dispatcherrcode{dest}"), LlvmInst::alloca(I64));
    text.assign(
        format!("dispatchhaserr{dest}"),
        DISPATCH_TICKET_ERROR.call([
            handle,
            reg(format!("dispatcherrmsg{dest}")),
            reg(format!("dispatcherrcode{dest}")),
        ]),
    );
    text.assign(
        format!("dispatchiserr{dest}"),
        LlvmInst::icmp(
            ICmpCond::Ne,
            I32,
            reg(format!("dispatchhaserr{dest}")),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("dispatcherrptr{dest}"),
        LlvmInst::load(Ptr, reg(format!("dispatcherrmsg{dest}"))),
    );
    text.assign(
        format!("dispatcherrc{dest}"),
        LlvmInst::load(I64, reg(format!("dispatcherrcode{dest}"))),
    );
    text.assign(
        format!("dispatchnaptr{dest}"),
        LlvmInst::GetElementPtr {
            inbounds: false,
            elem_ty: LlvmType::Array(3, Box::new(I8)),
            ptr: LlvmOperand::global(".bn_na"),
            indices: vec![(I64, LlvmOperand::int(0)), (I64, LlvmOperand::int(0))],
        },
    );
    let failed = reg(format!("dispatchiserr{dest}"));
    text.assign(
        format!("dispatchfinalptr{dest}"),
        LlvmInst::select(
            failed.clone(),
            Ptr,
            reg(format!("dispatcherrptr{dest}")),
            reg(format!("dispatchnaptr{dest}")),
        ),
    );
    text.assign(
        format!("dispatchfinalcode{dest}"),
        LlvmInst::select(
            failed.clone(),
            I64,
            reg(format!("dispatcherrc{dest}")),
            LlvmOperand::int(0),
        ),
    );
    text.assign(
        format!("dispatchagg0{dest}"),
        LlvmInst::insert(result.clone(), LlvmOperand::undef(), I1, failed, 0),
    );
    text.assign(
        format!("dispatchagg1{dest}"),
        LlvmInst::insert(
            result.clone(),
            reg(format!("dispatchagg0{dest}")),
            Ptr,
            reg(format!("dispatchfinalptr{dest}")),
            1,
        ),
    );
    text.assign(
        format!("v{dest}"),
        LlvmInst::insert(
            result,
            reg(format!("dispatchagg1{dest}")),
            I64,
            reg(format!("dispatchfinalcode{dest}")),
            2,
        ),
    );
}
