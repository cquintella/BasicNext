// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNJson` `Error`s (`language/0.6/bnjson.md` "Errors"), one producer for
//! both backends: `json_abi` returns a [`JsonFailure`], the interpreter turns
//! it into an `Error` value and the C ABI records it for the emitted code.

use bn_types::error_codes::json;

/// Where a read or write looked in a document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Place {
    Key(String),
    Index(i64),
}

/// Why a `BNJson` operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JsonFailure {
    /// The `Json` handle was released. Not an `Error` under `bni`
    /// (`USE_AFTER_RELEASE`); natively the call still returns `Error`.
    InvalidHandle,
    /// No member under the key.
    Missing(String),
    /// The value at `place` is `found`, not `expected`.
    WrongKind {
        place: Place,
        expected: &'static str,
        found: &'static str,
    },
    /// A key write into a non-object, an index write or append into a
    /// non-array, or `Length` of a scalar.
    NotContainer {
        expected: &'static str,
        found: &'static str,
    },
    /// An index outside an array of `length` elements.
    OutOfRange { index: i64, length: usize },
    /// A write that would nest the document past the depth limit.
    Depth,
    /// A NaN or infinite `FLOAT`.
    NonFinite,
    /// A document moved into itself.
    SelfMove,
    /// `Parse` rejected the text; the parser's reason.
    Parse(String),
    /// `Stringify` output past its bound; the reason.
    Stringify(String),
    /// The member is not provided.
    Unavailable,
}

impl JsonFailure {
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::InvalidHandle | Self::NonFinite | Self::SelfMove => json::INVALID_ARGUMENT,
            Self::Missing(_) => json::NOT_FOUND,
            Self::WrongKind { .. } | Self::NotContainer { .. } => json::TYPE_MISMATCH,
            Self::OutOfRange { .. } => json::OUT_OF_RANGE,
            Self::Depth | Self::Stringify(_) => json::LIMIT,
            Self::Unavailable => json::UNAVAILABLE,
            Self::Parse(_) => json::PARSE_FAILED,
        }
    }

    /// `Error.Message`: what failed, naming the key or index.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidHandle => "cannot use the Json document".into(),
            Self::Missing(key) => format!("no member \"{}\"", shown(key)),
            Self::WrongKind {
                place: Place::Key(key),
                expected,
                ..
            } => format!("member \"{}\" is not of type {expected}", shown(key)),
            Self::WrongKind {
                place: Place::Index(index),
                expected,
                ..
            } => format!("element {index} is not of type {expected}"),
            Self::NotContainer { expected, found } => {
                format!(
                    "the document is {} {found}, not an {expected}",
                    article(found)
                )
            }
            Self::OutOfRange { index, .. } => format!("no element {index}"),
            Self::Depth => "cannot write the value".into(),
            Self::NonFinite => "cannot write a non-finite FLOAT".into(),
            Self::SelfMove => "cannot move a document into itself".into(),
            Self::Parse(_) => "cannot parse the text as JSON".into(),
            Self::Stringify(_) => "cannot stringify the document".into(),
            Self::Unavailable => "the operation is not provided".into(),
        }
    }

    /// `Error.Cause`: the violated rule.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::InvalidHandle => "the document was released".into(),
            Self::Missing(_) => "the object has no member with that key".into(),
            Self::WrongKind { found, .. } => {
                format!("the value there is {} {found}", article(found))
            }
            Self::NotContainer { expected, .. } => {
                format!("only an {expected} accepts this operation")
            }
            Self::OutOfRange { length, .. } => {
                format!("the index must be at least 0 and less than the array length {length}")
            }
            Self::Depth => format!(
                "the document would nest deeper than {} levels",
                crate::json::MAX_DEPTH
            ),
            Self::NonFinite => "JSON has no NaN or infinity".into(),
            Self::SelfMove => "a value lives in exactly one place".into(),
            Self::Parse(reason) | Self::Stringify(reason) => reason.clone(),
            Self::Unavailable => "this build does not provide it".into(),
        }
    }
}

/// The kind name `Kind` reports for `value`.
#[must_use]
pub const fn kind_of(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Null => "null",
    }
}

/// "a" or "an" before a kind name.
fn article(kind: &str) -> &'static str {
    if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

fn shown(key: &str) -> String {
    let head: String = key.chars().take(64).collect();
    if key.chars().count() > 64 {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonFailure, Place};
    use bn_types::error_codes::json;

    #[test]
    fn failures_carry_code_message_and_cause() {
        let missing = JsonFailure::Missing("name".into());
        assert_eq!(missing.code(), json::NOT_FOUND);
        assert_eq!(missing.message(), "no member \"name\"");
        let wrong = JsonFailure::WrongKind {
            place: Place::Index(1),
            expected: "STRING",
            found: "number",
        };
        assert_eq!(wrong.code(), json::TYPE_MISMATCH);
        assert_eq!(wrong.message(), "element 1 is not of type STRING");
        assert_eq!(wrong.cause(), "the value there is a number");
        let range = JsonFailure::OutOfRange {
            index: 5,
            length: 2,
        };
        assert_eq!(range.code(), json::OUT_OF_RANGE);
        assert_eq!(
            range.cause(),
            "the index must be at least 0 and less than the array length 2"
        );
        assert_eq!(JsonFailure::Parse("x".into()).code(), json::PARSE_FAILED);
    }
}
