// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The directory, private-create, copy and move operations of
//! `HOST.FileSystem` (language/0.6/host.md, "Directories, private files, copy
//! and move"; bucket 0.6.12, FS-CORE-2): one implementation for the
//! interpreter provider and the native C ABI, over the entry operations of
//! [`FsPolicy`], which never follow a final symbolic link. The kind of an
//! entry is read first only to name the right failure; the act itself still
//! refuses a link that takes its place.

use std::io;
use std::path::{Path, PathBuf};

use super::{OpenFile, failure, text};
use crate::file_error::{Failure, FileError, FsOperation};
use crate::policy::FsPolicy;
use crate::secure_fs::EntryKind;

/// Classifies a failed entry operation on `path`: the codes of bucket 0.6.12
/// first, then the classification `FS.Open` uses (policy, directory, system).
fn entry_failure(policy: &FsPolicy, path: &Path, write: bool, error: io::Error) -> Failure {
    match error.kind() {
        io::ErrorKind::AlreadyExists => Failure::AlreadyExists,
        io::ErrorKind::NotADirectory => Failure::NotDirectory { symlink: false },
        io::ErrorKind::DirectoryNotEmpty => Failure::NotEmpty,
        io::ErrorKind::CrossesDevices => Failure::CrossDevice,
        _ => failure(policy, path, write, error),
    }
}

/// A policy root is never a write target (bucket 0.6.12, G-3).
fn root_refusal() -> Failure {
    Failure::PolicyDenied(bn_catalog::text("file", "entry.root_not_target"))
}

/// The failure for an entry that must be a directory but is `kind`.
const fn not_a_directory(kind: EntryKind) -> Option<Failure> {
    match kind {
        EntryKind::Directory => None,
        EntryKind::Symlink => Some(Failure::NotDirectory { symlink: true }),
        EntryKind::File | EntryKind::Other => Some(Failure::NotDirectory { symlink: false }),
    }
}

/// The text of a path the system returned.
fn path_text(path: PathBuf) -> Result<String, Failure> {
    path.into_os_string().into_string().map_err(|_| {
        Failure::Io(io::Error::new(
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
pub fn get_current_directory(policy: &FsPolicy) -> Result<String, FileError> {
    let fail = |failure| FileError::on_process(FsOperation::GetCurrentDirectory, failure);
    let directory = std::env::current_dir().map_err(|error| fail(Failure::Io(error)))?;
    if !policy.allows_path(&directory, false) {
        return Err(fail(Failure::PolicyDenied(policy.denial_reason(false))));
    }
    path_text(directory).map_err(fail)
}

/// `FS.SetCurrentDirectory(path)`: enters the directory, never through a
/// symbolic link (a read of that directory).
///
/// # Errors
///
/// `IS_NOT_DIRECTORY` for a file or a link, `NOT_FOUND`, `POLICY_DENIED`.
pub fn set_current_directory(policy: &FsPolicy, path: &Path) -> Result<(), FileError> {
    let fail =
        |failure| FileError::on_path(FsOperation::SetCurrentDirectory, &text(path), None, failure);
    let kind = policy
        .entry_kind(path)
        .map_err(|error| fail(entry_failure(policy, path, false, error)))?;
    if let Some(failure) = not_a_directory(kind) {
        return Err(fail(failure));
    }
    policy
        .change_directory(path)
        .map_err(|error| fail(entry_failure(policy, path, false, error)))
}

/// `FS.CreateDirectory(path)`: one owner-only directory.
///
/// # Errors
///
/// `ALREADY_EXISTS`, `NOT_FOUND` for a missing parent, `PERMISSION_DENIED`
/// when the result is not owner-only, `POLICY_DENIED`.
pub fn create_directory(policy: &FsPolicy, path: &Path) -> Result<(), FileError> {
    policy.create_private_directory(path).map_err(|error| {
        FileError::on_path(
            FsOperation::CreateDirectory,
            &text(path),
            None,
            entry_failure(policy, path, true, error),
        )
    })
}

/// `FS.OpenNew(path)`: one owner-only file, open for writing.
///
/// # Errors
///
/// As [`create_directory`].
pub fn open_new(policy: &FsPolicy, path: &Path) -> Result<OpenFile, FileError> {
    let file = policy.create_private_file(path).map_err(|error| {
        FileError::on_path(
            FsOperation::OpenNew,
            &text(path),
            None,
            entry_failure(policy, path, true, error),
        )
    })?;
    Ok(OpenFile {
        file: Some(file),
        path: Some(text(path)),
        writable: true,
        family: None,
        buffer: Vec::new(),
        buffer_pos: 0,
    })
}

/// `FS.RemoveDirectory(path)`: one empty directory; a link or a file is not
/// removed, and a policy root is never a target.
///
/// # Errors
///
/// `IS_NOT_DIRECTORY`, `DIRECTORY_NOT_EMPTY`, `NOT_FOUND`, `POLICY_DENIED`.
pub fn remove_directory(policy: &FsPolicy, path: &Path) -> Result<(), FileError> {
    let fail =
        |failure| FileError::on_path(FsOperation::RemoveDirectory, &text(path), None, failure);
    if policy.names_a_root(path) {
        return Err(fail(root_refusal()));
    }
    let kind = policy
        .writable_entry_kind(path)
        .map_err(|error| fail(entry_failure(policy, path, true, error)))?;
    if let Some(failure) = not_a_directory(kind) {
        return Err(fail(failure));
    }
    policy
        .remove_empty_directory(path)
        .map_err(|error| fail(entry_failure(policy, path, true, error)))
}

/// `FS.IsDirectory(path)`: `TRUE` only for a directory; a file or a link is
/// `FALSE`.
///
/// # Errors
///
/// `NOT_FOUND` for a missing path, `POLICY_DENIED`.
pub fn is_directory(policy: &FsPolicy, path: &Path) -> Result<bool, FileError> {
    policy
        .entry_kind(path)
        .map(|kind| kind == EntryKind::Directory)
        .map_err(|error| {
            FileError::on_path(
                FsOperation::IsDirectory,
                &text(path),
                None,
                entry_failure(policy, path, false, error),
            )
        })
}

/// `FS.Copy(source, destination)`: the bytes of one regular file into a new
/// owner-only file; no mode, owner, special bit, access-control list or time
/// is copied, and a failed copy leaves no destination.
///
/// # Errors
///
/// `INVALID_ARGUMENT` for a link source, `IS_DIRECTORY`, `NOT_FOUND`,
/// `ALREADY_EXISTS`, `POLICY_DENIED`, `IO_FAILED`.
pub fn copy(policy: &FsPolicy, source: &Path, destination: &Path) -> Result<(), FileError> {
    let fail = |failure| {
        FileError::on_pair(
            FsOperation::Copy,
            &text(source),
            &text(destination),
            failure,
        )
    };
    match policy.entry_kind(source) {
        Ok(EntryKind::File) => {}
        Ok(EntryKind::Directory) => return Err(fail(Failure::Directory)),
        Ok(EntryKind::Symlink | EntryKind::Other) => return Err(fail(Failure::NotRegularSource)),
        Err(error) => return Err(fail(entry_failure(policy, source, false, error))),
    }
    let mut input = policy
        .open_entry(source)
        .map_err(|error| fail(entry_failure(policy, source, false, error)))?;
    // The kind was read before the open; the open file decides (a FIFO or a
    // device swapped in meanwhile is not copied).
    if !input.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Err(fail(Failure::NotRegularSource));
    }
    let mut output = policy
        .create_private_file(destination)
        .map_err(|error| fail(entry_failure(policy, destination, true, error)))?;
    if let Err(error) = io::copy(&mut input, &mut output).and_then(|_| output.sync_all()) {
        drop(output);
        // The copy already failed and is reported; a destination that cannot
        // be removed stays owner-only.
        let _ = policy.remove_file(destination);
        return Err(fail(Failure::Io(error)));
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
pub fn move_entry(policy: &FsPolicy, source: &Path, destination: &Path) -> Result<(), FileError> {
    let fail = |failure| {
        FileError::on_pair(
            FsOperation::Move,
            &text(source),
            &text(destination),
            failure,
        )
    };
    if policy.names_a_root(source) || policy.names_a_root(destination) {
        return Err(fail(root_refusal()));
    }
    policy
        .writable_entry_kind(source)
        .map_err(|error| fail(entry_failure(policy, source, true, error)))?;
    policy.rename_entry(source, destination).map_err(|error| {
        // A denial names the path the policy refuses; any other failure is
        // about the destination.
        let named = if policy.allows_path(source, true) {
            destination
        } else {
            source
        };
        fail(entry_failure(policy, named, true, error))
    })
}

#[cfg(test)]
mod tests {
    use super::{Failure, entry_failure};
    use crate::policy::FsPolicy;
    use std::io;
    use std::path::Path;

    /// `EXDEV` (`ERROR_NOT_SAME_DEVICE`) is hard to provoke in a test; its
    /// kind maps to `IO_FAILED` with no copy (bucket 0.6.12, R6).
    #[test]
    fn system_kinds_map_to_the_entry_failures() {
        let policy = FsPolicy::unrestricted();
        let path = Path::new("entry");
        let classify = |kind| entry_failure(&policy, path, true, io::Error::from(kind));
        assert!(matches!(
            classify(io::ErrorKind::CrossesDevices),
            Failure::CrossDevice
        ));
        assert!(matches!(
            classify(io::ErrorKind::AlreadyExists),
            Failure::AlreadyExists
        ));
        assert!(matches!(
            classify(io::ErrorKind::DirectoryNotEmpty),
            Failure::NotEmpty
        ));
        assert!(matches!(
            classify(io::ErrorKind::NotADirectory),
            Failure::NotDirectory { symlink: false }
        ));
        assert!(matches!(classify(io::ErrorKind::NotFound), Failure::Io(_)));
    }
}
