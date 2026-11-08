// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Common filesystem entry types: open mode and entry kind.

use std::fmt;
use std::io;

/// Open mode for `HOST.FileSystem` files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenMode {
    Read,
    Write,
    Append,
}

/// What a path names, read without following a final symbolic link.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// A refusal of the rooted walk or of the execution policy itself.
#[derive(Debug)]
pub struct PolicyRefusal(pub &'static str);

impl fmt::Display for PolicyRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for PolicyRefusal {}

/// An `io::Error` wrapping a [`PolicyRefusal`].
#[must_use]
pub fn denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, PolicyRefusal(message))
}

/// Whether `error` wraps a [`PolicyRefusal`].
#[must_use]
pub fn is_policy_refusal(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(<dyn std::error::Error + Send + Sync>::is::<PolicyRefusal>)
}
