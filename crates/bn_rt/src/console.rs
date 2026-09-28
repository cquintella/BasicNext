// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::io::{self, IsTerminal, Write};

use crate::terminal::terminal_dimensions;

const CLS: &str = "\x1b[2J\x1b[H";
const BEEP: &str = "\x07";

/// Console provider failure, with the same codes/messages as `bn run`.
#[derive(Debug)]
pub enum ConsoleError {
    Unavailable(&'static str),
    /// `PrintAt` outside the window (1-based position; window size).
    OutOfBounds {
        column: i128,
        row: i128,
        columns: i128,
        rows: i128,
    },
    Output(io::Error),
    Overflow,
}

impl ConsoleError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "HOST_CAPABILITY_UNAVAILABLE",
            Self::OutOfBounds { .. } => "INDEX_OUT_OF_BOUNDS",
            Self::Output(_) => "OUTPUT_ERROR",
            Self::Overflow => "NUMERIC_OVERFLOW",
        }
    }

    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Unavailable(message) => (*message).into(),
            Self::OutOfBounds { .. } => "console coordinate is outside the window".into(),
            Self::Output(error) => error.to_string(),
            Self::Overflow => "result does not fit INTEGER".into(),
        }
    }
}

impl ConsoleError {
    /// The failure with its identity's facts, for both backends (`bni`
    /// builds its diagnostic from it; natively the call site prints it).
    #[must_use]
    pub fn failure(&self) -> crate::RuntimeFailure {
        let facts = match self {
            Self::Unavailable(detail) => vec![("detail", (*detail).to_owned())],
            Self::OutOfBounds {
                column,
                row,
                columns,
                rows,
            } => vec![
                ("index", format!("({column}, {row})")),
                ("bound", format!("{columns} columns by {rows} rows")),
                ("context", "the console window".to_owned()),
            ],
            Self::Output(error) => vec![("message", format!("cannot write output: {error}"))],
            Self::Overflow => vec![(
                "operation",
                "converting the window size to INTEGER".to_owned(),
            )],
        };
        crate::RuntimeFailure {
            code: self.code(),
            facts,
        }
    }
}

/// Write the CLS sequence (`ESC[2J ESC[H`).
///
/// # Errors
///
/// Returns [`ConsoleError::Output`] when the writer fails.
pub fn cls(out: &mut dyn Write) -> Result<(), ConsoleError> {
    write!(out, "{CLS}").map_err(ConsoleError::Output)?;
    out.flush().map_err(ConsoleError::Output)
}

/// Write the BEL character.
///
/// # Errors
///
/// Returns [`ConsoleError::Output`] when the writer fails.
pub fn beep(out: &mut dyn Write) -> Result<(), ConsoleError> {
    write!(out, "{BEEP}").map_err(ConsoleError::Output)?;
    out.flush().map_err(ConsoleError::Output)
}

/// Positioned write; requires a TTY and in-window coordinates.
///
/// # Errors
///
/// Returns a TTY/bounds/output error matching the interpreter.
pub fn print_at(
    out: &mut dyn Write,
    column: i128,
    row: i128,
    text: &str,
) -> Result<(), ConsoleError> {
    require_tty("PrintAt requires a TTY")?;
    let (cols, rows) = dimensions()?;
    let width = i128::try_from(text.chars().count()).unwrap_or(i128::MAX);
    if column < 1 || row < 1 || row > rows || column > cols || width > cols - column + 1 {
        return Err(ConsoleError::OutOfBounds {
            column,
            row,
            columns: cols,
            rows,
        });
    }
    write!(out, "\x1b[{row};{column}H{text}").map_err(ConsoleError::Output)?;
    out.flush().map_err(ConsoleError::Output)
}

/// Terminal width in columns.
///
/// # Errors
///
/// Returns a TTY or overflow error matching the interpreter.
pub fn num_cols() -> Result<i32, ConsoleError> {
    require_tty("window size requires a TTY")?;
    let (cols, _) = dimensions()?;
    i32::try_from(cols).map_err(|_| ConsoleError::Overflow)
}

/// Terminal height in rows.
///
/// # Errors
///
/// Returns a TTY or overflow error matching the interpreter.
pub fn num_rows() -> Result<i32, ConsoleError> {
    require_tty("window size requires a TTY")?;
    let (_, rows) = dimensions()?;
    i32::try_from(rows).map_err(|_| ConsoleError::Overflow)
}

fn require_tty(message: &'static str) -> Result<(), ConsoleError> {
    if io::stdout().is_terminal() {
        Ok(())
    } else {
        Err(ConsoleError::Unavailable(message))
    }
}

fn dimensions() -> Result<(i128, i128), ConsoleError> {
    terminal_dimensions().ok_or(ConsoleError::Unavailable(
        "terminal dimensions are unavailable",
    ))
}

#[cfg(test)]
mod tests {
    use super::ConsoleError;

    /// Each failure names its identity's schema arguments, which the
    /// interpreter and the native site both render (`bn_diag` schemas:
    /// `DETAIL`, `INDEX_OUT_OF_BOUNDS`, `LEGACY_MESSAGE`, `OPERATION`).
    #[test]
    fn failures_carry_the_facts_of_their_identity() {
        let failure = ConsoleError::OutOfBounds {
            column: 0,
            row: 1,
            columns: 80,
            rows: 24,
        }
        .failure();
        assert_eq!(failure.code, "INDEX_OUT_OF_BOUNDS");
        assert_eq!(
            failure.facts,
            vec![
                ("index", "(0, 1)".to_owned()),
                ("bound", "80 columns by 24 rows".to_owned()),
                ("context", "the console window".to_owned()),
            ]
        );
        let names = |error: ConsoleError| {
            let failure = error.failure();
            (
                failure.code,
                failure
                    .facts
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            names(ConsoleError::Unavailable("PrintAt requires a TTY")),
            ("HOST_CAPABILITY_UNAVAILABLE", vec!["detail"])
        );
        assert_eq!(
            names(ConsoleError::Output(std::io::Error::other("closed"))),
            ("OUTPUT_ERROR", vec!["message"])
        );
        assert_eq!(
            names(ConsoleError::Overflow),
            ("NUMERIC_OVERFLOW", vec!["operation"])
        );
    }
}
