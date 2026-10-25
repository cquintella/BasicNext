// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNDispatch` — an external library module served through the provider
//! seam. Owns queues, tickets and sync primitives; a submitted task runs in
//! an isolated executor over a clone of the module and a forked host.

#![allow(
    clippy::too_many_lines,
    clippy::needless_return,
    clippy::ignored_unit_patterns
)] // Moved verbatim from the core (bucket 0.5.1d); one arm per BNDispatch member.

mod dispatch;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_stdlib::dispatch_error::DispatchFailure;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    integer_pub as integer, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};

pub const NAME: &str = "BNDispatch";

struct BoundedTaskOutput {
    bytes: Vec<u8>,
    maximum: usize,
}

impl std::io::Write for BoundedTaskOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let remaining = self.maximum.saturating_sub(self.bytes.len());
        if bytes.len() > remaining {
            return Err(std::io::Error::new(
                std::io::ErrorKind::WriteZero,
                "async task output exceeds configured bound",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Default)]
struct SharedDispatchState {
    queues: HashMap<u64, crate::dispatch::Queue>,
    next_queue: u64,
    tickets: HashMap<u64, crate::dispatch::Ticket>,
    next_ticket: u64,
    groups: HashMap<u64, Arc<crate::dispatch::DispatchGroup>>,
    barriers: HashMap<u64, Arc<crate::dispatch::Barrier>>,
    semaphores: HashMap<u64, Arc<crate::dispatch::DispatchSemaphore>>,
    mutexes: HashMap<u64, Arc<crate::dispatch::DispatchMutex>>,
    next_sync: u64,
}

impl SharedDispatchState {
    fn new() -> Self {
        Self {
            queues: HashMap::new(),
            next_queue: 1,
            tickets: HashMap::new(),
            next_ticket: 1,
            groups: HashMap::new(),
            barriers: HashMap::new(),
            semaphores: HashMap::new(),
            mutexes: HashMap::new(),
            next_sync: 1,
        }
    }
}

static SHARED_DISPATCH: OnceLock<Mutex<SharedDispatchState>> = OnceLock::new();

fn shared_dispatch() -> &'static Mutex<SharedDispatchState> {
    SHARED_DISPATCH.get_or_init(|| Mutex::new(SharedDispatchState::new()))
}

fn unlock_held_mutexes_in_shared_dispatch() {
    let thread_id = std::thread::current().id();
    let mutexes: Vec<Arc<crate::dispatch::DispatchMutex>> = {
        let state = shared_dispatch()
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.mutexes.values().cloned().collect()
    };
    for mutex in mutexes {
        mutex.unlock_if_held_by(thread_id);
    }
}

#[derive(Default, Debug)]
pub struct DispatchProvider;

impl Provider for DispatchProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = format!("BNDispatch.{member}");
        self.dispatch_call(core, &name, &arguments, span)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl DispatchProvider {
    /// The core queue behind the `Dispatch.Queue` handle `id`, for a `BNWeb`
    /// server that runs its handlers on it (`Server.UseQueue`).
    #[must_use]
    pub fn core_queue(&self, id: u64) -> Option<bn_core_dispatch::Queue> {
        let state = shared_dispatch()
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.queues.get(&id).map(crate::dispatch::Queue::core)
    }

    fn dispatch_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        match method {
            "New" | "Create" if name.contains(".Group.") => {
                require_arity(name, arguments, 0, span)?;
                let mut state = shared_dispatch()
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let id = state.next_sync;
                state.next_sync = state.next_sync.saturating_add(1);
                state
                    .groups
                    .insert(id, Arc::new(crate::dispatch::DispatchGroup::new()));
                Ok(Value::DispatchGroup(id))
            }
            "Enter" | "Leave" | "Wait" if name.contains(".Group.") => {
                let Value::DispatchGroup(id) = arguments.first().cloned().unwrap_or(Value::Null)
                else {
                    return Err(type_mismatch(
                        "Group",
                        "non-Group value",
                        "dispatch group",
                        span,
                    ));
                };
                let group = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.groups.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "group is invalid", span)
                    })?
                };
                match method {
                    "Enter" => {
                        require_arity(name, arguments, 1, span)?;
                        group.enter();
                        Ok(Value::Null)
                    }
                    "Leave" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(group
                            .leave()
                            .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
                    }
                    _ => {
                        require_arity(name, arguments, 2, span)?;
                        Ok(group
                            .wait(integer(&arguments[1], span)?.0)
                            .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
                    }
                }
            }
            "New" | "Create" if name.contains(".Barrier.") => {
                require_arity(name, arguments, 1, span)?;
                let parties = integer(&arguments[0], span)?.0;
                let barrier = match crate::dispatch::Barrier::new(parties) {
                    Ok(barrier) => barrier,
                    Err(failure) => return Ok(failed(name, &failure)),
                };
                let mut state = shared_dispatch()
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let id = state.next_sync;
                state.next_sync = state.next_sync.saturating_add(1);
                state.barriers.insert(id, Arc::new(barrier));
                Ok(Value::DispatchBarrier(id))
            }
            "Wait" if name.contains(".Barrier.") => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchBarrier(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "Barrier",
                        "non-Barrier value",
                        "dispatch barrier",
                        span,
                    ));
                };
                let barrier = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.barriers.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "barrier is invalid", span)
                    })?
                };
                Ok(barrier
                    .wait(integer(&arguments[1], span)?.0)
                    .map_or_else(|failure| failed(name, &failure), Value::Boolean))
            }
            "New" | "Create" if name.contains(".Semaphore.") => {
                require_arity(name, arguments, 1, span)?;
                let permits = integer(&arguments[0], span)?.0;
                let semaphore = match crate::dispatch::DispatchSemaphore::new(permits) {
                    Ok(semaphore) => semaphore,
                    Err(failure) => return Ok(failed(name, &failure)),
                };
                let mut state = shared_dispatch()
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let id = state.next_sync;
                state.next_sync = state.next_sync.saturating_add(1);
                state.semaphores.insert(id, Arc::new(semaphore));
                Ok(Value::DispatchSemaphore(id))
            }
            "Acquire" | "Release" if name.contains(".Semaphore.") => {
                let Value::DispatchSemaphore(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "Semaphore",
                        "non-Semaphore value",
                        "dispatch semaphore",
                        span,
                    ));
                };
                let semaphore = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.semaphores.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "semaphore is invalid", span)
                    })?
                };
                if method == "Acquire" {
                    require_arity(name, arguments, 2, span)?;
                    Ok(semaphore
                        .acquire(integer(&arguments[1], span)?.0)
                        .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
                } else {
                    require_arity(name, arguments, 1, span)?;
                    Ok(semaphore
                        .release()
                        .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
                }
            }
            "New" | "Create" if name.contains(".Mutex.") => {
                require_arity(name, arguments, 0, span)?;
                let mut state = shared_dispatch()
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                let id = state.next_sync;
                state.next_sync = state.next_sync.saturating_add(1);
                state
                    .mutexes
                    .insert(id, Arc::new(crate::dispatch::DispatchMutex::new()));
                Ok(Value::DispatchMutex(id))
            }
            "Lock" | "Unlock" if name.contains(".Mutex.") => {
                let Value::DispatchMutex(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "Mutex",
                        "non-Mutex value",
                        "dispatch mutex",
                        span,
                    ));
                };
                let mutex = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.mutexes.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "mutex is invalid", span)
                    })?
                };
                if method == "Lock" {
                    require_arity(name, arguments, 2, span)?;
                    Ok(mutex
                        .lock(integer(&arguments[1], span)?.0)
                        .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
                } else {
                    require_arity(name, arguments, 1, span)?;
                    Ok(mutex
                        .unlock()
                        .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
                }
            }
            "Serial" => {
                require_arity(name, arguments, 0, span)?;
                Ok(self.dispatch_queue(name, 1))
            }
            "Concurrent" => {
                require_arity(name, arguments, 1, span)?;
                let (workers, _) = integer(&arguments[0], span)?;
                Ok(self.dispatch_queue(name, workers))
            }
            "Auto" => {
                require_arity(name, arguments, 0, span)?;
                Ok(match bn_stdlib::dispatch_sync::auto_worker_count() {
                    Ok(workers) => {
                        self.dispatch_queue(name, i128::try_from(workers).expect("usize fits i128"))
                    }
                    Err(failure) => failed(name, &failure),
                })
            }
            "Async" => {
                if arguments.len() < 2 {
                    return Err(type_mismatch(
                        "Queue, FUNCTION",
                        "insufficient arguments",
                        "Async",
                        span,
                    ));
                }
                let Value::DispatchQueue(id) = arguments[0] else {
                    return Err(type_mismatch("Queue", "non-Queue value", "Async", span));
                };
                let Value::Function(task) = &arguments[1] else {
                    return Ok(Value::error(1, "Async expects a named function".into()));
                };
                let queue = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.queues.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "queue is invalid", span)
                    })?
                };
                let task_name = task.clone();
                let task_arguments = arguments[2..].to_vec();
                let worker_module = core.module().clone();
                let worker_host = core.host().fork_for_task();
                let ticket = queue.submit_with(task_name.to_string(), move |ticket| {
                    let mut input = std::io::Cursor::new(Vec::<u8>::new());
                    let mut output = BoundedTaskOutput {
                        bytes: Vec::new(),
                        maximum: bn_limits::dispatch_limits().output_max_bytes,
                    };
                    let outcome = bn_interp::execute_named_with_host(
                        &worker_module,
                        &task_name,
                        task_arguments,
                        &mut input,
                        &mut output,
                        &worker_host,
                    );
                    bn_stdlib::unlock_held_mutexes_for_current_thread();
                    unlock_held_mutexes_in_shared_dispatch();
                    match outcome {
                        Ok(result) => {
                            let output = String::from_utf8_lossy(&output.bytes).into_owned();
                            if ticket.set_output(output).is_err() {
                                ticket.mark_failed(
                                    1,
                                    "async task output exceeds configured bound".into(),
                                );
                            } else if let Value::Error { code, message, .. } = &result {
                                // TASK_FAILED; `ticket.Error()` keeps the worker's Error.
                                let failure = (*code, message.to_string());
                                ticket.set_result(result);
                                ticket.mark_failed(failure.0, failure.1);
                            } else {
                                ticket.set_result(result);
                                ticket.mark_completed();
                            }
                        }
                        Err(error) => ticket.mark_failed(1, error.message.to_string()),
                    }
                });
                let ticket = match ticket {
                    Ok(ticket) => ticket,
                    Err(failure) => return Ok(failed(name, &failure)),
                };
                let ticket_id = {
                    let mut state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    let id = state.next_ticket;
                    state.next_ticket = state.next_ticket.saturating_add(1);
                    state.tickets.insert(id, ticket);
                    id
                };
                Ok(Value::DispatchTicket(ticket_id))
            }
            "Join" if name.contains(".Queue.") => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchQueue(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "Queue",
                        "non-Queue value",
                        "dispatch operation",
                        span,
                    ));
                };
                let timeout = integer(&arguments[1], span)?.0;
                let queue = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.queues.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "queue is invalid", span)
                    })?
                };
                let tickets = queue.tickets();
                let result = queue.join(timeout);
                for ticket in tickets {
                    let output = ticket.take_output();
                    core.output()
                        .write_all(output.as_bytes())
                        .map_err(|error| {
                            runtime_error(bn_diag::DiagId::IO, error.to_string(), span)
                        })?;
                }
                Ok(result.map_or_else(|failure| failed(name, &failure), |_| Value::Null))
            }
            "Close" if name.contains(".Queue.") => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchQueue(id) = arguments[0] else {
                    return Err(type_mismatch(
                        "Queue",
                        "non-Queue value",
                        "dispatch operation",
                        span,
                    ));
                };
                let timeout = integer(&arguments[1], span)?.0;
                let queue = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.queues.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "queue is invalid", span)
                    })?
                };
                Ok(queue
                    .close(timeout)
                    .map_or_else(|failure| failed(name, &failure), |_| Value::Null))
            }
            "Id" | "Status" | "Wait" | "Cancel" | "Error" | "IsDone" | "Close"
                if name.contains(".Ticket.") =>
            {
                let Value::DispatchTicket(id) = arguments.first().cloned().unwrap_or(Value::Null)
                else {
                    return Err(type_mismatch(
                        "Ticket",
                        "non-Ticket value",
                        "dispatch ticket",
                        span,
                    ));
                };
                let ticket = {
                    let state = shared_dispatch()
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner);
                    state.tickets.get(&id).cloned().ok_or_else(|| {
                        runtime_error(bn_diag::DiagId::STALE_HANDLE, "ticket is invalid", span)
                    })?
                };
                return match method {
                    "Id" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(Value::Integer(
                            i128::from(ticket.id()),
                            bn_types::IntegerType::Int32,
                        ))
                    }
                    "Status" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(Value::Integer(
                            i128::from(ticket.status()),
                            bn_types::IntegerType::Int32,
                        ))
                    }
                    "Wait" => {
                        require_arity(name, arguments, 2, span)?;
                        let timeout = integer(&arguments[1], span)?.0;
                        let result = ticket.wait(timeout).map_or_else(
                            |failure| failed(name, &failure),
                            |_| ticket.result().unwrap_or(Value::Null),
                        );
                        let output = ticket.take_output();
                        core.output()
                            .write_all(output.as_bytes())
                            .map_err(|error| {
                                runtime_error(bn_diag::DiagId::IO, error.to_string(), span)
                            })?;
                        Ok(result)
                    }
                    "Cancel" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(ticket
                            .cancel()
                            .map_or_else(|failure| failed(name, &failure), Value::Boolean))
                    }
                    "Error" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(match ticket.result() {
                            Some(error @ Value::Error { .. }) => error,
                            _ => ticket
                                .error()
                                .map_or(Value::NotAvailable, |(code, message)| {
                                    Value::error(code, shared_string(message))
                                }),
                        })
                    }
                    "IsDone" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(Value::Boolean(ticket.is_done()))
                    }
                    "Close" => {
                        require_arity(name, arguments, 1, span)?;
                        {
                            let mut state = shared_dispatch()
                                .lock()
                                .unwrap_or_else(PoisonError::into_inner);
                            state.tickets.remove(&id);
                        }
                        ticket.close();
                        Ok(Value::Null)
                    }
                    _ => unreachable!(),
                };
            }
            _ => Ok(failed(
                name,
                &DispatchFailure::Unavailable("this build does not provide the member"),
            )),
        }
    }

    #[allow(clippy::unused_self)]
    fn dispatch_queue(&mut self, name: &str, workers: i128) -> Value {
        let id = {
            let mut state = shared_dispatch()
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            let id = state.next_queue;
            state.next_queue = state.next_queue.saturating_add(1);
            id
        };
        let queue = match crate::dispatch::Queue::new(workers, id) {
            Ok(queue) => queue,
            Err(failure) => return failed(name, &failure),
        };
        debug_assert!(
            (1..=bn_limits::dispatch_limits().worker_count_max).contains(&queue.workers())
        );
        let mut state = shared_dispatch()
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        state.queues.insert(id, queue);
        Value::DispatchQueue(id)
    }
}

/// The `Error` of a failed member `name` (`Dispatch.Semaphore.Release`),
/// with the report the native ABI records for the same failure.
fn failed(name: &str, failure: &DispatchFailure) -> Value {
    let mut parts = name.rsplit('.');
    let method = parts.next().unwrap_or_default();
    let class = parts.next().unwrap_or_default();
    Value::error_report(
        failure.code(),
        &format!("BNDispatch.{class}.{method}"),
        failure.message(),
        failure.cause(),
    )
}

#[cfg(test)]
mod tests {
    use super::BoundedTaskOutput;
    use std::io::Write;

    /// `Dispatch.*` codes in `modules/bn/BNDispatch.bn` are the ones both
    /// backends put in `Error.Code`.
    #[test]
    fn module_constants_match_the_runtime_codes() {
        let module = include_str!("../../../modules/bn/BNDispatch.bn");
        for (name, value) in bn_types::error_codes::dispatch::ALL {
            let line = format!("EXPORT CONST {name} AS INTEGER = {value}");
            assert!(module.contains(&line), "BNDispatch.bn lacks `{line}`");
        }
    }

    #[test]
    fn task_output_writer_rejects_bytes_after_registry_bound() {
        let maximum = bn_limits::dispatch_limits().output_max_bytes;
        let mut output = BoundedTaskOutput {
            bytes: Vec::new(),
            maximum,
        };

        output.write_all(&vec![b'x'; maximum]).expect("bound fits");
        let error = output
            .write_all(b"overflow")
            .expect_err("overflow must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::WriteZero);
        assert_eq!(output.bytes.len(), maximum);
    }
}
