// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNData` `Error`s (`language/0.6/bndata.md` "Errors"), one producer for both
//! backends: the interpreter turns a [`DataFailure`] into an `Error` value, the
//! C ABI records it for the emitted code.

use bn_types::error_codes::data;

/// Why a `BNData` operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataFailure {
    /// The `DataFrame` handle was released or is invalid.
    InvalidHandle,
    /// Invalid argument supplied to `DataFrame` operation.
    InvalidArgument(String),
    /// Invalid CSV separator (must be single character, not quote or newline).
    InvalidSeparator(String),
    /// Negative slice bound or index.
    NegativeBound { bound: &'static str, value: i64 },
    /// Destination length mismatch in column copy.
    DestinationLengthMismatch { expected: usize, got: usize },
    /// Column length mismatch when adding a column.
    ColumnLengthMismatch { expected: usize, got: usize },
    /// Row counts differ when appending columns.
    RowCountsDiffer { left: usize, right: usize },
    /// Column layouts differ when appending rows.
    ColumnLayoutsDiffer,
    /// Duplicate column name or label.
    DuplicateColumn(String),
    /// Statistical reduction on an empty numeric column.
    EmptyNumericColumn,
    /// Column name not found.
    ColumnNotFound(String),
    /// Key column not found in join.
    KeyColumnNotFound { key: String, side: &'static str },
    /// Column is not of the expected type.
    TypeMismatch {
        expected: &'static str,
        column: String,
    },
    /// Numeric conversion of a column cell failed.
    ConversionFailed { column: String, reason: String },
    /// Column types differ between frames in `AppendRows`.
    ColumnTypesDiffer { column: String },
    /// Column contains NA where none is allowed.
    ContainsNa(String),
    /// Column is not numeric for reductions or z-score.
    NonNumericColumn(String),
    /// Row index out of bounds.
    RowIndexOutOfRange { row: i64, count: usize },
    /// Column index out of bounds.
    ColumnIndexOutOfRange { column: i64, count: usize },
    /// Slice range exceeds frame dimension.
    SliceOutOfRange {
        start: usize,
        len: usize,
        total: usize,
        dim: &'static str,
    },
    /// File read or write failed.
    IoFailed(String),
    /// Ragged CSV row (inconsistent field count).
    RaggedRow { expected: usize, got: usize },
    /// Unterminated quoted field in CSV.
    UnterminatedQuotedField,
    /// Unparsable CSV / invalid format.
    InvalidFormat(String),
}

impl DataFailure {
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::InvalidHandle
            | Self::InvalidArgument(_)
            | Self::InvalidSeparator(_)
            | Self::NegativeBound { .. }
            | Self::DestinationLengthMismatch { .. }
            | Self::ColumnLengthMismatch { .. }
            | Self::RowCountsDiffer { .. }
            | Self::ColumnLayoutsDiffer
            | Self::DuplicateColumn(_)
            | Self::EmptyNumericColumn => data::INVALID_ARGUMENT,

            Self::ColumnNotFound(_) | Self::KeyColumnNotFound { .. } => data::NOT_FOUND,

            Self::TypeMismatch { .. }
            | Self::ConversionFailed { .. }
            | Self::ColumnTypesDiffer { .. }
            | Self::ContainsNa(_)
            | Self::NonNumericColumn(_) => data::TYPE_MISMATCH,

            Self::RowIndexOutOfRange { .. }
            | Self::ColumnIndexOutOfRange { .. }
            | Self::SliceOutOfRange { .. } => data::OUT_OF_RANGE,

            Self::IoFailed(_) => data::IO_FAILED,

            Self::RaggedRow { .. } | Self::UnterminatedQuotedField | Self::InvalidFormat(_) => {
                data::INVALID_FORMAT
            }
        }
    }

    /// `Error.Message`: what failed.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidHandle => "the DataFrame handle is invalid".into(),
            Self::InvalidArgument(reason) => reason.clone(),
            Self::InvalidSeparator(s) => format!("invalid CSV separator \"{s}\""),
            Self::NegativeBound { bound, value } => {
                format!("cannot use {value} as the {bound}")
            }
            Self::DestinationLengthMismatch { expected, got } => {
                format!("destination length {got} does not match column length {expected}")
            }
            Self::ColumnLengthMismatch { expected, got } => {
                format!("column length {got} does not match frame row count {expected}")
            }
            Self::RowCountsDiffer { left, right } => {
                format!("row counts differ: left frame has {left}, right frame has {right}")
            }
            Self::ColumnLayoutsDiffer => "column layouts differ between frames".into(),
            Self::DuplicateColumn(name) => format!("duplicate column name \"{name}\""),
            Self::EmptyNumericColumn => "numeric column is empty".into(),
            Self::ColumnNotFound(name) => format!("column \"{name}\" not found"),
            Self::KeyColumnNotFound { key, side } => {
                format!("{side} key column \"{key}\" not found")
            }
            Self::TypeMismatch { expected, column } => {
                format!("column \"{column}\" is not of type {expected}")
            }
            Self::ConversionFailed { column, reason } => {
                format!("cannot convert column \"{column}\": {reason}")
            }
            Self::ColumnTypesDiffer { column } => {
                format!("column \"{column}\" types differ between frames")
            }
            Self::ContainsNa(name) => format!("column \"{name}\" contains NA values"),
            Self::NonNumericColumn(name) => format!("column \"{name}\" is not numeric"),
            Self::RowIndexOutOfRange { row, count } => {
                format!("row index {row} out of range for frame with {count} rows")
            }
            Self::ColumnIndexOutOfRange { column, count } => {
                format!("column index {column} out of range for frame with {count} columns")
            }
            Self::SliceOutOfRange {
                start,
                len,
                total,
                dim,
            } => {
                format!(
                    "slice {dim} range {start}..{} out of bounds for total {total}",
                    start + len
                )
            }
            Self::IoFailed(reason) => format!("CSV I/O failed: {reason}"),
            Self::RaggedRow { expected, got } => {
                format!("ragged CSV row: expected {expected} fields, got {got}")
            }
            Self::UnterminatedQuotedField => "unterminated quoted field in CSV".into(),
            Self::InvalidFormat(reason) => format!("invalid CSV format: {reason}"),
        }
    }

    /// `Error.Cause`: the violated rule.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::InvalidHandle => "the handle was released or was not created by BNData".into(),
            Self::InvalidArgument(reason)
            | Self::ConversionFailed { reason, .. }
            | Self::IoFailed(reason)
            | Self::InvalidFormat(reason) => reason.clone(),
            Self::InvalidSeparator(_) => {
                "a CSV separator must be a single UTF-8 character other than '\"', '\\r', or '\\n'"
                    .into()
            }
            Self::NegativeBound { bound, .. } => {
                format!("the {bound} must not be negative")
            }
            Self::DestinationLengthMismatch { .. } => {
                "the destination array must have the same length as the column".into()
            }
            Self::ColumnLengthMismatch { .. } => {
                "every column in a DataFrame must have the same number of rows".into()
            }
            Self::RowCountsDiffer { .. } => {
                "AppendColumns requires frames with equal row counts".into()
            }
            Self::ColumnLayoutsDiffer => {
                "AppendRows requires frames with identical column names and ordering".into()
            }
            Self::DuplicateColumn(_) => "column names in a DataFrame must be unique".into(),
            Self::EmptyNumericColumn => "statistical reductions require at least one row".into(),
            Self::ColumnNotFound(_) => {
                "the DataFrame does not contain a column with that name".into()
            }
            Self::KeyColumnNotFound { side, .. } => {
                format!("the {side} frame does not contain the specified key column")
            }
            Self::TypeMismatch { expected, .. } => {
                format!("the requested operation requires a {expected} column")
            }
            Self::ColumnTypesDiffer { .. } => "AppendRows requires matching column types".into(),
            Self::ContainsNa(_) => "copying to a typed buffer does not permit NA cells".into(),
            Self::NonNumericColumn(_) => {
                "statistics and z-score operations require an integer or float column".into()
            }
            Self::RowIndexOutOfRange { count, .. } => {
                format!(
                    "the row index must be from 0 through {}",
                    count.saturating_sub(1)
                )
            }
            Self::ColumnIndexOutOfRange { count, .. } => {
                format!(
                    "the column index must be from 0 through {}",
                    count.saturating_sub(1)
                )
            }
            Self::SliceOutOfRange { dim, .. } => {
                format!("the slice bounds must not exceed the frame {dim} count")
            }
            Self::RaggedRow { .. } => {
                "every row in a CSV file must have the same number of fields".into()
            }
            Self::UnterminatedQuotedField => {
                "quoted fields in CSV must have a closing quote".into()
            }
        }
    }

    /// Construct a [`DataFailure`] from a runtime error string.
    #[must_use]
    pub fn from_message(message: &str) -> Self {
        match message {
            "duplicate column name" | "duplicate column label" => {
                Self::DuplicateColumn("duplicate column name".into())
            }
            "column length mismatch" => Self::ColumnLengthMismatch {
                expected: 0,
                got: 0,
            },
            "column layouts differ" => Self::ColumnLayoutsDiffer,
            "row counts differ" => Self::RowCountsDiffer { left: 0, right: 0 },
            "column not found" => Self::ColumnNotFound(String::new()),
            "empty numeric column" => Self::EmptyNumericColumn,
            "destination length mismatch" => Self::DestinationLengthMismatch {
                expected: 0,
                got: 0,
            },
            "column type or NA mismatch" => Self::ContainsNa(String::new()),
            "column is not numeric" => Self::NonNumericColumn(String::new()),
            "negative slice bound" => Self::NegativeBound {
                bound: "slice bound",
                value: -1,
            },
            "DataFrame index out of bounds" => Self::RowIndexOutOfRange { row: -1, count: 0 },
            "column index out of bounds" => Self::ColumnIndexOutOfRange {
                column: -1,
                count: 0,
            },
            "ragged CSV row" => Self::RaggedRow {
                expected: 0,
                got: 0,
            },
            "unterminated quoted field" => Self::UnterminatedQuotedField,
            s if s.contains("left key") => Self::KeyColumnNotFound {
                key: s.into(),
                side: "left",
            },
            s if s.contains("right key") => Self::KeyColumnNotFound {
                key: s.into(),
                side: "right",
            },
            s if s.contains("types differ") => Self::ColumnTypesDiffer { column: s.into() },
            other => Self::InvalidFormat(other.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DataFailure;
    use bn_types::error_codes::data;

    #[test]
    fn failures_carry_code_message_and_cause() {
        let sep = DataFailure::InvalidSeparator("\"".into());
        assert_eq!(sep.code(), data::INVALID_ARGUMENT);
        assert_eq!(sep.message(), "invalid CSV separator \"\"\"");
        assert_eq!(
            sep.cause(),
            "a CSV separator must be a single UTF-8 character other than '\"', '\\r', or '\\n'"
        );

        let not_found = DataFailure::ColumnNotFound("Price".into());
        assert_eq!(not_found.code(), data::NOT_FOUND);
        assert_eq!(not_found.message(), "column \"Price\" not found");

        let mismatch = DataFailure::TypeMismatch {
            expected: "INTEGER",
            column: "Name".into(),
        };
        assert_eq!(mismatch.code(), data::TYPE_MISMATCH);

        let out = DataFailure::RowIndexOutOfRange { row: 10, count: 5 };
        assert_eq!(out.code(), data::OUT_OF_RANGE);

        let io = DataFailure::IoFailed("disk error".into());
        assert_eq!(io.code(), data::IO_FAILED);

        let format = DataFailure::UnterminatedQuotedField;
        assert_eq!(format.code(), data::INVALID_FORMAT);
    }
}
