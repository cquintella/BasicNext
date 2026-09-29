// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNDispatch` `Error`s (`language/0.6/bndispatch.md` "Errors"), one producer
//! for both backends: the interpreter turns a [`DispatchFailure`] into an
//! `Error` value, the C ABI records it for the emitted code.

use bn_types::error_codes::dispatch;

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
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
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

    /// `Error.Message`: what failed, with the value that identifies it.
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

    /// `Error.Cause`: the violated rule.
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

#[cfg(test)]
mod tests {
    use super::DispatchFailure;
    use bn_types::error_codes::dispatch;

    #[test]
    fn failures_carry_code_message_and_cause() {
        let range = DispatchFailure::OutOfRange {
            what: "worker count",
            value: 0,
            min: 1,
            max: 64,
        };
        assert_eq!(range.code(), dispatch::INVALID_ARGUMENT);
        assert_eq!(range.message(), "cannot use 0 as the worker count");
        assert_eq!(range.cause(), "the worker count must be from 1 through 64");
        let failed = DispatchFailure::TaskFailed {
            code: 1,
            message: "ASC requires a non-empty STRING".into(),
        };
        assert_eq!(failed.code(), dispatch::TASK_FAILED);
        assert_eq!(
            failed.cause(),
            "the worker returned Error 1: ASC requires a non-empty STRING"
        );
        assert_eq!(DispatchFailure::NotOwner.code(), dispatch::INVALID_STATE);
        assert_eq!(
            DispatchFailure::Closed("ticket").cause(),
            "the ticket was closed by Close"
        );
    }
}
