//! Interpretation driver: `run` and `eval` over the shared frontend
//! preparation, HOST environment composition, and the registries of shipped
//! HOST capabilities and `BN*` libraries (behind `lib-*` features).
//! Executables (`bn`, future `bni`) and DAP compose from here.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "interpreter driver runs interactive CLI execution and session responses"
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

pub mod cli;
pub mod environment;
pub(crate) mod eval;
pub(crate) mod hosts;
pub mod interactive;
pub mod libraries;
pub mod options;
pub mod run;
pub(crate) mod session_server;
