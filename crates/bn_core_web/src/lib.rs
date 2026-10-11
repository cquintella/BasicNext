// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb` core shared by the interpreter (`bn_lib_web`) and compiled
//! binaries (`bn_rt` `web_abi`): HTTP/1.1 and HTTP/2 transport (`hyper`),
//! TLS (`rustls`), routing, sessions, cookies, ACLs and egress policy, over
//! the `HOST.Net` core (`bn_core_net`). It holds no BN value: each adapter
//! converts at its boundary (bucket 0.6.5 E, D1).

#![allow(clippy::missing_errors_doc)] // Errors are plain `String` / `&str` messages the adapters map.

pub mod http;
#[cfg(test)]
mod test_support;
pub mod tls;
pub mod web;
pub mod web_state;
