// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Windows rooted access with the safe std API. Each component is opened by
//! its full path as the reparse point itself (`FILE_FLAG_OPEN_REPARSE_POINT`)
//! and rejected when it is one, so a symlink or junction is never followed.
//! Directory handles are opened without `FILE_SHARE_DELETE` and held until
//! the operation ends: while a handle is open inside a directory, neither it
//! nor any ancestor can be renamed (verified on NTFS), so the full path keeps
//! naming the components that were checked.

use super::{EntryKind, OpenMode, RootedDir, denied};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Component, Path, PathBuf};

const FILE_SHARE_READ: u32 = 0x1;
const FILE_SHARE_WRITE: u32 = 0x2;
const FILE_SHARE_DELETE: u32 = 0x4;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Directory,
    File(OpenMode),
    /// A file opened only to be checked before `remove_file`.
    Doomed,
}

/// Opens `path` as itself (never through a reparse point) and checks
/// that it is what `kind` expects.
pub(super) fn open_component(path: &Path, kind: Kind) -> io::Result<File> {
    let mut options = OpenOptions::new();
    let mut flags = FILE_FLAG_OPEN_REPARSE_POINT;
    match kind {
        Kind::Directory => {
            options
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
            flags |= FILE_FLAG_BACKUP_SEMANTICS;
        }
        Kind::Doomed | Kind::File(OpenMode::Read) => {
            options.read(true);
        }
        // Truncation waits for the checks below, so a reparse point is
        // never written through or cut.
        Kind::File(OpenMode::Write) => {
            options.write(true).create(true);
        }
        Kind::File(OpenMode::Append) => {
            options.append(true).create(true);
        }
    }
    if kind != Kind::Directory {
        options.share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE);
    }
    let file = options.custom_flags(flags).open(path)?;
    let metadata = file.metadata()?;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(denied("filesystem path traverses a symlink"));
    }
    match kind {
        Kind::Directory if !metadata.is_dir() => {
            Err(denied("filesystem path traverses a non-directory"))
        }
        Kind::File(_) | Kind::Doomed if !metadata.is_file() => {
            Err(denied("filesystem target is not a regular file"))
        }
        Kind::File(OpenMode::Write) => file.set_len(0).map(|()| file),
        _ => Ok(file),
    }
}

/// Opens every directory of `relative` under `root`, holding the handles,
/// and runs `last` on the final component's full path.
fn walk<T>(
    root: &Path,
    relative: &Path,
    last: impl FnOnce(&Path) -> io::Result<T>,
) -> io::Result<T> {
    let names: Vec<_> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    let Some((file, directories)) = names.split_last() else {
        return Err(denied("filesystem path has no file component"));
    };
    let mut path = root.to_path_buf();
    let mut held = Vec::with_capacity(directories.len());
    for name in directories {
        path.push(name);
        held.push(open_component(&path, Kind::Directory)?);
    }
    path.push(file);
    let result = last(&path);
    drop(held);
    result
}

pub(super) fn open(root: &Path, relative: &Path, mode: OpenMode) -> io::Result<File> {
    walk(root, relative, |path| {
        open_component(path, Kind::File(mode))
    })
}

/// Creates every missing directory of `relative` under `root`, holding the
/// handle of each one before it creates the next: a component that is a
/// reparse point, or becomes one, is refused by `open_component`.
pub(super) fn create_dir_all(root: &Path, relative: &Path) -> io::Result<()> {
    let mut path = root.to_path_buf();
    let mut held = Vec::new();
    for component in relative.components() {
        if let Component::Normal(name) = component {
            path.push(name);
            held.push(open_or_create_directory(&path)?);
        }
    }
    drop(held);
    Ok(())
}

fn open_or_create_directory(path: &Path) -> io::Result<File> {
    match open_component(path, Kind::Directory) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match std::fs::create_dir(path) {
                Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error),
                _ => {}
            }
            open_component(path, Kind::Directory)
        }
        other => other,
    }
}

/// Removes a regular file. A swap of the checked file for a link between
/// the check and the delete removes that link, never its target, and
/// the held parents keep it inside the root.
pub(super) fn remove_file(root: &Path, relative: &Path) -> io::Result<()> {
    walk(root, relative, |path| {
        drop(open_component(path, Kind::Doomed)?);
        std::fs::remove_file(path)
    })
}

/// The Windows error for "the directory name is invalid", which `std` reads
/// as `NotADirectory` (POSIX `ENOTDIR`).
const ERROR_DIRECTORY: i32 = 267;

/// Where an entry operation acts (bucket 0.6.12, FS-CORE-1): the full path
/// of the entry, with the handles of its parents held so that no parent can
/// be renamed or replaced, or a directory with no name to act on (a policy
/// root, a drive root). Windows sets no DACL on what it creates: the entry
/// inherits the parent's (G-5).
pub(crate) struct Anchor {
    path: PathBuf,
    is_root: bool,
    // Held, never read: an open handle keeps its directory from being
    // renamed or replaced while the operation runs.
    _held_parents: Vec<File>,
}

impl RootedDir {
    /// The anchor of `path`, reached from the pinned root without following
    /// a reparse point on the way. The root itself has no name.
    pub(crate) fn anchor(&self, path: &Path) -> io::Result<Anchor> {
        if self.is_root(path)? {
            return Ok(Anchor {
                path: self.canonical_path.clone(),
                is_root: true,
                _held_parents: Vec::new(),
            });
        }
        let relative = self.relative(path)?;
        let names: Vec<_> = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect();
        let mut entry = self.canonical_path.clone();
        let mut held = Vec::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            entry.push(name);
            if index + 1 < names.len() {
                held.push(open_component(&entry, Kind::Directory)?);
            }
        }
        Ok(Anchor {
            is_root: names.is_empty(),
            path: entry,
            _held_parents: held,
        })
    }
}

/// The anchor of `path` without a policy root: only the final component is
/// never followed.
pub(crate) fn anchor_unrooted(path: &Path) -> io::Result<Anchor> {
    let absolute = std::path::absolute(path)?;
    Ok(Anchor {
        is_root: absolute.file_name().is_none(),
        path: absolute,
        _held_parents: Vec::new(),
    })
}

impl Anchor {
    fn named(&self) -> io::Result<&Path> {
        if self.is_root {
            Err(denied(bn_catalog::text("file", "entry.root_not_target")))
        } else {
            Ok(&self.path)
        }
    }

    pub(crate) fn kind(&self) -> io::Result<EntryKind> {
        if self.is_root {
            return Ok(EntryKind::Directory);
        }
        let metadata = std::fs::symlink_metadata(&self.path)?;
        Ok(
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                EntryKind::Symlink
            } else if metadata.is_dir() {
                EntryKind::Directory
            } else if metadata.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            },
        )
    }

    pub(crate) fn create_private_directory(&self) -> io::Result<()> {
        if self.is_root {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        std::fs::create_dir(&self.path)?;
        // A reparse point put in place of the new directory is refused.
        open_component(&self.path, Kind::Directory).map(drop)
    }

    pub(crate) fn create_private_file(&self) -> io::Result<File> {
        if self.is_root {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        // CREATE_NEW fails on any existing name, a reparse point included.
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.path)
    }

    pub(crate) fn open_read(&self) -> io::Result<File> {
        let kind = if self.is_root || self.kind()? == EntryKind::Directory {
            Kind::Directory
        } else {
            Kind::File(OpenMode::Read)
        };
        open_component(&self.path, kind)
    }

    pub(crate) fn remove_empty_directory(&self) -> io::Result<()> {
        let path = self.named()?;
        if self.kind()? != EntryKind::Directory {
            return Err(io::Error::from_raw_os_error(ERROR_DIRECTORY));
        }
        // A reparse point swapped in after the check is removed as a link,
        // never through it.
        std::fs::remove_dir(path)
    }

    pub(crate) fn rename_to(&self, target: &Self) -> io::Result<()> {
        let source = self.named()?;
        let destination = target.named()?;
        match target.kind() {
            Ok(EntryKind::File) => {}
            Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        std::fs::rename(source, destination)
    }

    pub(crate) fn change_directory(&self) -> io::Result<()> {
        // The handle is held across the change, so the checked directory is
        // the one the process enters.
        let directory = open_component(&self.path, Kind::Directory)?;
        let changed = std::env::set_current_dir(&self.path);
        drop(directory);
        changed
    }
}

#[cfg(test)]
#[path = "secure_fs_windows_tests.rs"]
mod tests;
