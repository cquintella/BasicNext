// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNDispatch` errors and failure definitions.

#![allow(clippy::missing_errors_doc)]

use std::time::{Duration, Instant};

/// Why a `BNDispatch` operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchFailure {
    /// A count or timeout outside its bound; `what` names it.
    OutOfRange {
        what: &'static str,
        value: i128,
        min: i128,
        max: i128,
    },
    /// The wait did not finish within `ms` milliseconds.
    Timeout { ms: i128 },
    /// The queue or ticket (`what`) is closed.
    Closed(&'static str),
    /// The queue already holds `pending` pending tickets.
    Saturated { pending: usize },
    /// The ticket was cancelled before it ran.
    Cancelled,
    /// The worker returned `Error` `code` with `message`.
    TaskFailed { code: i64, message: String },
    /// `Group.Leave` with the count at 0.
    GroupUnderflow,
    /// `Semaphore.Release` with all `permits` available.
    ReleaseWithEveryPermit { permits: usize },
    /// `Mutex.Unlock` by a task that does not hold the lock.
    NotOwner,
    /// A queue joined or closed from one of its own workers.
    SelfWait,
    /// The system cannot provide what the operation needs (`reason`).
    Unavailable(&'static str),
}

impl DispatchFailure {
    #[must_use]
    pub const fn code(&self) -> i32 {
        use bn_types::error_codes::dispatch;
        match self {
            Self::OutOfRange { .. } => dispatch::INVALID_ARGUMENT,
            Self::Timeout { .. } => dispatch::TIMEOUT,
            Self::Closed(_) => dispatch::CLOSED,
            Self::Saturated { .. } => dispatch::SATURATED,
            Self::Cancelled => dispatch::CANCELLED,
            Self::TaskFailed { .. } => dispatch::TASK_FAILED,
            Self::GroupUnderflow
            | Self::ReleaseWithEveryPermit { .. }
            | Self::NotOwner
            | Self::SelfWait => dispatch::INVALID_STATE,
            Self::Unavailable(_) => dispatch::UNAVAILABLE,
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::OutOfRange { what, value, .. } => format!("cannot use {value} as the {what}"),
            Self::Timeout { ms } => format!("timed out after {ms} ms"),
            Self::Closed(what) => format!("the {what} is closed"),
            Self::Saturated { .. } => "cannot submit the task".into(),
            Self::Cancelled => "the task was cancelled".into(),
            Self::TaskFailed { .. } => "the task failed".into(),
            Self::GroupUnderflow => "cannot leave the group".into(),
            Self::ReleaseWithEveryPermit { .. } => "cannot release the semaphore".into(),
            Self::NotOwner => "cannot unlock the mutex".into(),
            Self::SelfWait => "cannot wait for the queue from its own worker".into(),
            Self::Unavailable(_) => "the operation is not available".into(),
        }
    }

    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::OutOfRange { what, min, max, .. } => {
                format!("the {what} must be from {min} through {max}")
            }
            Self::Timeout { .. } => "the operation did not complete within its timeout".into(),
            Self::Closed("queue") => "a closed queue accepts no more work".into(),
            Self::Closed(what) => format!("the {what} was closed by Close"),
            Self::Saturated { pending } => {
                format!("the queue already holds {pending} pending tickets")
            }
            Self::Cancelled => "Cancel, or the queue's Close, removed it before it ran".into(),
            Self::TaskFailed { code, message } => {
                format!("the worker returned Error {code}: {message}")
            }
            Self::GroupUnderflow => {
                "the group count is already 0: each Leave needs an Enter".into()
            }
            Self::ReleaseWithEveryPermit { permits: 1 } => {
                "its only permit is already available".into()
            }
            Self::ReleaseWithEveryPermit { permits } => {
                format!("all {permits} permits are already available")
            }
            Self::NotOwner => "only the task that locked the mutex can unlock it".into(),
            Self::SelfWait => "the worker would wait for itself".into(),
            Self::Unavailable(reason) => (*reason).into(),
        }
    }
}

/// The instant `timeout_ms` from now; the timeout must be within bounds.
pub fn deadline(timeout_ms: i128) -> Result<Instant, DispatchFailure> {
    let limits = bn_limits::dispatch_limits();
    let (min, max) = (limits.timeout_min_ms, limits.timeout_max_ms);
    if !(min..=max).contains(&timeout_ms) {
        return Err(DispatchFailure::OutOfRange {
            what: "timeout in milliseconds",
            value: timeout_ms,
            min,
            max,
        });
    }
    Ok(Instant::now() + Duration::from_millis(u64::try_from(timeout_ms).unwrap_or_default()))
}

fn count(what: &'static str, value: i128, max: usize) -> Result<usize, DispatchFailure> {
    usize::try_from(value)
        .ok()
        .filter(|count| (1..=max).contains(count))
        .ok_or(DispatchFailure::OutOfRange {
            what,
            value,
            min: 1,
            max: i128::try_from(max).unwrap_or(i128::MAX),
        })
}

pub fn worker_count(value: i128) -> Result<usize, DispatchFailure> {
    count(
        "worker count",
        value,
        bn_limits::dispatch_limits().worker_count_max,
    )
}

pub fn auto_worker_count() -> Result<usize, DispatchFailure> {
    std::thread::available_parallelism()
        .map(|count| {
            count
                .get()
                .min(bn_limits::dispatch_limits().worker_count_max)
        })
        .map_err(|_| DispatchFailure::Unavailable("the system reports no processor count"))
}
