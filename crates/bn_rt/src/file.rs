// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` semantics (language/0.6/host.md, "File system"): one
//! implementation for the interpreter provider and the native C ABI. Callers
//! only translate results: a [`FsError::Denied`] becomes the
//! `EXECUTION_POLICY_DENIED` diagnostic, every other failure a BN `Error`
//! carrying the message.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

use super::policy::FsPolicy;
use super::secure_fs::OpenMode;

/// A failed capability operation.
#[derive(Debug)]
pub enum FsError {
    /// The execution policy denies the path.
    Denied,
    /// Any other failure; the text is the BN `Error` message.
    Failed(String),
}

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

/// Classifies an open failure. Windows reports opening a directory as
/// access denied; when the policy allows the path, that is "a directory",
/// not a policy denial.
fn open_failure(policy: &FsPolicy, path: &Path, write: bool, error: &io::Error) -> FsError {
    if error.kind() != io::ErrorKind::PermissionDenied {
        return FsError::Failed(error.to_string());
    }
    if policy.allows_path(path, write) && path.is_dir() {
        return FsError::Failed(DIRECTORY.into());
    }
    FsError::Denied
}

const DIRECTORY: &str = "path is a directory";

/// `FS.Open(path, mode)`.
///
/// # Errors
///
/// [`FsError::Denied`] outside the policy; otherwise the open failure.
pub fn open(policy: &FsPolicy, path: &Path, mode: OpenMode) -> Result<OpenFile, FsError> {
    let write = mode != OpenMode::Read;
    let file = policy
        .open(path, mode)
        .map_err(|error| open_failure(policy, path, write, &error))?;
    if file.metadata().is_ok_and(|meta| meta.is_dir()) {
        return Err(FsError::Failed(DIRECTORY.into()));
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
/// [`FsError::Denied`] outside the policy; other I/O failures.
pub fn exists(policy: &FsPolicy, path: &Path) -> Result<bool, FsError> {
    match policy.open(path, OpenMode::Read) {
        Ok(file) => Ok(file.metadata().is_ok_and(|meta| meta.is_file())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => match open_failure(policy, path, false, &error) {
            FsError::Failed(message) if message == DIRECTORY => Ok(false),
            failure => Err(failure),
        },
    }
}

/// `FS.DeleteFile(path)`; a missing path is an error.
///
/// # Errors
///
/// [`FsError::Denied`] outside the policy; other I/O failures.
pub fn delete_file(policy: &FsPolicy, path: &Path) -> Result<(), FsError> {
    policy.remove_file(path).map_err(|error| {
        if error.kind() == io::ErrorKind::PermissionDenied {
            FsError::Denied
        } else {
            FsError::Failed(error.to_string())
        }
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
    use super::{FsError, OpenFile, delete_file, exists, open};
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
        assert!(matches!(
            open(&policy, &directory, OpenMode::Read),
            Err(FsError::Failed(message)) if message == "path is a directory"
        ));
        delete_file(&policy, &path).unwrap();
        assert!(!exists(&policy, &path).unwrap());
        assert!(matches!(
            delete_file(&policy, &path),
            Err(FsError::Failed(_))
        ));
        assert!(matches!(
            open(&FsPolicy::denied(), &path, OpenMode::Read),
            Err(FsError::Denied)
        ));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
