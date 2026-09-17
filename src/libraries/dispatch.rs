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

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::Value;

use crate::runtime::provider::{CoreContext, Provider};
use crate::runtime::{
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

pub struct DispatchProvider {
    queues: HashMap<u64, crate::dispatch::Queue>,
    next_queue: u64,
    tickets: HashMap<u64, crate::dispatch::Ticket>,
    next_ticket: u64,
    groups: HashMap<u64, crate::dispatch::DispatchGroup>,
    barriers: HashMap<u64, crate::dispatch::Barrier>,
    semaphores: HashMap<u64, crate::dispatch::DispatchSemaphore>,
    mutexes: HashMap<u64, crate::dispatch::DispatchMutex>,
    next_sync: u64,
}

impl Default for DispatchProvider {
    fn default() -> Self {
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
}

impl DispatchProvider {
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
                let id = self.next_sync;
                self.next_sync = self.next_sync.saturating_add(1);
                self.groups
                    .insert(id, crate::dispatch::DispatchGroup::new());
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
                let group = self.groups.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::STALE_HANDLE,
                        "group is invalid",
                        span,
                    )
                })?;
                match method {
                    "Enter" => {
                        require_arity(name, arguments, 1, span)?;
                        group.enter();
                        Ok(Value::Null)
                    }
                    "Leave" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(group.leave().map_or_else(dispatch_error, |_| Value::Null))
                    }
                    _ => {
                        require_arity(name, arguments, 2, span)?;
                        Ok(group
                            .wait(integer(&arguments[1], span)?.0)
                            .map_or_else(dispatch_error, |_| Value::Null))
                    }
                }
            }
            "New" | "Create" if name.contains(".Barrier.") => {
                require_arity(name, arguments, 1, span)?;
                let parties = integer(&arguments[0], span)?.0;
                let barrier = crate::dispatch::Barrier::new(parties).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::DISPATCH,
                        "barrier parties must be in 1..64",
                        span,
                    )
                })?;
                let id = self.next_sync;
                self.next_sync = self.next_sync.saturating_add(1);
                self.barriers.insert(id, barrier);
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
                let barrier = self.barriers.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::STALE_HANDLE,
                        "barrier is invalid",
                        span,
                    )
                })?;
                Ok(barrier
                    .wait(integer(&arguments[1], span)?.0)
                    .map_or_else(dispatch_error, Value::Boolean))
            }
            "New" | "Create" if name.contains(".Semaphore.") => {
                require_arity(name, arguments, 1, span)?;
                let semaphore =
                    crate::dispatch::DispatchSemaphore::new(integer(&arguments[0], span)?.0)
                        .ok_or_else(|| {
                            runtime_error(
                                crate::diagnostic::DiagId::DISPATCH,
                                "semaphore permits must be in 1..1024",
                                span,
                            )
                        })?;
                let id = self.next_sync;
                self.next_sync = self.next_sync.saturating_add(1);
                self.semaphores.insert(id, semaphore);
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
                let semaphore = self.semaphores.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::STALE_HANDLE,
                        "semaphore is invalid",
                        span,
                    )
                })?;
                if method == "Acquire" {
                    require_arity(name, arguments, 2, span)?;
                    Ok(semaphore
                        .acquire(integer(&arguments[1], span)?.0)
                        .map_or_else(dispatch_error, |_| Value::Null))
                } else {
                    require_arity(name, arguments, 1, span)?;
                    Ok(semaphore
                        .release()
                        .map_or_else(dispatch_error, |_| Value::Null))
                }
            }
            "New" | "Create" if name.contains(".Mutex.") => {
                require_arity(name, arguments, 0, span)?;
                let id = self.next_sync;
                self.next_sync = self.next_sync.saturating_add(1);
                self.mutexes
                    .insert(id, crate::dispatch::DispatchMutex::new());
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
                let mutex = self.mutexes.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::STALE_HANDLE,
                        "mutex is invalid",
                        span,
                    )
                })?;
                if method == "Lock" {
                    require_arity(name, arguments, 2, span)?;
                    Ok(mutex
                        .lock(integer(&arguments[1], span)?.0)
                        .map_or_else(dispatch_error, |_| Value::Null))
                } else {
                    require_arity(name, arguments, 1, span)?;
                    Ok(mutex.unlock().map_or_else(dispatch_error, |_| Value::Null))
                }
            }
            "Serial" => {
                require_arity(name, arguments, 0, span)?;
                Ok(self.dispatch_queue(1))
            }
            "Concurrent" => {
                require_arity(name, arguments, 1, span)?;
                let (workers, _) = integer(&arguments[0], span)?;
                Ok(self.dispatch_queue(workers))
            }
            "Auto" => {
                require_arity(name, arguments, 0, span)?;
                let workers = std::thread::available_parallelism()
                    .map(|count| {
                        count
                            .get()
                            .min(crate::config::dispatch_limits().worker_count_max)
                    })
                    .map_err(|error| {
                        runtime_error(
                            crate::diagnostic::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                            error.to_string(),
                            span,
                        )
                    })?;
                Ok(self.dispatch_queue(i128::try_from(workers).expect("usize fits i128")))
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
                    return Ok(Value::Error {
                        code: 1,
                        message: "Async expects a named function".into(),
                    });
                };
                let queue = self
                    .queues
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::STALE_HANDLE,
                            "queue is invalid",
                            span,
                        )
                    })?
                    .clone();
                let task_name = task.clone();
                let task_arguments = arguments[2..].to_vec();
                let worker_module = core.module().clone();
                let worker_host = core.host().fork_for_task();
                let ticket = queue
                    .submit_with(task_name.clone(), move |ticket| {
                        let mut input = std::io::Cursor::new(Vec::<u8>::new());
                        let mut output = BoundedTaskOutput {
                            bytes: Vec::new(),
                            maximum: crate::config::dispatch_limits().output_max_bytes,
                        };
                        match crate::runtime::execute_named_with_host(
                            &worker_module,
                            &task_name,
                            task_arguments,
                            &mut input,
                            &mut output,
                            &worker_host,
                        ) {
                            Ok(result) => {
                                let output = String::from_utf8_lossy(&output.bytes).into_owned();
                                if ticket.set_output(output).is_ok() {
                                    ticket.set_result(result);
                                    ticket.mark_completed();
                                } else {
                                    ticket.mark_failed(
                                        1,
                                        "async task output exceeds configured bound".into(),
                                    );
                                }
                            }
                            Err(error) => ticket.mark_failed(1, error.message.to_string()),
                        }
                    })
                    .map_err(|error| {
                        runtime_error(
                            crate::diagnostic::DiagId::DISPATCH,
                            format!("{error:?}"),
                            span,
                        )
                    })?;
                let ticket_id = self.next_ticket;
                self.next_ticket = self.next_ticket.saturating_add(1);
                self.tickets.insert(ticket_id, ticket);
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
                let queue = self
                    .queues
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::STALE_HANDLE,
                            "queue is invalid",
                            span,
                        )
                    })?
                    .clone();
                let tickets = queue.tickets();
                let result = if method == "Join" {
                    queue.join(timeout)
                } else {
                    queue.close(timeout)
                };
                for ticket in tickets {
                    let output = ticket.take_output();
                    core.output()
                        .write_all(output.as_bytes())
                        .map_err(|error| {
                            runtime_error(crate::diagnostic::DiagId::IO, error.to_string(), span)
                        })?;
                }
                return Ok(result.map_or_else(dispatch_error, |_| Value::Null));
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
                let queue = self.queues.get(&id).ok_or_else(|| {
                    runtime_error(
                        crate::diagnostic::DiagId::STALE_HANDLE,
                        "queue is invalid",
                        span,
                    )
                })?;
                Ok(queue
                    .close(timeout)
                    .map_or_else(dispatch_error, |_| Value::Null))
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
                let ticket = self
                    .tickets
                    .get(&id)
                    .ok_or_else(|| {
                        runtime_error(
                            crate::diagnostic::DiagId::STALE_HANDLE,
                            "ticket is invalid",
                            span,
                        )
                    })?
                    .clone();
                return match method {
                    "Id" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(Value::Integer(
                            i128::from(ticket.id()),
                            crate::types::IntegerType::Int32,
                        ))
                    }
                    "Status" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(Value::Integer(
                            i128::from(ticket.status()),
                            crate::types::IntegerType::Int32,
                        ))
                    }
                    "Wait" => {
                        require_arity(name, arguments, 2, span)?;
                        let timeout = integer(&arguments[1], span)?.0;
                        let result = ticket.wait(timeout).map_or_else(dispatch_error, |_| {
                            ticket.result().unwrap_or(Value::Null)
                        });
                        let output = ticket.take_output();
                        core.output()
                            .write_all(output.as_bytes())
                            .map_err(|error| {
                                runtime_error(
                                    crate::diagnostic::DiagId::IO,
                                    error.to_string(),
                                    span,
                                )
                            })?;
                        Ok(result)
                    }
                    "Cancel" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(ticket.cancel().map_or_else(dispatch_error, Value::Boolean))
                    }
                    "Error" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(ticket
                            .error()
                            .map_or(Value::NotAvailable, |(code, message)| Value::Error {
                                code,
                                message,
                            }))
                    }
                    "IsDone" => {
                        require_arity(name, arguments, 1, span)?;
                        Ok(Value::Boolean(ticket.is_done()))
                    }
                    "Close" => {
                        require_arity(name, arguments, 1, span)?;
                        ticket.close();
                        Ok(Value::Null)
                    }
                    _ => unreachable!(),
                };
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNDispatch operation unavailable".into(),
            }),
        }
    }

    fn dispatch_queue(&mut self, workers: i128) -> Value {
        let Some(queue) = crate::dispatch::Queue::new(workers) else {
            return Value::Error {
                code: 1,
                message: "worker count must be in 1..64".into(),
            };
        };
        debug_assert!(
            (1..=crate::config::dispatch_limits().worker_count_max).contains(&queue.workers())
        );
        let id = self.next_queue;
        self.next_queue += 1;
        self.queues.insert(id, queue);
        Value::DispatchQueue(id)
    }
}

fn dispatch_error(error: crate::dispatch::DispatchError) -> Value {
    let message = match error {
        crate::dispatch::DispatchError::TaskFailed(Some((_, message))) => message,
        other => format!("{other:?}"),
    };
    Value::Error { code: 1, message }
}

#[cfg(test)]
mod tests {
    use super::BoundedTaskOutput;
    use std::io::Write;

    #[test]
    fn task_output_writer_rejects_bytes_after_registry_bound() {
        let maximum = crate::config::dispatch_limits().output_max_bytes;
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
