// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb` core shared by the interpreter (`bn_lib_web`) and compiled
//! binaries (`bn_rt` `web_abi`): HTTP/1.1 and HTTP/2 transport (`hyper`),
//! TLS (`rustls`), routing, sessions, cookies, ACLs and egress policy, over
//! the `HOST.Net` core (`bn_core_net`). It holds no BN value: each adapter
//! converts at its boundary.

#![allow(
    clippy::missing_errors_doc,
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

// The C ABI calls `bn_rt_*` symbols. The staticlib leaves them for the
// final link with `libbn_rt.a` / `libbn_stdlib.a`; the Rust test harness has to link the crates.
#[cfg(test)]
use bn_rt as _;
#[cfg(test)]
use bn_stdlib as _;

pub mod http;
#[cfg(test)]
mod test_support;
pub mod tls;
pub mod tls_roots;
pub mod web;
pub(crate) mod web_abi;
pub mod web_error;
pub mod web_state;
pub use web::*;
pub use web_error::WebFailure;
