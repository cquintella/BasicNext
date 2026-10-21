//! Compilation driver: build orchestration over the shared validated IR,
//! compiler-only options, clang/`libbn_rt.a` discovery and artifact
//! linking. Executables (`bn`, future `bnc`) call `build::build`.
//! No interpreter crate is reachable from here.

pub mod artifact;
pub mod build;
pub mod cli;
pub mod options;
pub(crate) mod toolchain;

#[cfg(test)]
mod tests;
