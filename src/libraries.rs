//! `BN*` library modules served through the provider seam. Each library is a
//! separate concern from the language; `default_libraries` is what the CLI
//! registers on a `HostEnv`.

pub mod data;
pub mod json;
pub mod log;
pub mod math;

use std::sync::Arc;

use crate::runtime::provider::Libraries;

/// The libraries this build of `bn` ships. Feature-gated per library once the
/// crates split (bucket 0.5.1d SPRINT 3).
#[must_use]
pub fn default_libraries() -> Libraries {
    let mut libraries = Libraries::default();
    libraries.register(math::NAME, Arc::new(|| Box::new(math::MathProvider)));
    libraries.register(
        json::NAME,
        Arc::new(|| Box::new(json::JsonProvider::default())),
    );
    libraries.register(
        log::NAME,
        Arc::new(|| Box::new(log::LogProvider::default())),
    );
    libraries.register(
        data::NAME,
        Arc::new(|| Box::new(data::DataProvider::default())),
    );
    libraries
}
