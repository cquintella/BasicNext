// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` shared core: directory operations, entry error classification,
//! and descriptor-relative Unix entry operations.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

pub mod entry;
mod kind;
#[cfg(unix)]
pub mod secure;

pub use entry::{
    EntryFailure, FsEntryPolicy, copy, create_directory, entry_failure, get_current_directory,
    is_directory, move_entry, open_new, remove_directory, set_current_directory,
};
pub use kind::{EntryKind, OpenMode, PolicyRefusal, denied, is_policy_refusal};
