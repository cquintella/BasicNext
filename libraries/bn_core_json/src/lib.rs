// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bn_core_json` — Basic Next JSON domain core.
//!
//! Bounded parse and stringify, the per-task document table, and `JsonFailure`.
//! `bn_lib_json` and `bn_rt::json_abi` both call this crate.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]

mod documents;
mod error;
mod parse;

pub use documents::{
    append_moved, append_value, array, clone_document, document, element, get_json, get_json_at,
    get_string, get_value, has, kind, length, move_into, move_into_at, number, object,
    parse_document, release, retain, set_at, set_value, store, stringify_document,
};
pub use error::{JsonFailure, Place};
pub use parse::parse;
