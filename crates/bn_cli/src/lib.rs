//! Shared command-line services for Basic Next executables: common options
//! and configuration, the CLI adapter over frontend preparation, `check`,
//! diagnostic presentation, output helpers and the companion process log.
//! No backend (interpreter, LLVM) or concrete HOST provider is reachable
//! from this crate.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "command-line interface presents output and diagnostics on stdout and stderr"
)]
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

pub mod check;
mod config;
pub mod diagnostics;
pub mod frontend;
pub mod help;
pub mod options;
pub mod output;
pub mod process_log;
