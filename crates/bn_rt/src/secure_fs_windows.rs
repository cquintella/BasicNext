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

use super::{OpenMode, denied};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Component, Path};

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

/// Removes a regular file. A swap of the checked file for a link between
/// the check and the delete removes that link, never its target, and
/// the held parents keep it inside the root.
pub(super) fn remove_file(root: &Path, relative: &Path) -> io::Result<()> {
    walk(root, relative, |path| {
        drop(open_component(path, Kind::Doomed)?);
        std::fs::remove_file(path)
    })
}

#[cfg(test)]
mod tests {
    use super::super::{OpenMode, RootedDir};
    use std::io::{Read as _, Write as _};
    use std::path::{Path, PathBuf};

    fn fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "bn-secure-fs-win-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A directory junction, which needs no privilege (unlike a symlink).
    fn junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .expect("run mklink");
        assert!(status.status.success(), "mklink /J failed: {status:?}");
    }

    #[test]
    fn files_inside_the_root_write_read_and_remove() {
        let base = fixture("cycle");
        std::fs::create_dir(base.join("sub")).unwrap();
        let root = RootedDir::new(&base).unwrap();
        let path = base.join("sub").join("f.txt");
        root.open(&path, OpenMode::Write)
            .unwrap()
            .write_all(b"long text")
            .unwrap();
        // Write truncates, Append appends.
        root.open(&path, OpenMode::Write)
            .unwrap()
            .write_all(b"ab")
            .unwrap();
        root.open(&path, OpenMode::Append)
            .unwrap()
            .write_all(b"c")
            .unwrap();
        let mut text = String::new();
        root.open(&path, OpenMode::Read)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "abc");
        root.remove_file(&path).unwrap();
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn junctions_and_parent_traversal_cannot_leave_the_root() {
        let base = fixture("escape");
        let outside = fixture("outside");
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        junction(&base.join("jn"), &outside);
        let root = RootedDir::new(&base).unwrap();
        let through = base.join("jn").join("secret.txt");
        for mode in [OpenMode::Read, OpenMode::Write, OpenMode::Append] {
            let error = root.open(&through, mode).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        }
        assert!(root.remove_file(&through).is_err());
        // The junction itself is not a file to open or remove.
        assert!(root.open(&base.join("jn"), OpenMode::Read).is_err());
        assert!(root.remove_file(&base.join("jn")).is_err());
        assert!(
            root.open(&base.join("..").join("x"), OpenMode::Read)
                .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("secret.txt")).unwrap(),
            "secret"
        );
        let _ = std::fs::remove_dir(base.join("jn"));
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
