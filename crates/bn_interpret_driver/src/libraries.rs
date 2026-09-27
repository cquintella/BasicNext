// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Registry of the `BN*` library providers this build of `bn` ships. Each
//! library is its own crate (`bn_lib_*`) behind a Cargo feature; a build
//! without a feature simply does not register that library, and a program
//! importing it gets `LIBRARY_PROVIDER_UNAVAILABLE` at the first call.

use bn_interp::provider::Providers;

/// The libraries this build of `bn` ships (bucket 0.5.1d, D-F2-07).
#[must_use]
pub fn default_libraries() -> Providers {
    #[allow(unused_mut)] // With no library feature the registry stays empty.
    let mut libraries = Providers::default();
    #[cfg(feature = "lib-math")]
    libraries.register(
        bn_lib_math::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_math::MathProvider)),
    );
    #[cfg(feature = "lib-json")]
    libraries.register(
        bn_lib_json::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_json::JsonProvider)),
    );
    #[cfg(feature = "lib-log")]
    libraries.register(
        bn_lib_log::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_log::LogProvider::default())),
    );
    #[cfg(feature = "lib-data")]
    libraries.register(
        bn_lib_data::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_data::DataProvider::default())),
    );
    #[cfg(feature = "lib-dispatch")]
    libraries.register(
        bn_lib_dispatch::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_dispatch::DispatchProvider::default())),
    );
    #[cfg(feature = "lib-web")]
    libraries.register(
        bn_lib_web::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_web::WebProvider::default())),
    );
    #[cfg(feature = "lib-crypto")]
    libraries.register(
        bn_lib_crypto::NAME,
        std::sync::Arc::new(|| Box::new(bn_lib_crypto::CryptoProvider::default())),
    );
    libraries
}
