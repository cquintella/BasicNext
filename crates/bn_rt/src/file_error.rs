// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A failed `HOST.FileSystem` operation and the BN `Error` it becomes
//! (language/0.6/error.md; codes in host.md, "File system errors"): the
//! portable code, the canonical operation, a message naming the file, and
//! the cause. Both backends build their `Error` from this one type.

use std::fmt;
use std::io;
use std::string::FromUtf8Error;

use bn_types::error_codes::fs;

use super::secure_fs::OpenMode;

/// The `HOST.FileSystem` operations, by canonical name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsOperation {
    Open,
    Exists,
    DeleteFile,
    Close,
    ReadLine,
    ReadAll,
    Write,
    WriteLine,
    ReadBytes,
    WriteBytes,
}

impl FsOperation {
    /// The `Error.Operation` text.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "HOST.FileSystem.Open",
            Self::Exists => "HOST.FileSystem.Exists",
            Self::DeleteFile => "HOST.FileSystem.DeleteFile",
            Self::Close => "HOST.FileSystem.File.Close",
            Self::ReadLine => "HOST.FileSystem.File.ReadLine",
            Self::ReadAll => "HOST.FileSystem.File.ReadAll",
            Self::Write => "HOST.FileSystem.File.Write",
            Self::WriteLine => "HOST.FileSystem.File.WriteLine",
            Self::ReadBytes => "HOST.FileSystem.File.ReadBytes",
            Self::WriteBytes => "HOST.FileSystem.File.WriteBytes",
        }
    }

    /// What a file method could not do, before its object.
    const fn action(self) -> &'static str {
        match self {
            Self::Close => "flush",
            Self::ReadLine => "read a line from",
            Self::ReadAll => "read",
            Self::Write | Self::WriteLine => "write to",
            Self::ReadBytes => "read bytes from",
            Self::WriteBytes => "write bytes to",
            Self::Open => "open",
            Self::Exists => "check",
            Self::DeleteFile => "delete",
        }
    }
}

/// Why an operation failed.
#[derive(Debug)]
pub enum Failure {
    /// A computed mode that is not `FS.READ`, `FS.WRITE`, or `FS.APPEND`.
    UnknownMode(i128),
    /// The execution policy denies the path; the text is its reason.
    PolicyDenied(&'static str),
    Directory,
    Closed,
    /// A text method on a file in binary use, or a byte method in text use.
    WrongFamily {
        binary_in_use: bool,
    },
    InvalidUtf8(FromUtf8Error),
    /// A write on a file opened for `READ`, or a read on one opened for
    /// `WRITE` or `APPEND`. Detected before the call, so every host reports
    /// it alike.
    WrongMode {
        writing: bool,
    },
    Io(io::Error),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// What the operation acted on, for the message.
#[derive(Debug)]
enum Subject {
    /// A path argument, with the mode of `Open`.
    Path(String, Option<OpenMode>),
    /// A file method's file: its path, or none for `NEW FS.File()`.
    File(Option<String>),
}

/// A failed `HOST.FileSystem` operation.
#[derive(Debug)]
pub struct FileError {
    operation: FsOperation,
    subject: Subject,
    failure: Failure,
}

const fn mode_name(mode: OpenMode) -> &'static str {
    match mode {
        OpenMode::Read => "READ",
        OpenMode::Write => "WRITE",
        OpenMode::Append => "APPEND",
    }
}

impl FileError {
    /// A failure of an operation on `path` (`Open` with its `mode`).
    #[must_use]
    pub fn on_path(
        operation: FsOperation,
        path: &str,
        mode: Option<OpenMode>,
        failure: Failure,
    ) -> Self {
        Self {
            operation,
            subject: Subject::Path(path.to_owned(), mode),
            failure,
        }
    }

    /// A failure of a file method; `path` is `None` for `NEW FS.File()`.
    #[must_use]
    pub fn on_file(operation: FsOperation, path: Option<&str>, failure: Failure) -> Self {
        Self {
            operation,
            subject: Subject::File(path.map(str::to_owned)),
            failure,
        }
    }

    #[must_use]
    pub const fn failure(&self) -> &Failure {
        &self.failure
    }

    /// `Error.Code`.
    #[must_use]
    pub fn code(&self) -> i32 {
        match &self.failure {
            Failure::UnknownMode(_) => fs::INVALID_ARGUMENT,
            Failure::PolicyDenied(_) => fs::POLICY_DENIED,
            Failure::Directory => fs::IS_DIRECTORY,
            Failure::Closed => fs::CLOSED,
            Failure::WrongFamily { .. } => fs::WRONG_FAMILY,
            Failure::InvalidUtf8(_) => fs::INVALID_UTF8,
            Failure::WrongMode { .. } => fs::IO_FAILED,
            Failure::Io(error) => match error.kind() {
                io::ErrorKind::NotFound => fs::NOT_FOUND,
                io::ErrorKind::PermissionDenied => fs::PERMISSION_DENIED,
                _ => fs::IO_FAILED,
            },
        }
    }

    /// `Error.Operation`.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation.name()
    }

    /// `Error.Message`: what could not be done, naming the file.
    #[must_use]
    pub fn message(&self) -> String {
        let action = self.operation.action();
        match (&self.subject, &self.failure) {
            (Subject::Path(path, _), Failure::UnknownMode(mode)) => {
                format!("cannot open \"{path}\" in mode {mode}")
            }
            (Subject::Path(path, Some(mode)), _) => {
                format!("cannot open \"{path}\" for {}", mode_name(*mode))
            }
            (Subject::Path(path, None), _) if self.operation == FsOperation::Exists => {
                format!("cannot check whether \"{path}\" exists")
            }
            (Subject::Path(path, None) | Subject::File(Some(path)), _) => {
                format!("cannot {action} \"{path}\"")
            }
            (Subject::File(None), _) => format!("cannot {action} a file"),
        }
    }

    /// `Error.Cause`: why, never a restatement of the message.
    #[must_use]
    pub fn cause(&self) -> String {
        match &self.failure {
            Failure::UnknownMode(_) => {
                "the mode must be FS.READ (0), FS.WRITE (1), or FS.APPEND (2)".into()
            }
            Failure::PolicyDenied(reason) => (*reason).into(),
            Failure::Directory => "the path is a directory, not a file".into(),
            Failure::Closed => match self.subject {
                Subject::File(None) => {
                    "the file was never opened (NEW FS.File() makes a closed file)".into()
                }
                _ => "the file is closed".into(),
            },
            Failure::WrongFamily { binary_in_use } => format!(
                "the file is in {} use; a file keeps the family of its first successful \
                 method until Close",
                if *binary_in_use { "binary" } else { "text" }
            ),
            Failure::InvalidUtf8(error) => format!("the bytes are not UTF-8: {error}"),
            Failure::WrongMode { writing: true } => {
                "the file is open for READ; open it with FS.WRITE or FS.APPEND to write".into()
            }
            Failure::WrongMode { writing: false } => {
                "the file is open for writing; open it with FS.READ to read".into()
            }
            Failure::Io(error) => error.to_string(),
        }
    }
}

impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.message(), self.cause())
    }
}
