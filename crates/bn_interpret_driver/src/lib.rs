//! Interpretation driver: `run` and `eval` over the shared frontend
//! preparation, HOST environment composition, and the registries of shipped
//! HOST capabilities and `BN*` libraries (behind `lib-*` features).
//! Executables (`bn`, future `bni`) and DAP compose from here.

pub mod cli;
pub mod environment;
pub(crate) mod eval;
pub(crate) mod hosts;
pub mod libraries;
pub mod options;
pub mod run;
pub(crate) mod session_server;
