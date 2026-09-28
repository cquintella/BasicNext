// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` semantics (language/0.6/host.md, "File system"): one
//! implementation for the interpreter provider and the native C ABI. Every
//! failure is a [`FileError`], which both callers turn into a BN `Error`;
//! that includes a path the execution policy denies (0.6.md,
//! "`HOST.FileSystem` execution policy": "A denied operation returns `Error`").

use std::fmt;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;
use std::string::FromUtf8Error;

use super::policy::FsPolicy;
use super::secure_fs::OpenMode;

/// Text or binary use (host.md: the first successful method picks it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Family {
    Text,
    Binary,
}

/// What a policy denial refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Open,
    Read,
    Delete,
}

/// A failed `HOST.FileSystem` operation; its `Display` text is the BN
/// `Error` message.
#[derive(Debug)]
pub enum FileError {
    /// A computed mode that is not `FS.READ`, `FS.WRITE`, or `FS.APPEND`.
    UnknownMode(i128),
    PolicyDenied(Access),
    Directory,
    Closed,
    /// A text method on a file in binary use, or a byte method in text use.
    WrongFamily {
        binary_in_use: bool,
    },
    InvalidUtf8(FromUtf8Error),
    Io(io::Error),
}

impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMode(_) => formatter.write_str("unknown file mode"),
            Self::PolicyDenied(Access::Open) => {
                formatter.write_str("filesystem path is outside the execution policy")
            }
            Self::PolicyDenied(Access::Read) => {
                formatter.write_str("filesystem read is outside the execution policy")
            }
            Self::PolicyDenied(Access::Delete) => {
                formatter.write_str("filesystem deletion is outside the execution policy")
            }
            Self::Directory => formatter.write_str("path is a directory"),
            Self::Closed => formatter.write_str("file is closed"),
            Self::WrongFamily {
                binary_in_use: true,
            } => formatter.write_str("file is in binary mode"),
            Self::WrongFamily {
                binary_in_use: false,
            } => formatter.write_str("file is in text mode"),
            Self::InvalidUtf8(error) => write!(formatter, "INVALID_UTF8: {error}"),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl From<io::Error> for FileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// An `FS.File`: open, or closed (`NEW FS.File()`, or after `Close`).
#[derive(Debug, Default)]
pub struct OpenFile {
    file: Option<File>,
    writable: bool,
    family: Option<Family>,
}

/// Classifies a failed open or remove. A path outside the policy is a
/// denial; an allowed directory is [`FileError::Directory`] (Windows reports
/// opening one as access denied); anything else is the OS error.
fn failure(
    policy: &FsPolicy,
    path: &Path,
    write: bool,
    error: io::Error,
    access: Access,
) -> FileError {
    if error.kind() != io::ErrorKind::PermissionDenied {
        FileError::Io(error)
    } else if !policy.allows_path(path, write) {
        FileError::PolicyDenied(access)
    } else if path.is_dir() {
        FileError::Directory
    } else {
        FileError::Io(error)
    }
}

/// The open mode for `FS.READ` (0), `FS.WRITE` (1), or `FS.APPEND` (2).
///
/// # Errors
///
/// [`FileError::UnknownMode`] for any other value.
pub const fn open_mode(mode: i128) -> Result<OpenMode, FileError> {
    match mode {
        0 => Ok(OpenMode::Read),
        1 => Ok(OpenMode::Write),
        2 => Ok(OpenMode::Append),
        _ => Err(FileError::UnknownMode(mode)),
    }
}

/// `FS.Open(path, mode)`.
///
/// # Errors
///
/// Policy denial, a directory, or the open failure.
pub fn open(policy: &FsPolicy, path: &Path, mode: OpenMode) -> Result<OpenFile, FileError> {
    let write = mode != OpenMode::Read;
    let file = policy
        .open(path, mode)
        .map_err(|error| failure(policy, path, write, error, Access::Open))?;
    if file.metadata().is_ok_and(|meta| meta.is_dir()) {
        return Err(FileError::Directory);
    }
    Ok(OpenFile {
        file: Some(file),
        writable: write,
        family: None,
    })
}

/// `FS.Exists(path)`: a regular file is `TRUE`; missing or a directory is
/// `FALSE`.
///
/// # Errors
///
/// Policy denial or another I/O failure.
pub fn exists(policy: &FsPolicy, path: &Path) -> Result<bool, FileError> {
    match policy.open(path, OpenMode::Read) {
        Ok(file) => Ok(file.metadata().is_ok_and(|meta| meta.is_file())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => match failure(policy, path, false, error, Access::Read) {
            FileError::Directory => Ok(false),
            failure => Err(failure),
        },
    }
}

/// `FS.DeleteFile(path)`; a missing path is an error.
///
/// # Errors
///
/// Policy denial or the remove failure.
pub fn delete_file(policy: &FsPolicy, path: &Path) -> Result<(), FileError> {
    policy
        .remove_file(path)
        .map_err(|error| failure(policy, path, true, error, Access::Delete))
}

impl OpenFile {
    /// The file for `family` use.
    fn file_for(&mut self, family: Family) -> Result<&mut File, FileError> {
        let Some(file) = self.file.as_mut() else {
            return Err(FileError::Closed);
        };
        match self.family {
            Some(current) if current != family => Err(FileError::WrongFamily {
                binary_in_use: current == Family::Binary,
            }),
            _ => Ok(file),
        }
    }

    /// `Close()`: flushes a file opened for writing and releases the handle,
    /// even when the flush fails. A closed file closes successfully. Nothing
    /// is flushed for a read-only handle (Windows rejects `FlushFileBuffers`
    /// on one).
    ///
    /// # Errors
    ///
    /// The flush failure.
    pub fn close(&mut self) -> Result<(), FileError> {
        self.family = None;
        let Some(file) = self.file.take() else {
            return Ok(());
        };
        if self.writable {
            Ok(file.sync_all()?)
        } else {
            Ok(())
        }
    }

    /// `ReadLine()`: one line without `\n` or `\r\n`; `None` is `EOF`.
    ///
    /// # Errors
    ///
    /// Closed, binary use, I/O failure, or invalid UTF-8.
    pub fn read_line(&mut self) -> Result<Option<String>, FileError> {
        let file = self.file_for(Family::Text)?;
        let mut bytes = Vec::new();
        let mut read_any = false;
        let mut one = [0_u8; 1];
        // ponytail: one read per byte keeps the position exact for the next
        // call without a buffer to share; buffer when large files matter.
        loop {
            match file.read(&mut one) {
                Ok(0) => break,
                Ok(_) => {
                    read_any = true;
                    if one[0] == b'\n' {
                        break;
                    }
                    bytes.push(one[0]);
                }
                Err(error) => return Err(error.into()),
            }
        }
        if !read_any {
            self.family = Some(Family::Text);
            return Ok(None);
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        let line = String::from_utf8(bytes).map_err(FileError::InvalidUtf8)?;
        self.family = Some(Family::Text);
        Ok(Some(line))
    }

    /// `ReadAll()`: the remaining text.
    ///
    /// # Errors
    ///
    /// Closed, binary use, I/O failure, or invalid UTF-8.
    pub fn read_all(&mut self) -> Result<String, FileError> {
        let file = self.file_for(Family::Text)?;
        let mut text = String::new();
        file.read_to_string(&mut text)?;
        self.family = Some(Family::Text);
        Ok(text)
    }

    /// `Write(text)`, or `WriteLine(text)` when `line` adds one `\n`.
    ///
    /// # Errors
    ///
    /// Closed, binary use, or I/O failure.
    pub fn write(&mut self, text: &str, line: bool) -> Result<(), FileError> {
        let file = self.file_for(Family::Text)?;
        file.write_all(text.as_bytes())?;
        if line {
            file.write_all(b"\n")?;
        }
        self.family = Some(Family::Text);
        Ok(())
    }

    /// `ReadBytes(buffer)`: fills `buffer`; `None` is `EOF`.
    ///
    /// # Errors
    ///
    /// Closed, text use, or I/O failure.
    pub fn read_bytes(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, FileError> {
        let file = self.file_for(Family::Binary)?;
        let count = file.read(buffer)?;
        self.family = Some(Family::Binary);
        Ok((count > 0).then_some(count))
    }

    /// `WriteBytes(buffer, count)` once the caller has checked `count`.
    ///
    /// # Errors
    ///
    /// Closed, text use, or I/O failure.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), FileError> {
        let file = self.file_for(Family::Binary)?;
        file.write_all(bytes)?;
        self.family = Some(Family::Binary);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenFile, delete_file, exists, open};
    use crate::policy::FsPolicy;
    use crate::secure_fs::OpenMode;

    #[test]
    fn families_eof_and_close_follow_host_md() {
        let policy = FsPolicy::unrestricted();
        let directory = std::env::temp_dir().join(format!("bn-file-core-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("text.txt");

        let mut file = open(&policy, &path, OpenMode::Write).unwrap();
        file.write("one", true).unwrap();
        file.write("tw\r", false).unwrap();
        file.write("", true).unwrap();
        assert_eq!(
            file.write_bytes(b"x").unwrap_err().to_string(),
            "file is in text mode"
        );
        file.close().unwrap();
        file.close().unwrap(); // idempotent

        let mut file = open(&policy, &path, OpenMode::Read).unwrap();
        assert_eq!(file.read_line().unwrap().as_deref(), Some("one"));
        assert_eq!(file.read_line().unwrap().as_deref(), Some("tw"));
        assert_eq!(file.read_line().unwrap(), None);
        let mut buffer = [0_u8; 4];
        assert_eq!(
            file.read_bytes(&mut buffer).unwrap_err().to_string(),
            "file is in text mode"
        );
        // Read-only handles close successfully on every platform.
        file.close().unwrap();
        assert_eq!(file.read_all().unwrap_err().to_string(), "file is closed");

        let mut file = open(&policy, &path, OpenMode::Read).unwrap();
        assert_eq!(file.read_bytes(&mut buffer).unwrap(), Some(4));
        assert_eq!(
            file.read_line().unwrap_err().to_string(),
            "file is in binary mode"
        );
        file.close().unwrap();

        assert_eq!(
            OpenFile::default().read_all().unwrap_err().to_string(),
            "file is closed"
        );
        assert!(exists(&policy, &path).unwrap());
        assert!(!exists(&policy, &directory).unwrap());
        assert_eq!(
            open(&policy, &directory, OpenMode::Read)
                .unwrap_err()
                .to_string(),
            "path is a directory"
        );
        delete_file(&policy, &path).unwrap();
        assert!(!exists(&policy, &path).unwrap());
        assert!(delete_file(&policy, &path).is_err());
        assert_eq!(
            open(&FsPolicy::denied(), &path, OpenMode::Read)
                .unwrap_err()
                .to_string(),
            "filesystem path is outside the execution policy"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
