// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The directory, private-create, copy and move operations of
//! `HOST.FileSystem` (language/0.6/host.md, "Directories, private files, copy
//! and move"): one implementation for the
//! interpreter provider and the native C ABI, over the entry operations of
//! [`FsEntryPolicy`], which never follow a final symbolic link. The kind of an
//! entry is read first only to name the right failure; the act itself still
//! refuses a link that takes its place.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use crate::kind::EntryKind;

/// Entry operations required by the shared filesystem entry logic.
pub trait FsEntryPolicy {
    /// Whether `path` is allowed for read or write.
    fn allows_path(&self, path: &Path, write: bool) -> bool;

    /// The denial reason if a path is not allowed.
    fn denial_reason(&self, write: bool) -> &'static str;

    /// What `path` names, without following a final symbolic link (a read).
    ///
    /// # Errors
    ///
    /// Returns an error if reading the entry kind fails.
    fn entry_kind(&self, path: &Path) -> io::Result<EntryKind>;

    /// What `path` names, checked against the write roots: the kind a write
    /// acts on, without requiring a read root.
    ///
    /// # Errors
    ///
    /// Returns an error if reading the writable entry kind fails.
    fn writable_entry_kind(&self, path: &Path) -> io::Result<EntryKind>;

    /// Changes the working directory to `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if entering the directory fails.
    fn change_directory(&self, path: &Path) -> io::Result<()>;

    /// Creates an owner-only directory at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if creating the directory fails.
    fn create_private_directory(&self, path: &Path) -> io::Result<()>;

    /// Creates an owner-only file at `path`, returning it open for write.
    ///
    /// # Errors
    ///
    /// Returns an error if creating the file fails.
    fn create_private_file(&self, path: &Path) -> io::Result<File>;

    /// Removes an empty directory at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if removing the directory fails.
    fn remove_empty_directory(&self, path: &Path) -> io::Result<()>;

    /// Opens an entry for reading without following a final symbolic link.
    ///
    /// # Errors
    ///
    /// Returns an error if opening the entry fails.
    fn open_entry(&self, path: &Path) -> io::Result<File>;

    /// Removes a file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if removing the file fails.
    fn remove_file(&self, path: &Path) -> io::Result<()>;

    /// Renames `source` to `destination`.
    ///
    /// # Errors
    ///
    /// Returns an error if renaming the entry fails.
    fn rename_entry(&self, source: &Path, destination: &Path) -> io::Result<()>;

    /// Whether `path` names a policy root itself, which is never a write target.
    fn names_a_root(&self, path: &Path) -> bool;
}

/// Why an entry operation failed.
#[derive(Debug)]
pub enum EntryFailure {
    AlreadyExists,
    NotDirectory { symlink: bool },
    NotEmpty,
    CrossDevice,
    PolicyDenied(&'static str),
    Directory,
    NotRegularSource,
    Io(io::Error),
}

impl From<io::Error> for EntryFailure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn failure(
    policy: &impl FsEntryPolicy,
    path: &Path,
    write: bool,
    error: io::Error,
) -> EntryFailure {
    if error.kind() != io::ErrorKind::PermissionDenied {
        EntryFailure::Io(error)
    } else if !policy.allows_path(path, write) {
        EntryFailure::PolicyDenied(policy.denial_reason(write))
    } else if path.is_dir() {
        EntryFailure::Directory
    } else {
        EntryFailure::Io(error)
    }
}

/// Classifies a failed entry operation on `path`: the entry codes of 0.6.12
/// first, then the classification `FS.Open` uses (policy, directory, system).
pub fn entry_failure(
    policy: &impl FsEntryPolicy,
    path: &Path,
    write: bool,
    error: io::Error,
) -> EntryFailure {
    match error.kind() {
        io::ErrorKind::AlreadyExists => EntryFailure::AlreadyExists,
        io::ErrorKind::NotADirectory => EntryFailure::NotDirectory { symlink: false },
        io::ErrorKind::DirectoryNotEmpty => EntryFailure::NotEmpty,
        io::ErrorKind::CrossesDevices => EntryFailure::CrossDevice,
        _ => failure(policy, path, write, error),
    }
}

/// A policy root is never a write target.
fn root_refusal() -> EntryFailure {
    EntryFailure::PolicyDenied(bn_catalog::text("file", "entry.root_not_target"))
}

/// The failure for an entry that must be a directory but is `kind`.
const fn not_a_directory(kind: EntryKind) -> Option<EntryFailure> {
    match kind {
        EntryKind::Directory => None,
        EntryKind::Symlink => Some(EntryFailure::NotDirectory { symlink: true }),
        EntryKind::File | EntryKind::Other => Some(EntryFailure::NotDirectory { symlink: false }),
    }
}

/// The text of a path the system returned.
fn path_text(path: PathBuf) -> Result<String, EntryFailure> {
    path.into_os_string().into_string().map_err(|_| {
        EntryFailure::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            bn_catalog::text("file", "entry.path_not_text.cause"),
        ))
    })
}

/// `FS.GetCurrentDirectory()`: the absolute path of the working directory.
///
/// # Errors
///
/// `POLICY_DENIED` when the working directory is outside every read root.
pub fn get_current_directory(policy: &impl FsEntryPolicy) -> Result<String, EntryFailure> {
    let directory = std::env::current_dir().map_err(EntryFailure::Io)?;
    if !policy.allows_path(&directory, false) {
        return Err(EntryFailure::PolicyDenied(policy.denial_reason(false)));
    }
    path_text(directory)
}

/// `FS.SetCurrentDirectory(path)`: enters the directory, never through a
/// symbolic link (a read of that directory).
///
/// # Errors
///
/// `IS_NOT_DIRECTORY` for a file or a link, `NOT_FOUND`, `POLICY_DENIED`.
pub fn set_current_directory(policy: &impl FsEntryPolicy, path: &Path) -> Result<(), EntryFailure> {
    let kind = policy
        .entry_kind(path)
        .map_err(|error| entry_failure(policy, path, false, error))?;
    if let Some(failure) = not_a_directory(kind) {
        return Err(failure);
    }
    policy
        .change_directory(path)
        .map_err(|error| entry_failure(policy, path, false, error))
}

/// `FS.CreateDirectory(path)`: one owner-only directory.
///
/// # Errors
///
/// `ALREADY_EXISTS`, `NOT_FOUND` for a missing parent, `PERMISSION_DENIED`
/// when the result is not owner-only, `POLICY_DENIED`.
pub fn create_directory(policy: &impl FsEntryPolicy, path: &Path) -> Result<(), EntryFailure> {
    policy
        .create_private_directory(path)
        .map_err(|error| entry_failure(policy, path, true, error))
}

/// `FS.OpenNew(path)`: one owner-only file, open for writing.
///
/// # Errors
///
/// As [`create_directory`].
pub fn open_new(policy: &impl FsEntryPolicy, path: &Path) -> Result<File, EntryFailure> {
    policy
        .create_private_file(path)
        .map_err(|error| entry_failure(policy, path, true, error))
}

/// `FS.RemoveDirectory(path)`: one empty directory; a link or a file is not
/// removed, and a policy root is never a target.
///
/// # Errors
///
/// `IS_NOT_DIRECTORY`, `DIRECTORY_NOT_EMPTY`, `NOT_FOUND`, `POLICY_DENIED`.
pub fn remove_directory(policy: &impl FsEntryPolicy, path: &Path) -> Result<(), EntryFailure> {
    if policy.names_a_root(path) {
        return Err(root_refusal());
    }
    let kind = policy
        .writable_entry_kind(path)
        .map_err(|error| entry_failure(policy, path, true, error))?;
    if let Some(failure) = not_a_directory(kind) {
        return Err(failure);
    }
    policy
        .remove_empty_directory(path)
        .map_err(|error| entry_failure(policy, path, true, error))
}

/// `FS.IsDirectory(path)`: `TRUE` only for a directory; a file or a link is
/// `FALSE`.
///
/// # Errors
///
/// `NOT_FOUND` for a missing path, `POLICY_DENIED`.
pub fn is_directory(policy: &impl FsEntryPolicy, path: &Path) -> Result<bool, EntryFailure> {
    policy
        .entry_kind(path)
        .map(|kind| kind == EntryKind::Directory)
        .map_err(|error| entry_failure(policy, path, false, error))
}

/// `FS.Copy(source, destination)`: the bytes of one regular file into a new
/// owner-only file; no mode, owner, special bit, access-control list or time
/// is copied, and a failed copy leaves no destination.
///
/// # Errors
///
/// `INVALID_ARGUMENT` for a link source, `IS_DIRECTORY`, `NOT_FOUND`,
/// `ALREADY_EXISTS`, `POLICY_DENIED`, `IO_FAILED`.
pub fn copy(
    policy: &impl FsEntryPolicy,
    source: &Path,
    destination: &Path,
) -> Result<(), EntryFailure> {
    match policy.entry_kind(source) {
        Ok(EntryKind::File) => {}
        Ok(EntryKind::Directory) => return Err(EntryFailure::Directory),
        Ok(EntryKind::Symlink | EntryKind::Other) => return Err(EntryFailure::NotRegularSource),
        Err(error) => return Err(entry_failure(policy, source, false, error)),
    }
    let mut input = policy
        .open_entry(source)
        .map_err(|error| entry_failure(policy, source, false, error))?;
    // The kind was read before the open; the open file decides (a FIFO or a
    // device swapped in meanwhile is not copied).
    if !input.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(EntryFailure::NotRegularSource);
    }
    let mut output = policy
        .create_private_file(destination)
        .map_err(|error| entry_failure(policy, destination, true, error))?;
    if let Err(error) = io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
        drop(output);
        // The copy already failed and is reported; a destination that cannot
        // be removed stays owner-only.
        let _ = policy.remove_file(destination);
        return Err(EntryFailure::Io(error));
    }
    Ok(())
}

/// `FS.Move(source, destination)`: renames one entry on the same file
/// system, replacing a regular file only; no copy across file systems.
///
/// # Errors
///
/// `ALREADY_EXISTS` for a directory or a link at the destination,
/// `NOT_FOUND`, `IO_FAILED` across file systems, `POLICY_DENIED`.
pub fn move_entry(
    policy: &impl FsEntryPolicy,
    source: &Path,
    destination: &Path,
) -> Result<(), EntryFailure> {
    if policy.names_a_root(source) || policy.names_a_root(destination) {
        return Err(root_refusal());
    }
    policy
        .writable_entry_kind(source)
        .map_err(|error| entry_failure(policy, source, true, error))?;
    policy.rename_entry(source, destination).map_err(|error| {
        // A denial names the path the policy refuses; any other failure is
        // about the destination.
        let named = if policy.allows_path(source, true) {
            destination
        } else {
            source
        };
        entry_failure(policy, named, true, error)
    })
}

#[cfg(test)]
mod tests {
    use super::{EntryFailure, EntryKind, FsEntryPolicy, entry_failure};
    use std::fs::File;
    use std::io;
    use std::path::Path;

    struct UnrestrictedPolicy;

    impl FsEntryPolicy for UnrestrictedPolicy {
        fn allows_path(&self, _path: &Path, _write: bool) -> bool {
            true
        }
        fn denial_reason(&self, _write: bool) -> &'static str {
            "unrestricted"
        }
        fn entry_kind(&self, _path: &Path) -> io::Result<EntryKind> {
            Ok(EntryKind::File)
        }
        fn writable_entry_kind(&self, _path: &Path) -> io::Result<EntryKind> {
            Ok(EntryKind::File)
        }
        fn change_directory(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }
        fn create_private_directory(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }
        fn create_private_file(&self, _path: &Path) -> io::Result<File> {
            Err(io::ErrorKind::Unsupported.into())
        }
        fn remove_empty_directory(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }
        fn open_entry(&self, _path: &Path) -> io::Result<File> {
            Err(io::ErrorKind::Unsupported.into())
        }
        fn remove_file(&self, _path: &Path) -> io::Result<()> {
            Ok(())
        }
        fn rename_entry(&self, _source: &Path, _destination: &Path) -> io::Result<()> {
            Ok(())
        }
        fn names_a_root(&self, _path: &Path) -> bool {
            false
        }
    }

    /// `EXDEV` (`ERROR_NOT_SAME_DEVICE`) is hard to provoke in a test; its
    /// kind maps to `IO_FAILED` with no copy.
    #[test]
    fn system_kinds_map_to_the_entry_failures() {
        let policy = UnrestrictedPolicy;
        let path = Path::new("entry");
        let classify = |kind| entry_failure(&policy, path, true, io::Error::from(kind));
        assert!(matches!(
            classify(io::ErrorKind::CrossesDevices),
            EntryFailure::CrossDevice
        ));
        assert!(matches!(
            classify(io::ErrorKind::AlreadyExists),
            EntryFailure::AlreadyExists
        ));
        assert!(matches!(
            classify(io::ErrorKind::DirectoryNotEmpty),
            EntryFailure::NotEmpty
        ));
        assert!(matches!(
            classify(io::ErrorKind::NotADirectory),
            EntryFailure::NotDirectory { symlink: false }
        ));
        assert!(matches!(
            classify(io::ErrorKind::NotFound),
            EntryFailure::Io(_)
        ));
    }
}
