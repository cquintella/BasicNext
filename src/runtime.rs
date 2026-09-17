//! Facade over `bn_interp` (bucket 0.5.1d SPRINT 2): the language core lives
//! in its own crate; `bn` adds the providers it ships.

pub use bn_interp::*;

/// The providers this build of `bn` ships. A `HostEnv` starts with **empty**
/// registries (the language alone); the binaries and tests add the shipped
/// `BN*` libraries and HOST capabilities through this trait, so the core
/// never names a provider (bucket 0.5.1d, D-F2-07).
pub trait HostEnvDefaults {
    #[must_use]
    fn with_default_providers(self) -> Self;
}

impl HostEnvDefaults for HostEnv {
    fn with_default_providers(self) -> Self {
        self.with_libraries(crate::libraries::default_libraries())
            .with_hosts(crate::hosts::default_hosts())
    }
}
