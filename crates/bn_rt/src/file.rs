// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` semantics (language/0.6/host.md, "File system"): one
//! implementation for the interpreter provider and the native C ABI. Every
//! failure is a [`FileError`] that both callers turn into the same BN
//! `Error`; that includes a path the execution policy denies (0.6.md,
//! "`HOST.FileSystem` execution policy": "A denied operation returns `Error`").

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

pub use super::file_error::{Failure, FileError, FsOperation};
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
    path: Option<String>,
    writable: bool,
    family: Option<Family>,
}

/// Classifies a failed open or remove. A path outside the policy is a
/// denial with the policy's reason; an allowed directory is
/// [`Failure::Directory`] (Windows reports opening one as access denied);
/// anything else is the OS error.
fn failure(policy: &FsPolicy, path: &Path, write: bool, error: io::Error) -> Failure {
    if error.kind() != io::ErrorKind::PermissionDenied {
        Failure::Io(error)
    } else if !policy.allows_path(path, write) {
        Failure::PolicyDenied(policy.denial_reason(write))
    } else if path.is_dir() {
        Failure::Directory
    } else {
        Failure::Io(error)
    }
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The open mode for `FS.READ` (0), `FS.WRITE` (1), or `FS.APPEND` (2).
///
/// # Errors
///
/// `FS.INVALID_ARGUMENT` for any other value.
pub fn open_mode(path: &Path, mode: i128) -> Result<OpenMode, FileError> {
    match mode {
        0 => Ok(OpenMode::Read),
        1 => Ok(OpenMode::Write),
        2 => Ok(OpenMode::Append),
        _ => Err(FileError::on_path(
            FsOperation::Open,
            &text(path),
            None,
            Failure::UnknownMode(mode),
        )),
    }
}

/// `FS.Open(path, mode)`.
///
/// # Errors
///
/// Policy denial, a directory, or the open failure.
pub fn open(policy: &FsPolicy, path: &Path, mode: OpenMode) -> Result<OpenFile, FileError> {
    let write = mode != OpenMode::Read;
    let fail = |failure| FileError::on_path(FsOperation::Open, &text(path), Some(mode), failure);
    let file = policy
        .open(path, mode)
        .map_err(|error| fail(failure(policy, path, write, error)))?;
    if file.metadata().is_ok_and(|meta| meta.is_dir()) {
        return Err(fail(Failure::Directory));
    }
    Ok(OpenFile {
        file: Some(file),
        path: Some(text(path)),
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
        Err(error) => match failure(policy, path, false, error) {
            Failure::Directory => Ok(false),
            failure => Err(FileError::on_path(
                FsOperation::Exists,
                &text(path),
                None,
                failure,
            )),
        },
    }
}

/// `FS.DeleteFile(path)`; a missing path is an error.
///
/// # Errors
///
/// Policy denial or the remove failure.
pub fn delete_file(policy: &FsPolicy, path: &Path) -> Result<(), FileError> {
    policy.remove_file(path).map_err(|error| {
        FileError::on_path(
            FsOperation::DeleteFile,
            &text(path),
            None,
            failure(policy, path, true, error),
        )
    })
}

impl OpenFile {
    fn error(&self, operation: FsOperation, failure: Failure) -> FileError {
        FileError::on_file(operation, self.path.as_deref(), failure)
    }

    /// The file for `family` use, to write when `writing`.
    fn file_for(&mut self, family: Family, writing: bool) -> Result<&mut File, Failure> {
        let Some(file) = self.file.as_mut() else {
            return Err(Failure::Closed);
        };
        match self.family {
            Some(current) if current != family => Err(Failure::WrongFamily {
                binary_in_use: current == Family::Binary,
            }),
            _ if writing != self.writable => Err(Failure::WrongMode { writing }),
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
        if !self.writable {
            return Ok(());
        }
        file.sync_all()
            .map_err(|error| self.error(FsOperation::Close, Failure::Io(error)))
    }

    /// `ReadLine()`: one line without `\n` or `\r\n`; `None` is `EOF`.
    ///
    /// # Errors
    ///
    /// Closed, binary use, I/O failure, or invalid UTF-8.
    pub fn read_line(&mut self) -> Result<Option<String>, FileError> {
        self.read_line_bytes()
            .map_err(|failure| self.error(FsOperation::ReadLine, failure))
    }

    fn read_line_bytes(&mut self) -> Result<Option<String>, Failure> {
        let file = self.file_for(Family::Text, false)?;
        let mut bytes = Vec::new();
        let mut read_any = false;
        let mut one = [0_u8; 1];
        // ponytail: one read per byte keeps the position exact for the next
        // call without a buffer to share; buffer when large files matter.
        loop {
            match file.read(&mut one)? {
                0 => break,
                _ => {
                    read_any = true;
                    if one[0] == b'\n' {
                        break;
                    }
                    bytes.push(one[0]);
                }
            }
        }
        if !read_any {
            self.family = Some(Family::Text);
            return Ok(None);
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        let line = String::from_utf8(bytes).map_err(Failure::InvalidUtf8)?;
        self.family = Some(Family::Text);
        Ok(Some(line))
    }

    /// `ReadAll()`: the remaining text.
    ///
    /// # Errors
    ///
    /// Closed, binary use, I/O failure, or invalid UTF-8.
    pub fn read_all(&mut self) -> Result<String, FileError> {
        self.read_all_text()
            .map_err(|failure| self.error(FsOperation::ReadAll, failure))
    }

    fn read_all_text(&mut self) -> Result<String, Failure> {
        let file = self.file_for(Family::Text, false)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let text = String::from_utf8(bytes).map_err(Failure::InvalidUtf8)?;
        self.family = Some(Family::Text);
        Ok(text)
    }

    /// `Write(text)`, or `WriteLine(text)` when `line` adds one `\n`.
    ///
    /// # Errors
    ///
    /// Closed, binary use, or I/O failure.
    pub fn write(&mut self, text: &str, line: bool) -> Result<(), FileError> {
        let operation = if line {
            FsOperation::WriteLine
        } else {
            FsOperation::Write
        };
        self.write_text(text, line)
            .map_err(|failure| self.error(operation, failure))
    }

    fn write_text(&mut self, text: &str, line: bool) -> Result<(), Failure> {
        let file = self.file_for(Family::Text, true)?;
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
        self.read_into(buffer)
            .map_err(|failure| self.error(FsOperation::ReadBytes, failure))
    }

    fn read_into(&mut self, buffer: &mut [u8]) -> Result<Option<usize>, Failure> {
        let file = self.file_for(Family::Binary, false)?;
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
        self.write_from(bytes)
            .map_err(|failure| self.error(FsOperation::WriteBytes, failure))
    }

    fn write_from(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        let file = self.file_for(Family::Binary, true)?;
        file.write_all(bytes)?;
        self.family = Some(Family::Binary);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FileError, OpenFile, delete_file, exists, open, open_mode};
    use crate::policy::FsPolicy;
    use crate::secure_fs::OpenMode;
    use bn_types::error_codes::fs;

    /// Code, operation, message, and cause of an `Error`.
    fn parts(error: &FileError) -> (i32, &'static str, String, String) {
        (
            error.code(),
            error.operation(),
            error.message(),
            error.cause(),
        )
    }

    #[test]
    fn families_eof_and_close_follow_host_md() {
        let policy = FsPolicy::unrestricted();
        let directory = std::env::temp_dir().join(format!("bn-file-core-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("text.txt");
        let name = path.to_string_lossy().into_owned();

        let mut file = open(&policy, &path, OpenMode::Write).unwrap();
        file.write("one", true).unwrap();
        file.write("tw\r", false).unwrap();
        file.write("", true).unwrap();
        let (code, operation, message, cause) = parts(&file.write_bytes(b"x").unwrap_err());
        assert_eq!(code, fs::WRONG_FAMILY);
        assert_eq!(operation, "HOST.FileSystem.File.WriteBytes");
        assert_eq!(message, format!("cannot write bytes to \"{name}\""));
        assert!(cause.starts_with("the file is in text use"), "{cause}");
        file.close().unwrap();
        file.close().unwrap(); // idempotent

        let mut file = open(&policy, &path, OpenMode::Read).unwrap();
        assert_eq!(file.read_line().unwrap().as_deref(), Some("one"));
        assert_eq!(file.read_line().unwrap().as_deref(), Some("tw"));
        assert_eq!(file.read_line().unwrap(), None);
        let mut buffer = [0_u8; 4];
        assert_eq!(
            file.read_bytes(&mut buffer).unwrap_err().code(),
            fs::WRONG_FAMILY
        );
        // Read-only handles close successfully on every platform.
        file.close().unwrap();
        let (code, operation, message, cause) = parts(&file.read_all().unwrap_err());
        assert_eq!(
            (code, operation),
            (fs::CLOSED, "HOST.FileSystem.File.ReadAll")
        );
        assert_eq!(message, format!("cannot read \"{name}\""));
        assert_eq!(cause, "the file is closed");

        let mut file = open(&policy, &path, OpenMode::Read).unwrap();
        assert_eq!(file.read_bytes(&mut buffer).unwrap(), Some(4));
        assert!(
            file.read_line()
                .unwrap_err()
                .cause()
                .starts_with("the file is in binary use")
        );
        file.close().unwrap();

        let (code, _, message, cause) = parts(&OpenFile::default().read_all().unwrap_err());
        assert_eq!(code, fs::CLOSED);
        assert_eq!(message, "cannot read a file");
        assert!(cause.contains("never opened"), "{cause}");

        assert!(exists(&policy, &path).unwrap());
        assert!(!exists(&policy, &directory).unwrap());
        let error = open(&policy, &directory, OpenMode::Read).unwrap_err();
        assert_eq!(error.code(), fs::IS_DIRECTORY);
        assert_eq!(error.cause(), "the path is a directory, not a file");
        delete_file(&policy, &path).unwrap();
        assert!(!exists(&policy, &path).unwrap());
        let error = delete_file(&policy, &path).unwrap_err();
        assert_eq!(
            (error.code(), error.operation()),
            (fs::NOT_FOUND, "HOST.FileSystem.DeleteFile")
        );
        assert_eq!(error.message(), format!("cannot delete \"{name}\""));

        let (code, operation, message, cause) =
            parts(&open(&policy, &path, OpenMode::Read).unwrap_err());
        assert_eq!((code, operation), (fs::NOT_FOUND, "HOST.FileSystem.Open"));
        assert_eq!(message, format!("cannot open \"{name}\" for READ"));
        assert!(cause.contains("os error"), "{cause}");

        let error = open(&FsPolicy::denied(), &path, OpenMode::Read).unwrap_err();
        assert_eq!(error.code(), fs::POLICY_DENIED);
        assert_eq!(error.cause(), "the execution policy denies file access");
        let error = open_mode(&path, 7).unwrap_err();
        assert_eq!(error.code(), fs::INVALID_ARGUMENT);
        assert_eq!(error.message(), format!("cannot open \"{name}\" in mode 7"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_write_on_a_read_file_fails_the_same_on_every_host() {
        let path = std::env::temp_dir().join(format!("bn-file-mode-{}", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        let mut file = open(&FsPolicy::unrestricted(), &path, OpenMode::Read).unwrap();
        let error = file.write_bytes(b"y").unwrap_err();
        assert_eq!(error.code(), fs::IO_FAILED);
        assert_eq!(
            error.cause(),
            "the file is open for READ; open it with FS.WRITE or FS.APPEND to write"
        );
        // The failed write chose no family: a byte read still works.
        let mut buffer = [0_u8; 1];
        assert_eq!(file.read_bytes(&mut buffer).unwrap(), Some(1));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_all_of_invalid_utf8_is_invalid_utf8() {
        let path = std::env::temp_dir().join(format!("bn-file-utf8-{}", std::process::id()));
        std::fs::write(&path, [b'a', 0xff]).unwrap();
        let mut file = open(&FsPolicy::unrestricted(), &path, OpenMode::Read).unwrap();
        let error = file.read_all().unwrap_err();
        assert_eq!(error.code(), fs::INVALID_UTF8);
        assert!(
            error.cause().starts_with("the bytes are not UTF-8"),
            "{}",
            error.cause()
        );
        std::fs::remove_file(path).unwrap();
    }
}
