// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Registry of the `HOST` capability providers. `HOST` is part of the language
//! (`docs/language/0.5/0.5.md`), so every capability is always built: the
//! large ones are crates (`bn_host_net`, `bn_host_fs`, and `bn_host_exec`
//! behind the `exec` shell here), the small shells (`clock`, `random`,
//! `console`, `exec`) live in this directory. `BN*` libraries are registered
//! in `crate::libraries` (same crate) and never mix with these.

pub mod clock;
pub mod console;
pub mod exec;
pub mod random;

use std::sync::Arc;

use bn_interp::provider::Providers;

/// The HOST capabilities this build of `bn` ships, keyed by the segment after
/// `HOST.` (`"Net"`, `"FileSystem"`, …). `HOST.NumProcs`/`HOST.Args` are
/// language surface and stay in the core.
#[must_use]
pub fn default_hosts() -> Providers {
    let mut hosts = Providers::default();
    hosts.register(
        bn_host_net::NAME,
        Arc::new(|| Box::new(bn_host_net::NetProvider::default())),
    );
    hosts.register(
        bn_host_fs::NAME,
        Arc::new(|| Box::new(bn_host_fs::FsProvider::default())),
    );
    hosts.register(exec::NAME, Arc::new(|| Box::new(exec::ExecProvider)));
    hosts.register(clock::NAME, Arc::new(|| Box::new(clock::ClockProvider)));
    hosts.register(random::NAME, Arc::new(|| Box::new(random::RandomProvider)));
    hosts.register(
        console::NAME,
        Arc::new(|| Box::new(console::ConsoleProvider)),
    );
    hosts
}
