// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Entry operations of `HOST.FileSystem` on Unix (bucket 0.6.12, FS-CORE-1):
//! the kind of an entry, an owner-only create of a directory or a file, a
//! read open, the removal of an empty directory, a rename and a change of
//! directory, none of which follows a final symbolic link. Each acts through
//! the descriptor of the parent directory, reached by the rooted walk under
//! a policy root or by a plain open without one, so a link swapped in after
//! a check is never written through.

#![allow(unsafe_code)] // Narrow libc boundary for descriptor-relative entry calls.

use std::ffi::{CStr, CString};
use std::fs::{File, OpenOptions, Permissions};
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Component, Path};

use super::{EntryKind, RootedDir, c_name, denied, open_directory_at};

/// The modes of an owner-only directory and file. `fchmod` sets them again
/// after the create, so the process umask neither widens nor narrows them.
const PRIVATE_DIRECTORY_MODE: libc::mode_t = 0o700;
const PRIVATE_FILE_MODE: libc::mode_t = 0o600;
/// Group and other permission bits: owner-only means none is set.
const SHARED_PERMISSION_BITS: u32 = 0o077;

/// Where an entry operation acts: the parent directory and the final name,
/// or a directory with no name to act on (a policy root, `/`).
pub(crate) struct Anchor {
    directory: File,
    name: Option<CString>,
}

impl RootedDir {
    /// The anchor of `path`, reached from the pinned root without following
    /// a symbolic link on the way. The root itself has no name.
    pub(crate) fn anchor(&self, path: &Path) -> io::Result<Anchor> {
        if self.is_root(path)? {
            return Ok(Anchor {
                directory: self.directory.try_clone()?,
                name: None,
            });
        }
        let relative = self.relative(path)?;
        let mut directory = self.directory.try_clone()?;
        let mut names = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .peekable();
        while let Some(name) = names.next() {
            let name = c_name(name)?;
            if names.peek().is_none() {
                return Ok(Anchor {
                    directory,
                    name: Some(name),
                });
            }
            directory = open_directory_at(&directory, &name)?;
        }
        Ok(Anchor {
            directory,
            name: None,
        })
    }
}

/// The anchor of `path` without a policy root: the parent is opened as the
/// system resolves it, and only the final component is never followed.
pub(crate) fn anchor_unrooted(path: &Path) -> io::Result<Anchor> {
    let absolute = std::path::absolute(path)?;
    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => Ok(Anchor {
            directory: open_directory(parent)?,
            name: Some(c_name(name)?),
        }),
        _ => Ok(Anchor {
            directory: open_directory(&absolute)?,
            name: None,
        }),
    }
}

fn open_directory(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(path)
}

impl Anchor {
    /// The final name; the anchored directory itself is not a write target.
    fn name(&self) -> io::Result<&CStr> {
        self.name
            .as_deref()
            .ok_or_else(|| denied(bn_catalog::text("file", "entry.root_not_target")))
    }

    /// What the entry is, without following a final symbolic link.
    ///
    /// # Errors
    ///
    /// `NotFound` for a missing entry; the system error otherwise.
    pub(crate) fn kind(&self) -> io::Result<EntryKind> {
        let Some(name) = &self.name else {
            return Ok(EntryKind::Directory);
        };
        let mut status = MaybeUninit::<libc::stat>::uninit();
        // SAFETY: the parent descriptor is live, `name` is a NUL-terminated
        // single component, and `status` points to writable storage of the
        // size `fstatat` fills.
        let result = unsafe {
            libc::fstatat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                status.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fstatat` returned 0, so it initialized `status`.
        let mode = unsafe { status.assume_init() }.st_mode;
        Ok(match mode & libc::S_IFMT {
            libc::S_IFREG => EntryKind::File,
            libc::S_IFDIR => EntryKind::Directory,
            libc::S_IFLNK => EntryKind::Symlink,
            _ => EntryKind::Other,
        })
    }

    /// Creates the directory, owner-only (`0700`). A directory that keeps a
    /// group or other bit after `fchmod` is removed and the call fails.
    ///
    /// # Errors
    ///
    /// `AlreadyExists` for an existing entry (a symbolic link included),
    /// `NotFound` for a missing parent, `PermissionDenied` when the result is
    /// not owner-only, and the system error otherwise.
    pub(crate) fn create_private_directory(&self) -> io::Result<()> {
        let Some(name) = &self.name else {
            return Err(io::ErrorKind::AlreadyExists.into());
        };
        // SAFETY: the parent descriptor is live and `name` is a NUL-terminated
        // single component; `mkdirat` never follows a final symbolic link.
        let result = unsafe {
            libc::mkdirat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                PRIVATE_DIRECTORY_MODE,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error());
        }
        let created = open_directory_at(&self.directory, name)
            .and_then(|directory| make_private(&directory, PRIVATE_DIRECTORY_MODE));
        if created.is_err() {
            self.unlink(name, libc::AT_REMOVEDIR);
        }
        created
    }

    /// Creates the file, owner-only (`0600`), and returns it open for
    /// writing. An existing entry is never truncated or followed.
    ///
    /// # Errors
    ///
    /// As [`Self::create_private_directory`].
    pub(crate) fn create_private_file(&self) -> io::Result<File> {
        let Some(name) = &self.name else {
            return Err(io::ErrorKind::AlreadyExists.into());
        };
        let flags =
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: the parent descriptor is live, `name` is a NUL-terminated
        // single component, and the mode argument is supplied for O_CREAT.
        let descriptor = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                flags,
                libc::c_uint::from(PRIVATE_FILE_MODE),
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a successful `openat` returned a new owned descriptor.
        let file = File::from(unsafe { OwnedFd::from_raw_fd(descriptor) });
        if let Err(error) = make_private(&file, PRIVATE_FILE_MODE) {
            drop(file);
            self.unlink(name, 0);
            return Err(error);
        }
        Ok(file)
    }

    /// Opens the entry for reading without following a final symbolic link.
    /// The anchored directory itself opens as that directory. `O_NONBLOCK` keeps
    /// a FIFO swapped in after a check from blocking the open; it changes
    /// nothing for a regular file.
    ///
    /// # Errors
    ///
    /// The system error: `ELOOP` for a symbolic link, `NotFound` for a
    /// missing entry.
    pub(crate) fn open_read(&self) -> io::Result<File> {
        let Some(name) = &self.name else {
            return self.directory.try_clone();
        };
        // SAFETY: the parent descriptor is live and `name` is a NUL-terminated
        // single component; O_NOFOLLOW refuses a final symbolic link.
        let descriptor = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a successful `openat` returned a new owned descriptor.
        Ok(File::from(unsafe { OwnedFd::from_raw_fd(descriptor) }))
    }

    /// Removes the empty directory. A symbolic link or a file is not removed.
    ///
    /// # Errors
    ///
    /// `NotADirectory`, `DirectoryNotEmpty`, `NotFound`, a refusal for the
    /// anchored directory itself, and the system error otherwise.
    pub(crate) fn remove_empty_directory(&self) -> io::Result<()> {
        let name = self.name()?;
        // SAFETY: the parent descriptor is live and `name` is a NUL-terminated
        // single component; AT_REMOVEDIR removes only a directory and fails
        // with ENOTDIR on a symbolic link.
        let result = unsafe {
            libc::unlinkat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                libc::AT_REMOVEDIR,
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Renames the entry to `target`, replacing a regular file there. A
    /// directory or a symbolic link at `target` is not replaced. A link that
    /// takes the place of the checked file is replaced as an entry, never
    /// written through.
    ///
    /// # Errors
    ///
    /// `AlreadyExists` for a directory or a symbolic link at `target`, a
    /// refusal for an anchored directory itself, `CrossesDevices` across file
    /// systems, and the system error otherwise.
    pub(crate) fn rename_to(&self, target: &Self) -> io::Result<()> {
        let source = self.name()?;
        let destination = target.name()?;
        match target.kind() {
            Ok(EntryKind::File) => {}
            Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        // SAFETY: both parent descriptors are live and both names are
        // NUL-terminated single components; `renameat` moves the entry
        // itself and never follows a symbolic link.
        let result = unsafe {
            libc::renameat(
                self.directory.as_raw_fd(),
                source.as_ptr(),
                target.directory.as_raw_fd(),
                destination.as_ptr(),
            )
        };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Makes the entry the working directory of the process with `fchdir`,
    /// never through a symbolic link.
    ///
    /// # Errors
    ///
    /// `NotADirectory` for a file or a symbolic link, `NotFound` for a
    /// missing entry, and the system error otherwise.
    pub(crate) fn change_directory(&self) -> io::Result<()> {
        let directory = match &self.name {
            None => self.directory.try_clone()?,
            Some(name) => self.open_final_directory(name)?,
        };
        // SAFETY: `directory` owns a live directory descriptor for the call.
        let result = unsafe { libc::fchdir(directory.as_raw_fd()) };
        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Opens the final directory without following a symbolic link. A link is
    /// not a directory here (G-2): Linux reports `ELOOP` and macOS `ENOTDIR`
    /// for it, and both become `ENOTDIR`.
    fn open_final_directory(&self, name: &CStr) -> io::Result<File> {
        // SAFETY: the parent descriptor is live and `name` is a NUL-terminated
        // single component.
        let descriptor = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            let error = io::Error::last_os_error();
            return Err(if error.raw_os_error() == Some(libc::ELOOP) {
                io::Error::from_raw_os_error(libc::ENOTDIR)
            } else {
                error
            });
        }
        // SAFETY: a successful `openat` returned a new owned descriptor.
        Ok(File::from(unsafe { OwnedFd::from_raw_fd(descriptor) }))
    }

    /// Removes the entry just created after a failed owner-only check. The
    /// result is ignored: the create already failed, and that is reported.
    fn unlink(&self, name: &CStr, flags: libc::c_int) {
        // SAFETY: the parent descriptor is live and `name` is a NUL-terminated
        // single component; `unlinkat` never follows a final symbolic link.
        let _ = unsafe { libc::unlinkat(self.directory.as_raw_fd(), name.as_ptr(), flags) };
    }
}

/// Sets `mode` on the open entry and checks that no group or other bit is
/// left (G-6: the mode bits are the check; inherited ACLs are not read).
fn make_private(entry: &File, mode: libc::mode_t) -> io::Result<()> {
    entry.set_permissions(Permissions::from_mode(u32::from(mode)))?;
    let mode = entry.metadata()?.permissions().mode();
    if is_private(mode) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            bn_catalog::text("file", "entry.not_private"),
        ))
    }
}

/// Whether `mode` grants nothing to the group or to others.
#[must_use]
pub(crate) const fn is_private(mode: u32) -> bool {
    mode & SHARED_PERMISSION_BITS == 0
}

#[cfg(test)]
mod tests {
    use super::is_private;

    #[test]
    fn only_owner_bits_are_private() {
        for mode in [0o700, 0o600, 0o400, 0o40_700] {
            assert!(is_private(mode), "{mode:o}");
        }
        for mode in [0o710, 0o701, 0o640, 0o604, 0o666, 0o2770] {
            assert!(!is_private(mode), "{mode:o}");
        }
    }
}
