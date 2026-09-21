//! Common output helpers and exit-code classification: language errors exit
//! 1, tool/configuration errors exit 2.

use std::{
    fmt, fs,
    io::{self, IsTerminal},
    process::ExitCode,
};

use bn_frontend::token::Token;

use crate::options::Color;

/// The program or its configuration is invalid in the language's terms.
#[must_use]
pub fn language_error() -> ExitCode {
    ExitCode::from(1)
}

/// The toolchain, an external tool, or the command line failed.
#[must_use]
pub fn tool_error() -> ExitCode {
    ExitCode::from(2)
}

#[must_use]
pub fn color_enabled(color: Color) -> bool {
    matches!(color, Color::Always) || (matches!(color, Color::Auto) && io::stdout().is_terminal())
}

#[must_use]
pub fn colorize(text: &str, color: Color) -> String {
    if color_enabled(color) {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.into()
    }
}

/// Prints a pipeline-stage message to stderr when `verbosity >= level`.
pub fn log(verbosity: u8, level: u8, message: impl fmt::Display) {
    if verbosity >= level {
        eprintln!("[bn] {message}");
    }
}

#[must_use]
pub fn module_index(id: u32) -> usize {
    usize::try_from(id).unwrap_or(usize::MAX)
}

#[must_use]
pub fn tokens_text(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| {
            format!(
                "{}:{}:{} {:?}",
                token.span.start.line, token.span.start.column, token.span.end.column, token.kind
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Writes an emitted artifact to `path`, or to stdout when `path` is `None`.
#[must_use]
pub fn emit_output(output: String, path: Option<&str>) -> ExitCode {
    if let Some(path) = path {
        return match fs::write(path, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: cannot write {path}: {error}");
                tool_error()
            }
        };
    }
    print!("{output}");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::{Color, colorize};

    #[test]
    fn success_color_can_be_disabled() {
        assert_eq!(colorize("ok", Color::Never), "ok");
        assert!(colorize("ok", Color::Always).contains("ok"));
    }
}
