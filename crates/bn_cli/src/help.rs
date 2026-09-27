//! Shared help text for the common options; each executable prepends its own
//! command list and appends its command-specific flags.

use std::process::ExitCode;

use crate::output::tool_error;

/// Help lines for every flag `parse_options` understands without an extension.
pub const COMMON_OPTIONS: &str = "\
  -v, --verbose              show pipeline stages (repeat for tokens: -v -v)
  -vv                        show stages and tokens
  --emit tokens|ast|typed-ast|ir
                             print a frontend artifact (use with check or lex)
  -o, --output <file>        write an emitted artifact to <file>
  --trace                    report the execution entry point
  --module-path <dir>        add an ordered import search directory (repeatable)
  --no-filesystem            deny HOST.FileSystem imports (run only)
  --sandbox                  opt into filesystem root restrictions
  --read-root <dir>          allow reads below a sandbox root (repeatable)
  --write-root <dir>         allow writes below a sandbox root (repeatable)
  --warnings errors           promote all warning diagnostics to errors
  --allow <CODE>              suppress one warning diagnostic (repeatable)
  --warn <CODE>               keep one warning diagnostic at warning level
  --deny <CODE>               promote one warning diagnostic to an error
  --color auto|always|never  control ANSI color on status messages
  --log-level error|warn|info|debug
                             process log verbosity (build)
  --log-file <file>          companion process log path (build)
  --no-log                   disable the companion process log
  --config <file>            load warning/logging configuration
  -V, --version              print version
  -h, --help                 print this help
";

/// Prints the one-line usage to stderr and returns the tool-error exit code.
#[must_use]
pub fn usage(line: &str) -> ExitCode {
    eprintln!("{line}");
    tool_error()
}
