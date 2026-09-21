//! Interpretation driver: `run` and `eval` over the shared frontend
//! preparation, HOST environment composition, and the registries of shipped
//! HOST capabilities and `BN*` libraries (behind `lib-*` features).
//! Executables (`bn`, future `bni`) and DAP compose from here.

pub mod environment;
pub mod eval;
pub mod hosts;
pub mod libraries;
pub mod run;
