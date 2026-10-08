// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Env` — the one environment-reading implementation both backends call
//! (bucket 0.6.5b). This crate holds the execution policy, portable failure codes,
//! and [`get`] / [`has`]. It contains no `unsafe` code and reads the process
//! environment via `std::env::var_os`.

use std::{env, ffi::OsString};

pub use bn_types::error_codes::env::*;

/// The execution policy in force for environment access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub allowed: bool,
}

impl Default for Policy {
    fn default() -> Self {
        Self { allowed: true }
    }
}

/// Why an environment operation produced `Error`: matches the fields of
/// the Basic Next Error contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: i32,
    pub operation: &'static str,
    pub message: String,
    pub cause: String,
}

impl Failure {
    pub fn new(
        code: i32,
        operation: &'static str,
        message: impl Into<String>,
        cause: impl Into<String>,
    ) -> Self {
        Self {
            code,
            operation,
            message: message.into(),
            cause: cause.into(),
        }
    }
}

fn check_name(name: &str, operation: &'static str) -> Result<(), Failure> {
    if name.is_empty() || name.contains('=') || name.contains('\0') {
        return Err(Failure::new(
            INVALID_NAME,
            operation,
            format!("cannot access environment variable with invalid name {name:?}"),
            "variable name is empty, contains '=', or contains NUL",
        ));
    }
    Ok(())
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn check_policy(policy: &Policy, operation: &'static str) -> Result<(), Failure> {
    if !policy.allowed {
        return Err(Failure::new(
            POLICY_DENIED,
            operation,
            "cannot read environment variable",
            "the execution policy denies environment access",
        ));
    }
    Ok(())
}

/// Reads the value of an environment variable.
///
/// Checked in order:
/// 1. Name is non-empty, contains no `=` and no NUL (`INVALID_NAME`).
/// 2. Policy allows the read (`POLICY_DENIED`).
/// 3. Variable is present (`NOT_SET`).
/// 4. Value is valid UTF-8 (`INVALID_UTF8`).
///
/// # Errors
///
/// Returns a [`Failure`] if the name is invalid, policy is denied, variable
/// is not set, or the value is not valid UTF-8.
pub fn get(name: &str, policy: &Policy) -> Result<String, Failure> {
    const OP: &str = "HOST.Env.Get";
    check_name(name, OP)?;
    check_policy(policy, OP)?;
    value_of(name, env::var_os(name))
}

/// Turns what the environment holds for `name` into the `Get` result:
/// absent → `NOT_SET`, not UTF-8 → `INVALID_UTF8`. Separate from the read so
/// both outcomes are testable without changing the process environment.
fn value_of(name: &str, value: Option<OsString>) -> Result<String, Failure> {
    const OP: &str = "HOST.Env.Get";
    let val = value.ok_or_else(|| {
        Failure::new(
            NOT_SET,
            OP,
            format!("environment variable \"{name}\" is not set"),
            "variable is not defined in the process environment",
        )
    })?;

    val.into_string().map_err(|_| {
        Failure::new(
            INVALID_UTF8,
            OP,
            format!("value of environment variable \"{name}\" is not valid UTF-8"),
            "os string conversion to UTF-8 failed",
        )
    })
}

/// Checks whether an environment variable is present in the environment.
///
/// Returns `Ok(true)` if set, `Ok(false)` if absent.
/// Errors if name is invalid or policy denies the read.
///
/// # Errors
///
/// Returns a [`Failure`] if the name is invalid or policy is denied.
pub fn has(name: &str, policy: &Policy) -> Result<bool, Failure> {
    const OP: &str = "HOST.Env.Has";
    check_name(name, OP)?;
    check_policy(policy, OP)?;

    Ok(env::var_os(name).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_name_checks() {
        let policy = Policy::default();
        for bad in ["", "FOO=BAR", "FOO\0BAR"] {
            let get_err = get(bad, &policy).unwrap_err();
            assert_eq!(get_err.code, INVALID_NAME);
            assert_eq!(get_err.operation, "HOST.Env.Get");

            let has_err = has(bad, &policy).unwrap_err();
            assert_eq!(has_err.code, INVALID_NAME);
            assert_eq!(has_err.operation, "HOST.Env.Has");
        }
    }

    #[test]
    fn policy_denied_checks() {
        let denied = Policy { allowed: false };
        let get_err = get("PATH", &denied).unwrap_err();
        assert_eq!(get_err.code, POLICY_DENIED);
        assert_eq!(get_err.operation, "HOST.Env.Get");

        let has_err = has("PATH", &denied).unwrap_err();
        assert_eq!(has_err.code, POLICY_DENIED);
        assert_eq!(has_err.operation, "HOST.Env.Has");
    }

    #[test]
    fn not_set_and_has_false() {
        let policy = Policy::default();
        let name = "BN_DEFINITELY_NONEXISTENT_VAR_12345";
        let get_err = get(name, &policy).unwrap_err();
        assert_eq!(get_err.code, NOT_SET);
        assert_eq!(get_err.operation, "HOST.Env.Get");

        let has_res = has(name, &policy).unwrap();
        assert!(!has_res);
    }

    #[test]
    fn get_returns_a_present_value() {
        // PATH is set on every supported host; the value itself varies.
        assert!(get("PATH", &Policy::default()).is_ok());
        assert_eq!(
            value_of("X", Some(OsString::from("value"))),
            Ok("value".to_string())
        );
        assert_eq!(value_of("X", Some(OsString::new())), Ok(String::new()));
    }

    #[test]
    fn get_reports_a_value_that_is_not_utf8() {
        #[cfg(unix)]
        let value = {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(vec![b'a', 0xff, b'b'])
        };
        #[cfg(windows)]
        let value = {
            use std::os::windows::ffi::OsStringExt;
            // An unpaired surrogate is valid in a Windows environment block
            // and is not UTF-8.
            OsString::from_wide(&[0x61, 0xD800, 0x62])
        };
        let failure = value_of("BN_BAD", Some(value)).unwrap_err();
        assert_eq!(failure.code, INVALID_UTF8);
        assert_eq!(failure.operation, "HOST.Env.Get");
    }

    #[test]
    fn name_is_checked_before_policy_and_policy_before_presence() {
        let denied = Policy { allowed: false };
        assert_eq!(get("", &denied).unwrap_err().code, INVALID_NAME);
        assert_eq!(has("A=B", &denied).unwrap_err().code, INVALID_NAME);
        assert_eq!(
            get("BN_DEFINITELY_NONEXISTENT_VAR_12345", &denied)
                .unwrap_err()
                .code,
            POLICY_DENIED
        );
    }
}
