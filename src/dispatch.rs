//! Bounded state for the external `BNDispatch` provider.
#![allow(dead_code)]

use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::{Duration, Instant};

pub(crate) mod sync;
pub(crate) use sync::{Barrier, DispatchGroup, DispatchMutex, DispatchSemaphore};

pub(crate) const MAX_WORKERS: usize = 64;
pub(crate) const MAX_PENDING: usize = 1_024;
pub(crate) const MIN_TIMEOUT_MS: i128 = 1;
pub(crate) const MAX_TIMEOUT_MS: i128 = 60_000;
pub(crate) const PENDING: i32 = 0;
pub(crate) const RUNNING: i32 = 1;
pub(crate) const COMPLETED: i32 = 2;
pub(crate) const FAILED: i32 = 3;
pub(crate) const CANCELLED: i32 = 4;

#[derive(Clone)]
pub(crate) struct Queue {
    inner: Arc<QueueInner>,
}

struct QueueInner {
    workers: usize,
    state: Mutex<QueueState>,
    wake: Condvar,
    sender: mpsc::SyncSender<Box<dyn FnOnce() + Send>>,
}

struct QueueState {
    closed: bool,
    next_id: u64,
    tickets: Vec<Arc<TicketInner>>,
}

#[derive(Clone)]
pub(crate) struct Ticket {
    inner: Arc<TicketInner>,
}

struct TicketInner {
    id: u64,
    state: Mutex<TicketState>,
    wake: Condvar,
}

struct TicketState {
    status: i32,
    error: Option<(i32, String)>,
    closed: bool,
    task: String,
    output: String,
}

impl Queue {
    pub(crate) fn new(workers: i128) -> Option<Self> {
        let workers = usize::try_from(workers).ok()?;
        if !(1..=MAX_WORKERS).contains(&workers) {
            return None;
        }
        let (sender, receiver) = mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(MAX_PENDING);
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..workers {
            let receiver = Arc::clone(&receiver);
            std::thread::spawn(move || {
                loop {
                    let job = receiver.lock().expect("queue receiver poisoned").recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                }
            });
        }
        Some(Self {
            inner: Arc::new(QueueInner {
                workers,
                state: Mutex::new(QueueState {
                    closed: false,
                    next_id: 1,
                    tickets: Vec::new(),
                }),
                wake: Condvar::new(),
                sender,
            }),
        })
    }

    pub(crate) fn workers(&self) -> usize {
        self.inner.workers
    }

    pub(crate) fn tickets(&self) -> Vec<Ticket> {
        self.inner
            .state
            .lock()
            .expect("queue mutex poisoned")
            .tickets
            .iter()
            .cloned()
            .map(|inner| Ticket { inner })
            .collect()
    }

    pub(crate) fn submit(&self, task: String) -> Result<Ticket, DispatchError> {
        self.submit_with(task, |_| {})
    }

    pub(crate) fn submit_with<F>(&self, task: String, job: F) -> Result<Ticket, DispatchError>
    where
        F: FnOnce(Ticket) + Send + 'static,
    {
        let mut state = self.inner.state.lock().expect("queue mutex poisoned");
        if state.closed {
            return Err(DispatchError::Closed);
        }
        if state.tickets.len() >= MAX_PENDING {
            return Err(DispatchError::Saturated);
        }
        let ticket = Arc::new(TicketInner {
            id: state.next_id,
            state: Mutex::new(TicketState {
                status: PENDING,
                error: None,
                closed: false,
                task,
                output: String::new(),
            }),
            wake: Condvar::new(),
        });
        state.next_id = state.next_id.saturating_add(1);
        state.tickets.push(Arc::clone(&ticket));
        let public_ticket = Ticket {
            inner: Arc::clone(&ticket),
        };
        let queued_ticket = public_ticket.clone();
        let queued = self
            .inner
            .sender
            .try_send(Box::new(move || job(queued_ticket)));
        if queued.is_err() {
            state.tickets.pop();
            return Err(DispatchError::Saturated);
        }
        self.inner.wake.notify_all();
        Ok(public_ticket)
    }

    pub(crate) fn join(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut state = self.inner.state.lock().expect("queue mutex poisoned");
        loop {
            if state.tickets.iter().all(|ticket| ticket.is_terminal()) {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchError::Timeout);
            }
            (state, _) = self
                .inner
                .wake
                .wait_timeout(state, remaining)
                .expect("queue mutex poisoned");
        }
    }

    pub(crate) fn close(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut state = self.inner.state.lock().expect("queue mutex poisoned");
        state.closed = true;
        for ticket in &state.tickets {
            ticket.cancel_pending();
        }
        self.inner.wake.notify_all();
        loop {
            if state.tickets.iter().all(|ticket| ticket.is_terminal()) {
                return Ok(());
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchError::Timeout);
            }
            (state, _) = self
                .inner
                .wake
                .wait_timeout(state, remaining)
                .expect("queue mutex poisoned");
        }
    }
}

impl Ticket {
    pub(crate) fn id(&self) -> u64 {
        self.inner.id
    }
    pub(crate) fn task(&self) -> Result<String, DispatchError> {
        let state = self.inner.state.lock().expect("ticket mutex poisoned");
        if state.closed {
            return Err(DispatchError::Closed);
        }
        Ok(state.task.clone())
    }
    pub(crate) fn status(&self) -> Result<i32, DispatchError> {
        let state = self.inner.state.lock().expect("ticket mutex poisoned");
        if state.closed {
            return Err(DispatchError::Closed);
        }
        Ok(state.status)
    }
    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut state = self.inner.state.lock().expect("ticket mutex poisoned");
        loop {
            if state.closed {
                return Err(DispatchError::Closed);
            }
            match state.status {
                COMPLETED => return Ok(()),
                FAILED => return Err(DispatchError::TaskFailed(state.error.clone())),
                CANCELLED => return Err(DispatchError::Cancelled),
                _ => {}
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchError::Timeout);
            }
            (state, _) = self
                .inner
                .wake
                .wait_timeout(state, remaining)
                .expect("ticket mutex poisoned");
        }
    }
    pub(crate) fn cancel(&self) -> Result<bool, DispatchError> {
        let mut state = self.inner.state.lock().expect("ticket mutex poisoned");
        if state.closed {
            return Err(DispatchError::Closed);
        }
        if state.status == PENDING {
            state.status = CANCELLED;
            self.inner.wake.notify_all();
            return Ok(true);
        }
        Ok(false)
    }
    pub(crate) fn error(&self) -> Result<Option<(i32, String)>, DispatchError> {
        let state = self.inner.state.lock().expect("ticket mutex poisoned");
        if state.closed {
            return Err(DispatchError::Closed);
        }
        Ok(state.error.clone())
    }
    pub(crate) fn is_done(&self) -> Result<bool, DispatchError> {
        Ok(matches!(self.status()?, COMPLETED | FAILED | CANCELLED))
    }
    pub(crate) fn close(&self) {
        let mut state = self.inner.state.lock().expect("ticket mutex poisoned");
        state.closed = true;
        state.task.clear();
        state.error = None;
        state.output.clear();
        self.inner.wake.notify_all();
    }
    pub(crate) fn mark_running(&self) -> Result<(), DispatchError> {
        let mut state = self.inner.state.lock().expect("ticket mutex poisoned");
        if state.closed || state.status != PENDING {
            return Err(DispatchError::Closed);
        }
        state.status = RUNNING;
        Ok(())
    }
    pub(crate) fn mark_completed(&self) {
        let mut state = self.inner.state.lock().expect("ticket mutex poisoned");
        state.status = COMPLETED;
        self.inner.wake.notify_all();
    }
    pub(crate) fn mark_failed(&self, code: i32, message: String) {
        let mut state = self.inner.state.lock().expect("ticket mutex poisoned");
        state.status = FAILED;
        state.error = Some((code, message));
        self.inner.wake.notify_all();
    }
    pub(crate) fn set_output(&self, output: String) -> Result<(), ()> {
        if output.len() > crate::config::web_limits().async_output_max_bytes {
            return Err(());
        }
        self.inner
            .state
            .lock()
            .expect("ticket mutex poisoned")
            .output = output;
        Ok(())
    }
    pub(crate) fn take_output(&self) -> String {
        std::mem::take(
            &mut self
                .inner
                .state
                .lock()
                .expect("ticket mutex poisoned")
                .output,
        )
    }
}

impl TicketInner {
    fn is_terminal(&self) -> bool {
        let state = self.state.lock().expect("ticket mutex poisoned");
        state.closed || matches!(state.status, COMPLETED | FAILED | CANCELLED)
    }
    fn cancel_pending(&self) {
        let mut state = self.state.lock().expect("ticket mutex poisoned");
        if state.status == PENDING {
            state.status = CANCELLED;
            self.wake.notify_all();
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DispatchError {
    Closed,
    Saturated,
    Timeout,
    Cancelled,
    TaskFailed(Option<(i32, String)>),
    InvalidTimeout,
}

fn deadline(timeout_ms: i128) -> Result<Instant, DispatchError> {
    if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(DispatchError::InvalidTimeout);
    }
    Ok(Instant::now()
        + Duration::from_millis(
            u64::try_from(timeout_ms).expect("validated timeout is non-negative"),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_rejects_invalid_workers_and_tracks_ticket_lifecycle() {
        assert!(Queue::new(0).is_none());
        let queue = Queue::new(2).expect("valid queue");
        let ticket = queue.submit("Work".into()).expect("ticket");
        assert_eq!(ticket.status(), Ok(PENDING));
        assert!(ticket.cancel().expect("cancel"));
        assert_eq!(ticket.status(), Ok(CANCELLED));
    }
    #[test]
    fn close_cancels_pending_tickets() {
        let queue = Queue::new(1).expect("valid queue");
        let ticket = queue.submit("Work".into()).expect("ticket");
        queue.close(100).expect("close");
        assert_eq!(ticket.status(), Ok(CANCELLED));
    }

    #[test]
    fn ticket_rejects_output_above_registry_bound() {
        let queue = Queue::new(1).expect("valid queue");
        let ticket = queue.submit("Work".into()).expect("ticket");
        let maximum = crate::config::web_limits().async_output_max_bytes;

        assert!(ticket.set_output("x".repeat(maximum + 1)).is_err());
        assert_eq!(ticket.take_output(), "");
    }

    #[test]
    fn queue_rejects_the_ticket_after_the_pending_bound() {
        let queue = Queue::new(1).expect("valid queue");
        for _ in 0..MAX_PENDING {
            queue.submit("Work".into()).expect("within pending bound");
        }
        assert!(matches!(
            queue.submit("Overflow".into()),
            Err(DispatchError::Saturated)
        ));
    }

    #[test]
    fn concurrent_queue_runs_two_jobs_at_once() {
        let queue = Queue::new(2).expect("valid queue");
        let rendezvous = Arc::new(std::sync::Barrier::new(2));
        for _ in 0..2 {
            let rendezvous = Arc::clone(&rendezvous);
            queue
                .submit_with("Work".into(), move |ticket| {
                    ticket.mark_running().expect("job starts");
                    rendezvous.wait();
                    ticket.mark_completed();
                })
                .expect("job within queue bound");
        }
        queue.join(1_000).expect("both workers rendezvous");
    }
}
