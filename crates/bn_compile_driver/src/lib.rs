//! Compilation driver: build orchestration over the shared validated IR,
//! compiler-only options, clang/`libbn_rt.a` discovery and artifact
//! linking. Executables (`bn`, future `bnc`) call `build::build`.
//! No interpreter crate is reachable from here.

#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "compiler driver presents build progress and artifact output on stdout and stderr"
)]
#![allow(
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    reason = "DEC-55 tier 3"
)]

pub mod artifact;
pub mod build;
pub mod cli;
pub mod options;
pub(crate) mod toolchain;

#[cfg(test)]
mod tests;
