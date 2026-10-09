//! Interpreter adapter wrapping `bn_core_dispatch` with Basic Next `Value` storage.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use bn_value::Value;

pub(crate) use bn_core_dispatch::DispatchFailure as DispatchError;
pub(crate) use bn_core_dispatch::{Queue as CoreQueue, Ticket as CoreTicket};
pub(crate) use bn_rt::dispatch_sync::{Barrier, DispatchGroup, DispatchMutex, DispatchSemaphore};

#[derive(Clone)]
pub(crate) struct Queue {
    core: CoreQueue,
}

#[derive(Clone)]
pub(crate) struct Ticket {
    core: CoreTicket,
    result: Arc<Mutex<Option<Value>>>,
}

impl Queue {
    pub(crate) fn new(workers: i128, id: u64) -> Result<Self, DispatchError> {
        let core = CoreQueue::new(workers, id)?;
        Ok(Self { core })
    }

    pub(crate) fn workers(&self) -> usize {
        self.core.workers()
    }

    pub(crate) fn tickets(&self) -> Vec<Ticket> {
        self.core
            .tickets()
            .into_iter()
            .map(|core| Ticket {
                core,
                result: Arc::new(Mutex::new(None)),
            })
            .collect()
    }

    pub(crate) fn submit_with<F>(&self, task: String, job: F) -> Result<Ticket, DispatchError>
    where
        F: FnOnce(Ticket) + Send + 'static,
    {
        let result = Arc::new(Mutex::new(None));
        let result_for_job = Arc::clone(&result);
        let core_ticket = self.core.submit_with(task, move |core_t| {
            let ticket = Ticket {
                core: core_t,
                result: result_for_job,
            };
            job(ticket);
        })?;
        Ok(Ticket {
            core: core_ticket,
            result,
        })
    }

    pub(crate) fn join(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        self.core.join(timeout_ms)
    }

    pub(crate) fn close(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        self.core.close(timeout_ms)
    }
}

impl Ticket {
    pub(crate) fn id(&self) -> u64 {
        self.core.id()
    }

    pub(crate) fn status(&self) -> i32 {
        self.core.status()
    }

    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        self.core.wait(timeout_ms)
    }

    pub(crate) fn cancel(&self) -> Result<bool, DispatchError> {
        self.core.cancel()
    }

    pub(crate) fn error(&self) -> Option<(i32, String)> {
        self.core.error()
    }

    pub(crate) fn is_done(&self) -> bool {
        self.core.is_done()
    }

    pub(crate) fn close(&self) {
        self.core.close();
    }

    pub(crate) fn mark_completed(&self) {
        self.core.mark_completed();
    }

    pub(crate) fn mark_failed(&self, code: i32, message: String) {
        self.core.mark_failed(code, message);
    }

    pub(crate) fn set_output(&self, output: String) -> Result<(), DispatchError> {
        self.core.set_output(output)
    }

    pub(crate) fn take_output(&self) -> String {
        self.core.take_output()
    }

    pub(crate) fn set_result(&self, value: Value) {
        *self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(value);
    }

    pub(crate) fn result(&self) -> Option<Value> {
        self.result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
