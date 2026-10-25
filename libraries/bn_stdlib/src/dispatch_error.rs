// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNDispatch` `Error`s (`language/0.6/bndispatch.md` "Errors"), one producer
//! for both backends: the interpreter turns a [`DispatchFailure`] into an
//! `Error` value, the C ABI records it for the emitted code.

pub use bn_core_dispatch::DispatchFailure;

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
