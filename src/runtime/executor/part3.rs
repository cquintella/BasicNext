#![allow(
    clippy::wildcard_imports,
    clippy::too_many_lines,
    clippy::needless_return,
    clippy::ignored_unit_patterns,
    clippy::redundant_closure
)]
use super::*;

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

impl Executor<'_, '_> {
    pub(crate) fn call_named(
        &mut self,
        name: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if name.starts_with("HOST.") {
            return self.host_call(name, &arguments, span);
        }
        if is_host_file_method(name) {
            return self.file_call(name, &arguments, span);
        }
        if self.is_bndata_provider(name)
            && (name.ends_with(".CONSTRUCTOR") || name.ends_with(".$fields"))
        {
            return Ok(Value::Null);
        }
        if self.is_bndata_provider(name) && name.contains(".DataFrame.") {
            return self.dataframe_call(name, &arguments, span);
        }
        if self.is_bndata_provider(name)
            && (name.ends_with(".ReadCSV") || name.ends_with(".WriteCSV"))
        {
            return self.data_call(name, &arguments, span);
        }
        if self.is_bnmath_provider(name) {
            let math_name = name.rsplit('.').next().unwrap_or(name);
            let builtin_name = format!("BNMath.{math_name}");
            if is_temporal_builtin(&builtin_name) {
                return temporal_call(&builtin_name, &arguments, span);
            }
            return builtin(&builtin_name, &arguments, span, &self.memory);
        }
        if self.is_bnlog_provider(name) {
            if name.contains(".Fields.") {
                return self.log_fields_call(name, &arguments, span);
            }
            if name.contains(".Entry.") {
                return self.log_entry_call(name, &arguments, span);
            }
            if name.contains(".Logger.") {
                return self.log_logger_call(name, &arguments, span);
            }
            if name.ends_with(".CONSTRUCTOR") || name.ends_with(".$fields") {
                return Ok(Value::Null);
            }
            return Ok(Value::Error {
                code: 1,
                message: "BNLog provider unavailable".into(),
            });
        }
        if self.is_bnjson_provider(name) {
            return self.json_call(name, &arguments, span);
        }
        if self.is_bnweb_provider(name)
            && (name.contains(".Server.")
                || name.contains(".Response.")
                || name.contains(".Client.")
                || name.contains(".TLSConfig.")
                || name.contains(".ServerOptions.")
                || name.contains(".EgressPolicy.")
                || name.contains(".CookieJar.")
                || name.contains(".SessionStore.")
                || name.contains(".Scraper.")
                || name.contains(".ACL.")
                || (name.contains(".Request.")
                    && matches!(
                        name.rsplit('.').next(),
                        Some(
                            "CONSTRUCTOR"
                                | "Method"
                                | "Target"
                                | "Headers"
                                | "Query"
                                | "Body"
                                | "PeerAddress"
                                | "EffectiveClientAddress",
                        )
                    ))
                || name.contains(".HeaderValues.")
                || name.contains(".QueryValues."))
        {
            return self.web_call(name, &arguments, span);
        }
        if self.is_bndispatch_provider(name) {
            return self.dispatch_call(name, &arguments, span);
        }
        if is_temporal_builtin(name) {
            return temporal_call(name, &arguments, span);
        }
        if name.starts_with("BNMath.") || matches!(name, "ASC" | "CHAR") || name == "$for_condition"
        {
            return builtin(name, &arguments, span, &self.memory);
        }
        let (name, super_call) = name
            .strip_prefix("@super:")
            .map_or((name, false), |name| (name, true));
        let resolved = if super_call {
            name.to_string()
        } else {
            self.dispatch_name(name, &arguments)
        };
        let index = self
            .module
            .functions
            .iter()
            .position(|function| function.name == resolved)
            .ok_or_else(|| {
                runtime_error(
                    "NAME_NOT_FOUND",
                    format!("function '{resolved}' is not available"),
                    span,
                )
            })?;
        let constructed = (name.ends_with(".CONSTRUCTOR") || name.ends_with(".$fields"))
            .then(|| match arguments.first() {
                Some(Value::Object { handle, .. }) => Some(*handle),
                _ => None,
            })
            .flatten();
        let pinned = lifecycle_dispatch(&resolved, &arguments);
        if let Some(pinned) = pinned.clone() {
            self.pinned_dispatch.push(pinned);
        }
        self.call_depth += 1;
        let result = self.function(&self.module.functions[index], arguments);
        self.call_depth = self.call_depth.saturating_sub(1);
        if pinned.is_some() {
            let _ = self.pinned_dispatch.pop();
        }
        match result {
            Ok(Flow::Return(value)) => Ok(value.unwrap_or(Value::Null)),
            Ok(Flow::Stop(code)) => {
                self.stop_code = Some(code);
                Ok(Value::Null)
            }
            Err(error) => {
                if let Some(handle) = constructed {
                    let _ = self.objects.delete(handle, span);
                }
                Err(error)
            }
        }
    }

    pub(crate) fn is_bndata_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bndata_providers)
    }

    pub(crate) fn is_bnmath_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bnmath_providers)
    }

    pub(crate) fn is_bnlog_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bnlog_providers)
    }

    pub(crate) fn is_bnjson_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bnjson_providers)
    }

    pub(crate) fn is_bnweb_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bnweb_providers)
    }

    pub(crate) fn is_bndispatch_provider(&self, name: &str) -> bool {
        Self::standard_provider(name, &self.module.bndispatch_providers)
    }

    pub(crate) fn dispatch_call(
        &mut self,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = name.rsplit('.').next().unwrap_or_default();
        match method {
            "New" | "Create" if name.contains(".Group.") => {
                require_arity(name, arguments, 0, span)?;
                let id = self.next_dispatch_sync; self.next_dispatch_sync = self.next_dispatch_sync.saturating_add(1);
                self.dispatch_groups.insert(id, crate::dispatch::DispatchGroup::new());
                Ok(Value::DispatchGroup(id))
            }
            "Enter" | "Leave" | "Wait" if name.contains(".Group.") => {
                let Value::DispatchGroup(id) = arguments.first().cloned().unwrap_or(Value::Null) else { return Err(runtime_error("TYPE_MISMATCH", "operation expects Group", span)); };
                let group = self.dispatch_groups.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "group is invalid", span))?;
                match method { "Enter" => { require_arity(name, arguments, 1, span)?; group.enter(); Ok(Value::Null) }, "Leave" => { require_arity(name, arguments, 1, span)?; Ok(group.leave().map_or_else(dispatch_error, |_| Value::Null)) }, _ => { require_arity(name, arguments, 2, span)?; Ok(group.wait(integer(&arguments[1], span)?.0).map_or_else(dispatch_error, |_| Value::Null)) } }
            }
            "New" | "Create" if name.contains(".Barrier.") => {
                require_arity(name, arguments, 1, span)?;
                let parties = integer(&arguments[0], span)?.0;
                let barrier = crate::dispatch::Barrier::new(parties).ok_or_else(|| runtime_error("DISPATCH", "barrier parties must be in 1..64", span))?;
                let id = self.next_dispatch_sync; self.next_dispatch_sync = self.next_dispatch_sync.saturating_add(1); self.dispatch_barriers.insert(id, barrier); Ok(Value::DispatchBarrier(id))
            }
            "Wait" if name.contains(".Barrier.") => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchBarrier(id) = arguments[0] else { return Err(runtime_error("TYPE_MISMATCH", "operation expects Barrier", span)); };
                let barrier = self.dispatch_barriers.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "barrier is invalid", span))?;
                Ok(barrier.wait(integer(&arguments[1], span)?.0).map_or_else(dispatch_error, Value::Boolean))
            }
            "New" | "Create" if name.contains(".Semaphore.") => {
                require_arity(name, arguments, 1, span)?;
                let semaphore = crate::dispatch::DispatchSemaphore::new(integer(&arguments[0], span)?.0).ok_or_else(|| runtime_error("DISPATCH", "semaphore permits must be in 1..1024", span))?;
                let id = self.next_dispatch_sync; self.next_dispatch_sync = self.next_dispatch_sync.saturating_add(1); self.dispatch_semaphores.insert(id, semaphore); Ok(Value::DispatchSemaphore(id))
            }
            "Acquire" | "Release" if name.contains(".Semaphore.") => {
                let Value::DispatchSemaphore(id) = arguments[0] else { return Err(runtime_error("TYPE_MISMATCH", "operation expects Semaphore", span)); };
                let semaphore = self.dispatch_semaphores.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "semaphore is invalid", span))?;
                if method == "Acquire" { require_arity(name, arguments, 2, span)?; Ok(semaphore.acquire(integer(&arguments[1], span)?.0).map_or_else(dispatch_error, |_| Value::Null)) } else { require_arity(name, arguments, 1, span)?; Ok(semaphore.release().map_or_else(dispatch_error, |_| Value::Null)) }
            }
            "New" | "Create" if name.contains(".Mutex.") => {
                require_arity(name, arguments, 0, span)?; let id = self.next_dispatch_sync; self.next_dispatch_sync = self.next_dispatch_sync.saturating_add(1); self.dispatch_mutexes.insert(id, crate::dispatch::DispatchMutex::new()); Ok(Value::DispatchMutex(id))
            }
            "Lock" | "Unlock" if name.contains(".Mutex.") => {
                let Value::DispatchMutex(id) = arguments[0] else { return Err(runtime_error("TYPE_MISMATCH", "operation expects Mutex", span)); };
                let mutex = self.dispatch_mutexes.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "mutex is invalid", span))?;
                if method == "Lock" { require_arity(name, arguments, 2, span)?; Ok(mutex.lock(integer(&arguments[1], span)?.0).map_or_else(dispatch_error, |_| Value::Null)) } else { require_arity(name, arguments, 1, span)?; Ok(mutex.unlock().map_or_else(dispatch_error, |_| Value::Null)) }
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
                    .map(|count| count.get().min(crate::config::dispatch_limits().worker_count_max))
                    .map_err(|error| {
                        runtime_error("HOST_CAPABILITY_UNAVAILABLE", error.to_string(), span)
                    })?;
                Ok(self.dispatch_queue(i128::try_from(workers).expect("usize fits i128")))
            }
            "Async" => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchQueue(id) = arguments[0] else {
                    return Err(runtime_error("TYPE_MISMATCH", "Async expects Queue", span));
                };
                let Value::Function(task) = &arguments[1] else {
                    return Ok(Value::Error { code: 1, message: "Async expects a named function".into() });
                };
                let queue = self.dispatch_queues.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "queue is invalid", span))?.clone();
                let task_name = task.clone();
                let worker_module = self.module.clone();
                let worker_host = self.host.fork_for_task();
                let ticket = queue.submit_with(task_name.clone(), move |ticket| {
                    let mut worker_module = worker_module;
                    for function in &mut worker_module.functions {
                        if function.name == task_name { function.name = "Start".into(); }
                        else if function.name == "Start" { function.name = "__dispatch_start".into(); }
                    }
                    let mut input = std::io::Cursor::new(Vec::<u8>::new());
                    let mut output = BoundedTaskOutput {
                        bytes: Vec::new(),
                        maximum: crate::config::dispatch_limits().output_max_bytes,
                    };
                    match crate::runtime::execute_with_host(&worker_module, &mut input, &mut output, &worker_host) {
                        Ok(_) => {
                            let output = String::from_utf8_lossy(&output.bytes).into_owned();
                            if ticket.set_output(output).is_ok() {
                                ticket.mark_completed();
                            } else {
                                ticket.mark_failed(1, "async task output exceeds configured bound".into());
                            }
                        }
                        Err(error) => ticket.mark_failed(1, error.message),
                    }
                }).map_err(|error| runtime_error("DISPATCH", format!("{error:?}"), span))?;
                let ticket_id = self.next_dispatch_ticket;
                self.next_dispatch_ticket = self.next_dispatch_ticket.saturating_add(1);
                self.dispatch_tickets.insert(ticket_id, ticket);
                Ok(Value::DispatchTicket(ticket_id))
            }
            "Join" if name.contains(".Queue.") => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchQueue(id) = arguments[0] else {
                    return Err(runtime_error("TYPE_MISMATCH", "operation expects Queue", span));
                };
                let timeout = integer(&arguments[1], span)?.0;
                let queue = self.dispatch_queues.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "queue is invalid", span))?.clone();
                let tickets = queue.tickets();
                let result = if method == "Join" { queue.join(timeout) } else { queue.close(timeout) };
                for ticket in tickets {
                    let output = ticket.take_output();
                    self.output.write_all(output.as_bytes()).map_err(|error| runtime_error("IO", error.to_string(), span))?;
                }
                return Ok(result.map_or_else(|error| dispatch_error(error), |_| Value::Null));
            }
            "Close" if name.contains(".Queue.") => {
                require_arity(name, arguments, 2, span)?;
                let Value::DispatchQueue(id) = arguments[0] else {
                    return Err(runtime_error("TYPE_MISMATCH", "operation expects Queue", span));
                };
                let timeout = integer(&arguments[1], span)?.0;
                let queue = self.dispatch_queues.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "queue is invalid", span))?;
                Ok(queue.close(timeout).map_or_else(dispatch_error, |_| Value::Null))
            }
            "Id" | "Status" | "Wait" | "Cancel" | "Error" | "IsDone" | "Close"
                if name.contains(".Ticket.") => {
                let Value::DispatchTicket(id) = arguments.first().cloned().unwrap_or(Value::Null) else {
                    return Err(runtime_error("TYPE_MISMATCH", "operation expects Ticket", span));
                };
                let ticket = self.dispatch_tickets.get(&id).ok_or_else(|| runtime_error("STALE_HANDLE", "ticket is invalid", span))?.clone();
                return match method {
                    "Id" => { require_arity(name, arguments, 1, span)?; Ok(Value::Integer(i128::from(ticket.id()), crate::semantic::IntegerType::Int32)) }
                    "Status" => { require_arity(name, arguments, 1, span)?; Ok(Value::Integer(i128::from(ticket.status()), crate::semantic::IntegerType::Int32)) }
                    "Wait" => {
                        require_arity(name, arguments, 2, span)?;
                        let timeout = integer(&arguments[1], span)?.0;
                        let result = ticket.wait(timeout).map_or_else(dispatch_error, |_| Value::Null);
                        let output = ticket.take_output();
                        self.output.write_all(output.as_bytes()).map_err(|error| runtime_error("IO", error.to_string(), span))?;
                        Ok(result)
                    }
                    "Cancel" => { require_arity(name, arguments, 1, span)?; Ok(ticket.cancel().map_or_else(dispatch_error, Value::Boolean)) }
                    "Error" => { require_arity(name, arguments, 1, span)?; Ok(ticket.error().map_or(Value::NotAvailable, |(code, message)| Value::Error { code, message })) }
                    "IsDone" => { require_arity(name, arguments, 1, span)?; Ok(Value::Boolean(ticket.is_done())) }
                    "Close" => { require_arity(name, arguments, 1, span)?; ticket.close(); Ok(Value::Null) }
                    _ => unreachable!(),
                };
            }
            _ => Ok(Value::Error {
                code: 1,
                message: "BNDispatch operation unavailable".into(),
            }),
        }
    }

    pub(crate) fn dispatch_queue(&mut self, workers: i128) -> Value {
        let Some(queue) = crate::dispatch::Queue::new(workers) else {
            return Value::Error {
                code: 1,
                message: "worker count must be in 1..64".into(),
            };
        };
        debug_assert!(
            (1..=crate::config::dispatch_limits().worker_count_max).contains(&queue.workers())
        );
        let id = self.next_dispatch_queue;
        self.next_dispatch_queue += 1;
        self.dispatch_queues.insert(id, queue);
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
        let error = output.write_all(b"overflow").expect_err("overflow must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::WriteZero);
        assert_eq!(output.bytes.len(), maximum);
    }
}
