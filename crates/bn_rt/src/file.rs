// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` semantics (language/0.6/host.md, "File system"): one
//! implementation for the interpreter provider and the native C ABI. Every
//! failure is the message of a BN `Error`; that includes a path the execution
//! policy denies (0.6.md, "`HOST.FileSystem` execution policy": "A denied
//! operation returns `Error`").

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use super::policy::FsPolicy;
use super::secure_fs::OpenMode;

/// Text or binary use (host.md: the first successful method picks it).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Family {
    Text,
    Binary,
}

/// An `FS.File`: open, or closed (`NEW FS.File()`, or after `Close`).
#[derive(Debug, Default)]
pub struct OpenFile {
    file: Option<File>,
    writable: bool,
    family: Option<Family>,
}

const DIRECTORY: &str = "path is a directory";

/// The `Error` message of a failed open or remove. A path outside the policy
/// gets `denied`; an allowed directory is "a directory" (Windows reports
/// opening one as access denied); anything else is the OS error.
fn failure(policy: &FsPolicy, path: &Path, write: bool, error: &io::Error, denied: &str) -> String {
    if error.kind() != io::ErrorKind::PermissionDenied {
        error.to_string()
    } else if !policy.allows_path(path, write) {
        denied.into()
    } else if path.is_dir() {
        DIRECTORY.into()
    } else {
        error.to_string()
    }
}

/// `FS.Open(path, mode)`.
///
/// # Errors
///
/// The `Error` message: policy denial, a directory, or the open failure.
pub fn open(policy: &FsPolicy, path: &Path, mode: OpenMode) -> Result<OpenFile, String> {
    let write = mode != OpenMode::Read;
    let file = policy.open(path, mode).map_err(|error| {
        failure(
            policy,
            path,
            write,
            &error,
            "filesystem path is outside the execution policy",
        )
    })?;
    if file.metadata().is_ok_and(|meta| meta.is_dir()) {
        return Err(DIRECTORY.into());
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
/// The `Error` message: policy denial or another I/O failure.
pub fn exists(policy: &FsPolicy, path: &Path) -> Result<bool, String> {
    match policy.open(path, OpenMode::Read) {
        Ok(file) => Ok(file.metadata().is_ok_and(|meta| meta.is_file())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => {
            let message = failure(
                policy,
                path,
                false,
                &error,
                "filesystem read is outside the execution policy",
            );
            if message == DIRECTORY {
                Ok(false)
            } else {
                Err(message)
            }
        }
    }
}

/// `FS.DeleteFile(path)`; a missing path is an error.
///
/// # Errors
///
/// The `Error` message: policy denial or the remove failure.
pub fn delete_file(policy: &FsPolicy, path: &Path) -> Result<(), String> {
    policy.remove_file(path).map_err(|error| {
        failure(
            policy,
            path,
            true,
            &error,
            "filesystem deletion is outside the execution policy",
        )
    })
}

impl OpenFile {
    /// The file for `family` use, or the `Error` message.
    fn file_for(&mut self, family: Family) -> Result<&mut File, String> {
        let Some(file) = self.file.as_mut() else {
            return Err("file is closed".into());
        };
        match self.family {
            Some(Family::Binary) if family == Family::Text => Err("file is in binary mode".into()),
            Some(Family::Text) if family == Family::Binary => Err("file is in text mode".into()),
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
    /// The flush failure message.
    pub fn close(&mut self) -> Result<(), String> {
        self.family = None;
        let Some(file) = self.file.take() else {
            return Ok(());
        };
        if self.writable {
            file.sync_all().map_err(|error| error.to_string())
        } else {
            Ok(())
        }
    }

    /// `ReadLine()`: one line without `\n` or `\r\n`; `None` is `EOF`.
    ///
    /// # Errors
    ///
    /// Closed, binary use, I/O failure, or invalid UTF-8.
    pub fn read_line(&mut self) -> Result<Option<String>, String> {
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
                Err(error) => return Err(error.to_string()),
            }
        }
        if !read_any {
            self.family = Some(Family::Text);
            return Ok(None);
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        let line = String::from_utf8(bytes).map_err(|error| format!("INVALID_UTF8: {error}"))?;
        self.family = Some(Family::Text);
        Ok(Some(line))
    }

    /// `ReadAll()`: the remaining text.
    ///
    /// # Errors
    ///
    /// Closed, binary use, I/O failure, or invalid UTF-8.
    pub fn read_all(&mut self) -> Result<String, String> {
        let file = self.file_for(Family::Text)?;
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        self.family = Some(Family::Text);
        Ok(text)
    }

    /// `Write(text)`, or `WriteLine(text)` when `line` adds one `\n`.
    ///
    /// # Errors
    ///
    /// Closed, binary use, or I/O failure.
    pub fn write(&mut self, text: &str, line: bool) -> Result<(), String> {
        let file = self.file_for(Family::Text)?;
        file.write_all(text.as_bytes())
            .and_then(|()| if line { file.write_all(b"\n") } else { Ok(()) })
            .map_err(|error| error.to_string())?;
        self.family = Some(Family::Text);
        Ok(())
    }

    /// `ReadBytes(buffer)`: fills `buffer`; `None` is `EOF`.
    ///
    /// # Errors
    ///
    /// Closed, text use, or I/O failure.
    pub fn read_bytes(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, String> {
        let file = self.file_for(Family::Binary)?;
        let count = file.read(buffer).map_err(|error| error.to_string())?;
        self.family = Some(Family::Binary);
        Ok((count > 0).then_some(count))
    }

    /// `WriteBytes(buffer, count)` once the caller has checked `count`.
    ///
    /// # Errors
    ///
    /// Closed, text use, or I/O failure.
    pub fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        let file = self.file_for(Family::Binary)?;
        file.write_all(bytes).map_err(|error| error.to_string())?;
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
        assert_eq!(file.write_bytes(b"x").unwrap_err(), "file is in text mode");
        file.close().unwrap();
        file.close().unwrap(); // idempotent

        let mut file = open(&policy, &path, OpenMode::Read).unwrap();
        assert_eq!(file.read_line().unwrap().as_deref(), Some("one"));
        assert_eq!(file.read_line().unwrap().as_deref(), Some("tw"));
        assert_eq!(file.read_line().unwrap(), None);
        let mut buffer = [0_u8; 4];
        assert_eq!(
            file.read_bytes(&mut buffer).unwrap_err(),
            "file is in text mode"
        );
        // Read-only handles close successfully on every platform.
        file.close().unwrap();
        assert_eq!(file.read_all().unwrap_err(), "file is closed");

        let mut file = open(&policy, &path, OpenMode::Read).unwrap();
        assert_eq!(file.read_bytes(&mut buffer).unwrap(), Some(4));
        assert_eq!(file.read_line().unwrap_err(), "file is in binary mode");
        file.close().unwrap();

        assert_eq!(
            OpenFile::default().read_all().unwrap_err(),
            "file is closed"
        );
        assert!(exists(&policy, &path).unwrap());
        assert!(!exists(&policy, &directory).unwrap());
        assert_eq!(
            open(&policy, &directory, OpenMode::Read).unwrap_err(),
            "path is a directory"
        );
        delete_file(&policy, &path).unwrap();
        assert!(!exists(&policy, &path).unwrap());
        assert!(delete_file(&policy, &path).is_err());
        assert_eq!(
            open(&FsPolicy::denied(), &path, OpenMode::Read).unwrap_err(),
            "filesystem path is outside the execution policy"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
