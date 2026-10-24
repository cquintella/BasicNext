//! Shared help text for the common options; each executable prepends its own
//! command list and appends its command-specific flags.

use std::process::ExitCode;

use crate::output::tool_error;

/// Help lines for the flags both executables honor. Interpreter options are
/// listed by `bni`, compiler options (`--emit`, `--target`, …) by `bnc`.
// The first line stays on the opening line: a `\` line continuation would
// also strip its indentation.
pub const COMMON_OPTIONS: &str =
    "  -v, --verbose              show pipeline stages (repeat for tokens: -v -v)
  -vv                        show stages and tokens
  --module-path <dir>        add an ordered import search directory (repeatable)
  --ffi-path <dir>           add an ordered dynamic library search directory (repeatable)
  --sandbox                  opt into filesystem root restrictions
  --read-root <dir>          allow reads below a sandbox root (repeatable)
  --write-root <dir>         allow writes below a sandbox root (repeatable)
  --warnings errors          promote all warning diagnostics to errors
  --allow <CODE>             suppress one warning diagnostic (repeatable)
  --warn <CODE>              keep one warning diagnostic at warning level
  --deny <CODE>              promote one warning diagnostic to an error
  --color auto|always|never  control ANSI color on status messages
  --log-level error|warn|info|debug
                             process log verbosity (build)
  --log-file <file>          companion process log path (build)
  --no-log                   disable the companion process log
  --config <file>            load warning/logging configuration
  -V, --version              print version
  -h, --help                 print this help
";

/// The tutorial book, where `--help` of both executables points for more
/// information.
pub const BOOK_URL: &str = "https://github.com/cquintella/basicnext-book";

/// Prints the one-line usage to stderr and returns the tool-error exit code.
#[must_use]
pub fn usage(line: &str) -> ExitCode {
    eprintln!("{line}");
    tool_error()
}

/// Reports an option the tool rejected (`CONFIG_INVALID: ` marks a
/// configuration error) and prints the tool's whole help, `help`, to stderr:
/// an option the tool does not own is unknown to it (each option has one
/// owner; `bni` and `bnc` each list their own).
#[must_use]
pub fn option_error(message: &str, help: &str) -> ExitCode {
    if let Some(message) = message.strip_prefix("CONFIG_INVALID: ") {
        eprintln!("error[CONFIG_INVALID]: {message}");
    } else {
        eprintln!("error: {message}");
    }
    eprint!("{help}");
    tool_error()
}

/// Reads and lexes the source of a command. One step both drivers take
/// before their own work.
///
/// # Errors
///
/// The exit code of the read.
pub fn tool_source(
    options: &crate::options::Options,
) -> Result<(bn_source::SourceFile, Vec<bn_frontend::token::Token>), ExitCode> {
    crate::frontend::read_source(options)
}
