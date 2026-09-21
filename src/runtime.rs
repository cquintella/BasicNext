//! Facade over `bn_interp` plus the shipped-provider composition from
//! `bn_interpret_driver` (bucket 0.6.0, 1.2a): `bn::runtime::HostEnvDefaults`
//! keeps working for tests and DAP until they move to the owning crate (1.4).

pub use bn_interp::*;
pub use bn_interpret_driver::environment::HostEnvDefaults;
