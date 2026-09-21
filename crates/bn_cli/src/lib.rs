//! Shared command-line services for Basic Next executables: common options
//! and configuration, the CLI adapter over frontend preparation, `check`,
//! diagnostic presentation, output helpers and the companion process log.
//! No backend (interpreter, LLVM) or concrete HOST provider is reachable
//! from this crate.

pub mod check;
mod config;
pub mod diagnostics;
pub mod frontend;
pub mod help;
pub mod options;
pub mod output;
pub mod process_log;
