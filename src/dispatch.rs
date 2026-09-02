//! Bounded state for the external `BNDispatch` provider.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, Weak, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ring::rand::{SecureRandom, SystemRandom};

pub(crate) mod sync;
pub(crate) use sync::{Barrier, DispatchGroup, DispatchMutex, DispatchSemaphore};

pub(crate) const PENDING: i32 = 0;
pub(crate) const RUNNING: i32 = 1;
pub(crate) const COMPLETED: i32 = 2;
pub(crate) const FAILED: i32 = 3;
pub(crate) const CANCELLED: i32 = 4;

type Job = Box<dyn FnOnce() + Send>;
type JobSender = mpsc::SyncSender<Job>;

#[derive(Clone)]
pub(crate) struct Queue {
    inner: Arc<QueueInner>,
}

struct QueueInner {
    workers: usize,
    state: Mutex<QueueState>,
    wake: Condvar,
    sender: Mutex<Option<JobSender>>,
    worker_handles: Mutex<Vec<JoinHandle<()>>>,
}

struct QueueState {
    closed: bool,
    tickets: HashMap<u64, Arc<TicketInner>>,
}

#[derive(Clone)]
pub(crate) struct Ticket {
    inner: Arc<TicketInner>,
}

struct TicketInner {
    id: u64,
    state: Mutex<TicketState>,
    wake: Condvar,
    queue_wake: Weak<QueueInner>,
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
        if !(1..=crate::config::dispatch_limits().worker_count_max).contains(&workers) {
            return None;
        }
        let (sender, receiver) = mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(
            crate::config::dispatch_limits().pending_tickets_max,
        );
        let receiver = Arc::new(Mutex::new(receiver));
        let mut worker_handles = Vec::with_capacity(workers);
        for _ in 0..workers {
            let receiver = Arc::clone(&receiver);
            worker_handles.push(std::thread::spawn(move || {
                loop {
                    let job = receiver
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .recv();
                    match job {
                        Ok(job) => {
                            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                        }
                        Err(_) => break,
                    }
                }
            }));
        }
        let inner = Arc::new(QueueInner {
            workers,
            state: Mutex::new(QueueState {
                closed: false,
                tickets: HashMap::new(),
            }),
            wake: Condvar::new(),
            sender: Mutex::new(Some(sender)),
            worker_handles: Mutex::new(worker_handles),
        });
        Some(Self { inner })
    }

    pub(crate) fn workers(&self) -> usize {
        self.inner.workers
    }

    pub(crate) fn tickets(&self) -> Vec<Ticket> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .tickets
            .values()
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
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(DispatchError::Closed);
        }
        state.tickets.retain(|_, ticket| !ticket.is_terminal());
        if state.tickets.len() >= crate::config::dispatch_limits().pending_tickets_max {
            return Err(DispatchError::Saturated);
        }
        let id = next_ticket_id(&state.tickets)?;
        let ticket = Arc::new(TicketInner {
            id,
            state: Mutex::new(TicketState {
                status: PENDING,
                error: None,
                closed: false,
                task,
                output: String::new(),
            }),
            wake: Condvar::new(),
            queue_wake: Arc::downgrade(&self.inner),
        });
        state.tickets.insert(id, Arc::clone(&ticket));
        let public_ticket = Ticket {
            inner: Arc::clone(&ticket),
        };
        let queued_ticket = public_ticket.clone();
        let sender = self
            .inner
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .cloned()
            .ok_or(DispatchError::Closed)?;
        let queued = sender.try_send(Box::new(move || {
            if queued_ticket.mark_running().is_err() {
                return;
            }
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                job(queued_ticket.clone());
            }));
            if result.is_err() {
                queued_ticket.mark_failed(1, "dispatch task panicked".into());
            } else if queued_ticket.status() == RUNNING {
                queued_ticket.mark_completed();
            }
        }));
        if queued.is_err() {
            state.tickets.remove(&id);
            return Err(DispatchError::Saturated);
        }
        self.inner.wake.notify_all();
        Ok(public_ticket)
    }

    pub(crate) fn join(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        if self.is_worker_thread() {
            return Err(DispatchError::SelfJoin);
        }
        let deadline = deadline(timeout_ms)?;
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if state.tickets.values().all(|ticket| ticket.is_terminal()) {
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
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub(crate) fn close(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        if self.is_worker_thread() {
            return Err(DispatchError::SelfJoin);
        }
        let deadline = deadline(timeout_ms)?;
        {
            let mut state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            for ticket in state.tickets.values() {
                ticket.cancel_pending();
            }
        }
        self.inner
            .sender
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        self.inner.wake.notify_all();
        loop {
            let all_terminal = {
                let state = self
                    .inner
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.tickets.values().all(|ticket| ticket.is_terminal())
            };
            if all_terminal {
                return self.join_workers_until(deadline);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DispatchError::Timeout);
            }
            let state = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let (_state, _) = self
                .inner
                .wake
                .wait_timeout(state, remaining)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    fn join_workers_until(&self, deadline: Instant) -> Result<(), DispatchError> {
        loop {
            let all_finished = {
                let workers = self
                    .inner
                    .worker_handles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                workers.iter().all(JoinHandle::is_finished)
            };
            if all_finished {
                let mut workers = self
                    .inner
                    .worker_handles
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                for worker in workers.drain(..) {
                    let _ = worker.join();
                }
                return Ok(());
            }
            if deadline.saturating_duration_since(Instant::now()).is_zero() {
                return Err(DispatchError::Timeout);
            }
            std::thread::yield_now();
        }
    }

    fn is_worker_thread(&self) -> bool {
        let current = std::thread::current().id();
        let workers = self
            .inner
            .worker_handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        workers.iter().any(|worker| worker.thread().id() == current)
    }

    #[cfg(test)]
    fn workers_joined_for_test(&self) -> bool {
        self.inner
            .worker_handles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_empty()
    }
}

impl Ticket {
    pub(crate) fn id(&self) -> u64 {
        self.inner.id
    }
    pub(crate) fn task(&self) -> Result<String, DispatchError> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(DispatchError::Closed);
        }
        Ok(state.task.clone())
    }
    pub(crate) fn status(&self) -> i32 {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.status
    }
    pub(crate) fn wait(&self, timeout_ms: i128) -> Result<(), DispatchError> {
        let deadline = deadline(timeout_ms)?;
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }
    pub(crate) fn cancel(&self) -> Result<bool, DispatchError> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    pub(crate) fn error(&self) -> Option<(i32, String)> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.error.clone()
    }
    pub(crate) fn is_done(&self) -> bool {
        matches!(self.status(), COMPLETED | FAILED | CANCELLED)
    }
    pub(crate) fn close(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        state.task.clear();
        state.output.clear();
        self.inner.wake.notify_all();
    }
    pub(crate) fn mark_running(&self) -> Result<(), DispatchError> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed || state.status != PENDING {
            return Err(DispatchError::Closed);
        }
        state.status = RUNNING;
        Ok(())
    }
    pub(crate) fn mark_completed(&self) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.status == RUNNING {
            state.status = COMPLETED;
            self.inner.wake.notify_all();
            self.inner.notify_queue();
        }
    }
    pub(crate) fn mark_failed(&self, code: i32, message: String) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.status == RUNNING {
            state.status = FAILED;
            state.error = Some((code, message));
            self.inner.wake.notify_all();
            self.inner.notify_queue();
        }
    }
    pub(crate) fn set_output(&self, output: String) -> Result<(), ()> {
        if output.len() > crate::config::dispatch_limits().output_max_bytes {
            return Err(());
        }
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .output = output;
        Ok(())
    }
    pub(crate) fn take_output(&self) -> String {
        std::mem::take(
            &mut self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .output,
        )
    }
}

impl TicketInner {
    fn is_terminal(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed || matches!(state.status, COMPLETED | FAILED | CANCELLED)
    }
    fn cancel_pending(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.status == PENDING {
            state.status = CANCELLED;
            self.wake.notify_all();
            self.notify_queue();
        }
    }

    fn notify_queue(&self) {
        if let Some(queue) = self.queue_wake.upgrade() {
            queue.wake.notify_all();
        }
    }
}

impl Drop for QueueInner {
    fn drop(&mut self) {
        if let Ok(sender) = self.sender.get_mut() {
            sender.take();
        }
        if let Ok(workers) = self.worker_handles.get_mut() {
            for worker in workers.drain(..) {
                let _ = worker.join();
            }
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
    Entropy,
    SelfJoin,
    InvalidTimeout,
    GroupUnderflow,
    InvalidRelease,
    NotOwner,
}

fn next_ticket_id(tickets: &HashMap<u64, Arc<TicketInner>>) -> Result<u64, DispatchError> {
    let random = SystemRandom::new();
    let mut bytes = [0_u8; std::mem::size_of::<u64>()];
    for _ in 0..8 {
        random
            .fill(&mut bytes)
            .map_err(|_| DispatchError::Entropy)?;
        let id = u64::from_ne_bytes(bytes);
        if id != 0 && !tickets.contains_key(&id) {
            return Ok(id);
        }
    }
    Err(DispatchError::Saturated)
}

fn deadline(timeout_ms: i128) -> Result<Instant, DispatchError> {
    let limits = crate::config::dispatch_limits();
    if !(limits.timeout_min_ms..=limits.timeout_max_ms).contains(&timeout_ms) {
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
        ticket.wait(1_000).expect("no-op task completes");
        assert_eq!(ticket.status(), COMPLETED);
        queue.close(1_000).expect("close queue");
    }
    #[test]
    fn close_cancels_pending_tickets() {
        let queue = Queue::new(1).expect("valid queue");
        let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = queue
            .submit_with("Running".into(), {
                let started = Arc::clone(&started);
                let release = Arc::clone(&release);
                move |_ticket| {
                    started.store(true, std::sync::atomic::Ordering::Release);
                    while !release.load(std::sync::atomic::Ordering::Acquire) {
                        std::thread::yield_now();
                    }
                }
            })
            .expect("running ticket");
        while !started.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::yield_now();
        }
        let pending = queue.submit("Pending".into()).expect("pending ticket");
        assert!(queue.close(1).is_err());
        assert_eq!(pending.status(), CANCELLED);
        release.store(true, std::sync::atomic::Ordering::Release);
        running.wait(1_000).expect("running task completes");
        queue.close(1_000).expect("close after drain");
    }

    #[test]
    fn ticket_rejects_output_above_registry_bound() {
        let queue = Queue::new(1).expect("valid queue");
        let ticket = queue.submit("Work".into()).expect("ticket");
        let maximum = crate::config::dispatch_limits().output_max_bytes;

        assert!(ticket.set_output("x".repeat(maximum + 1)).is_err());
        assert_eq!(ticket.take_output(), "");
    }

    #[test]
    fn queue_rejects_the_ticket_after_the_pending_bound() {
        let queue = Queue::new(1).expect("valid queue");
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        for _ in 0..crate::config::dispatch_limits().pending_tickets_max {
            let release_for_job = Arc::clone(&release);
            queue
                .submit_with("Work".into(), move |_ticket| {
                    while !release_for_job.load(std::sync::atomic::Ordering::Acquire) {
                        std::thread::yield_now();
                    }
                })
                .expect("within pending bound");
        }
        assert!(matches!(
            queue.submit("Overflow".into()),
            Err(DispatchError::Saturated)
        ));
        release.store(true, std::sync::atomic::Ordering::Release);
        queue.close(1_000).expect("close saturated queue");
    }

    #[test]
    fn concurrent_queue_runs_two_jobs_at_once() {
        let queue = Queue::new(2).expect("valid queue");
        let rendezvous = Arc::new(std::sync::Barrier::new(2));
        for _ in 0..2 {
            let rendezvous = Arc::clone(&rendezvous);
            queue
                .submit_with("Work".into(), move |ticket| {
                    rendezvous.wait();
                    ticket.mark_completed();
                })
                .expect("job within queue bound");
        }
        queue.join(1_000).expect("both workers rendezvous");
    }

    #[test]
    fn panic_in_one_job_fails_its_ticket_and_keeps_worker_available() {
        let queue = Queue::new(1).expect("valid queue");
        let failed = queue
            .submit_with("Panic".into(), |_ticket| panic!("controlled task panic"))
            .expect("panic task submission");
        let completed = queue
            .submit_with("Later".into(), |ticket| {
                ticket.mark_completed();
            })
            .expect("later task submission");

        assert!(matches!(
            failed.wait(1_000),
            Err(DispatchError::TaskFailed(Some((1, message)))) if message.contains("panic")
        ));
        completed.wait(1_000).expect("later task survives panic");
        queue.close(1_000).expect("close after panic");
        assert!(queue.workers_joined_for_test());
    }

    #[test]
    fn close_disconnects_and_joins_idle_workers() {
        let queue = Queue::new(2).expect("valid queue");
        queue.close(1_000).expect("close idle workers");
        assert!(queue.workers_joined_for_test());
    }

    #[test]
    fn worker_cannot_join_or_close_its_own_queue() {
        let queue = Queue::new(1).expect("valid queue");
        let result = Arc::new(std::sync::Mutex::new(None));
        let ticket = queue
            .submit_with("SelfClose".into(), {
                let queue = queue.clone();
                let result = Arc::clone(&result);
                move |_ticket| {
                    *result
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) =
                        Some(queue.close(100));
                }
            })
            .expect("self-close ticket");
        ticket.wait(1_000).expect("self-close task completes");
        assert_eq!(
            result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take(),
            Some(Err(DispatchError::SelfJoin))
        );
        queue.close(1_000).expect("close after worker returns");
    }

    #[test]
    fn ticket_ids_are_opaque_and_not_sequential() {
        let queue = Queue::new(1).expect("valid queue");
        let first = queue.submit("First".into()).expect("first ticket");
        first.wait(1_000).expect("first completes");
        let second = queue.submit("Second".into()).expect("second ticket");
        second.wait(1_000).expect("second completes");
        assert_ne!(second.id(), first.id().wrapping_add(1));
        queue.close(1_000).expect("close queue");
    }

    #[test]
    fn close_deadline_does_not_claim_success_for_running_work() {
        let queue = Queue::new(1).expect("valid queue");
        let started = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let started_by_job = Arc::clone(&started);
        let release_for_job = Arc::clone(&release);
        queue
            .submit_with("Blocked".into(), move |ticket| {
                started_by_job.wait();
                release_for_job.wait();
                ticket.mark_completed();
            })
            .expect("blocked task submission");
        started.wait();

        assert_eq!(queue.close(1), Err(DispatchError::Timeout));
        assert!(!queue.workers_joined_for_test());
        release.wait();
        queue.close(1_000).expect("close after running task exits");
        assert!(queue.workers_joined_for_test());
    }

    #[test]
    fn cancelling_pending_ticket_prevents_user_code_from_running() {
        let queue = Queue::new(1).expect("valid queue");
        let started = Arc::new(std::sync::Barrier::new(2));
        let release = Arc::new(std::sync::Barrier::new(2));
        let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started_by_job = Arc::clone(&started);
        let release_for_job = Arc::clone(&release);
        queue
            .submit_with("Blocker".into(), move |ticket| {
                started_by_job.wait();
                release_for_job.wait();
                ticket.mark_completed();
            })
            .expect("blocker submission");
        started.wait();
        let ran_by_job = Arc::clone(&ran);
        let cancelled = queue
            .submit_with("Cancelled".into(), move |_ticket| {
                ran_by_job.store(true, std::sync::atomic::Ordering::Release);
            })
            .expect("queued submission");

        assert!(cancelled.cancel().expect("cancel pending task"));
        release.wait();
        queue.join(1_000).expect("cancelled queue joins");
        assert!(!ran.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(cancelled.status(), CANCELLED);
        queue.close(1_000).expect("close after cancellation");
    }

    #[test]
    fn completed_tickets_do_not_consume_future_pending_capacity() {
        let queue = Queue::new(1).expect("valid queue");
        for _ in 0..(crate::config::dispatch_limits().pending_tickets_max + 8) {
            let ticket = queue
                .submit_with("Short".into(), |_ticket| {})
                .expect("terminal ticket can be replaced");
            ticket.wait(1_000).expect("short task completes");
        }
        queue.close(1_000).expect("close after capacity reuse");
    }

    #[test]
    fn closing_ticket_preserves_terminal_failure_diagnostic() {
        let queue = Queue::new(1).expect("valid queue");
        let ticket = queue
            .submit_with("Failure".into(), |_ticket| panic!("diagnostic panic"))
            .expect("failure task submission");
        let _ = ticket.wait(1_000);
        ticket.close();
        assert_eq!(ticket.status(), FAILED);
        assert!(matches!(
            ticket.error(),
            Some((1, message)) if message.contains("panic")
        ));
        queue.close(1_000).expect("close after failure");
    }
}
