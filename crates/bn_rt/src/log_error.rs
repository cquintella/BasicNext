// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` `Error`s (`language/0.6/bnlog.md` "Errors"), one producer for both
//! backends: the interpreter turns a [`LogFailure`] into an `Error` value, the
//! C ABI records it for the emitted code.

pub use bn_core_log::LogFailure;

#[cfg(test)]
mod tests {
    use super::LogFailure;
    use bn_types::error_codes::log;

    #[test]
    fn failures_carry_code_message_and_cause() {
        let level = LogFailure::OutOfRange {
            what: "log level",
            value: 9,
            min: 0,
            max: 6,
        };
        assert_eq!(level.code(), log::INVALID_ARGUMENT);
        assert_eq!(level.message(), "cannot use 9 as the log level");
        assert_eq!(level.cause(), "the log level must be from 0 through 6");

        let dup = LogFailure::DuplicateKey("foo".into());
        assert_eq!(dup.code(), log::DUPLICATE);
        assert_eq!(dup.message(), "key \"foo\" already exists");
        assert_eq!(dup.cause(), "the key \"foo\" is already present");

        let limit = LogFailure::LimitExceeded {
            what: "transports",
            count: 8,
            max: 8,
        };
        assert_eq!(limit.code(), log::LIMIT);
        assert_eq!(LogFailure::Closed.code(), log::CLOSED);
        assert_eq!(
            LogFailure::IoFailed("disk full".into()).code(),
            log::IO_FAILED
        );
        assert_eq!(
            LogFailure::CapabilityRequired("HOST.Console").code(),
            log::CAPABILITY_REQUIRED
        );
        assert_eq!(LogFailure::Timeout { ms: 100 }.code(), log::TIMEOUT);
        assert_eq!(
            LogFailure::Unavailable("no provider").code(),
            log::UNAVAILABLE
        );
    }
}
