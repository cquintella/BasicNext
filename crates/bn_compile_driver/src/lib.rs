//! Compilation driver: build orchestration over the shared validated IR,
//! compiler-only options, clang/wasm-ld/`libbn_rt.a` discovery and artifact
//! linking. Executables (`bn`, future `bnc`) call `build::build`.
//! No interpreter crate is reachable from here.

pub mod artifact;
pub mod build;
pub mod options;
pub mod toolchain;

#[cfg(test)]
mod tests;
