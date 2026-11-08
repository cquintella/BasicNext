// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` shared core: directory operations, entry error classification,
//! and descriptor-relative Unix entry operations.

pub mod entry;
mod kind;
#[cfg(unix)]
pub mod secure;

pub use entry::{
    EntryFailure, FsEntryPolicy, copy, create_directory, entry_failure, get_current_directory,
    is_directory, move_entry, open_new, remove_directory, set_current_directory,
};
pub use kind::{EntryKind, OpenMode, PolicyRefusal, denied, is_policy_refusal};
