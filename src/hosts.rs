// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST` capabilities served through the provider seam. `HOST` is part of
//! the language (`docs/language/0.5/0.5.md`); each capability is its own
//! provider so a build can ship the language without, say, networking.
//! `BN*` libraries live under `crate::libraries` and never mix with these.

pub mod net;

use std::sync::Arc;

use crate::runtime::provider::Providers;

/// The HOST capabilities this build of `bn` ships, keyed by the segment after
/// `HOST.` (`"Net"`).
#[must_use]
pub fn default_hosts() -> Providers {
    let mut hosts = Providers::default();
    hosts.register(
        net::NAME,
        Arc::new(|| Box::new(net::NetProvider::default())),
    );
    hosts
}
