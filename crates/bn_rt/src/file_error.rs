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
pub(crate) enum FsOperation {
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
    GetCurrentDirectory,
    SetCurrentDirectory,
    CreateDirectory,
    RemoveDirectory,
    IsDirectory,
    OpenNew,
    Copy,
    Move,
}

impl FsOperation {
    /// The `Error.Operation` text.
    #[must_use]
    pub fn name(self) -> &'static str {
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
            Self::GetCurrentDirectory => {
                bn_catalog::text("modules", "operation.fs_get_current_directory")
            }
            Self::SetCurrentDirectory => {
                bn_catalog::text("modules", "operation.fs_set_current_directory")
            }
            Self::CreateDirectory => bn_catalog::text("modules", "operation.fs_create_directory"),
            Self::RemoveDirectory => bn_catalog::text("modules", "operation.fs_remove_directory"),
            Self::IsDirectory => bn_catalog::text("modules", "operation.fs_is_directory"),
            Self::OpenNew => bn_catalog::text("modules", "operation.fs_open_new"),
            Self::Copy => bn_catalog::text("modules", "operation.fs_copy"),
            Self::Move => bn_catalog::text("modules", "operation.fs_move"),
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
            Self::GetCurrentDirectory
            | Self::SetCurrentDirectory
            | Self::CreateDirectory
            | Self::RemoveDirectory
            | Self::IsDirectory
            | Self::OpenNew
            | Self::Copy
            | Self::Move => panic!("the 0.6.12 operations take their message from the catalog"),
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
    /// The path is not a directory; `symlink` when it is a symbolic link,
    /// which is not followed (bucket 0.6.12, G-2).
    NotDirectory {
        symlink: bool,
    },
    /// `RemoveDirectory` of a directory with entries.
    NotEmpty,
    /// An existing entry where a new one is created, or a directory or a
    /// symbolic link at the destination of `Move`.
    AlreadyExists,
    /// A `Copy` source that is a symbolic link or not a regular file.
    NotRegularSource,
    /// `Move` between file systems, which does not copy.
    CrossDevice,
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
    /// The source and the destination of `Copy` or `Move`.
    Pair(String, String),
    /// The process itself (`GetCurrentDirectory`).
    Process,
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
    pub(crate) fn on_path(
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

    /// A failure of `Copy` or `Move` from `source` to `destination`.
    #[must_use]
    pub(crate) fn on_pair(
        operation: FsOperation,
        source: &str,
        destination: &str,
        failure: Failure,
    ) -> Self {
        Self {
            operation,
            subject: Subject::Pair(source.to_owned(), destination.to_owned()),
            failure,
        }
    }

    /// A failure of an operation on the process (`GetCurrentDirectory`).
    #[must_use]
    pub(crate) const fn on_process(operation: FsOperation, failure: Failure) -> Self {
        Self {
            operation,
            subject: Subject::Process,
            failure,
        }
    }

    /// A failure of a file method; `path` is `None` for `NEW FS.File()`.
    #[must_use]
    pub(crate) fn on_file(operation: FsOperation, path: Option<&str>, failure: Failure) -> Self {
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
            Failure::UnknownMode(_) | Failure::NotRegularSource => fs::INVALID_ARGUMENT,
            Failure::PolicyDenied(_) => fs::POLICY_DENIED,
            Failure::Directory => fs::IS_DIRECTORY,
            Failure::Closed => fs::CLOSED,
            Failure::WrongFamily { .. } => fs::WRONG_FAMILY,
            Failure::InvalidUtf8(_) => fs::INVALID_UTF8,
            Failure::WrongMode { .. } | Failure::CrossDevice => fs::IO_FAILED,
            Failure::NotDirectory { .. } => fs::IS_NOT_DIRECTORY,
            Failure::NotEmpty => fs::DIRECTORY_NOT_EMPTY,
            Failure::AlreadyExists => fs::ALREADY_EXISTS,
            Failure::Io(error) => match error.kind() {
                io::ErrorKind::NotFound => fs::NOT_FOUND,
                io::ErrorKind::PermissionDenied => fs::PERMISSION_DENIED,
                _ => fs::IO_FAILED,
            },
        }
    }

    /// `Error.Operation`.
    #[must_use]
    pub fn operation(&self) -> &'static str {
        self.operation.name()
    }

    /// `Error.Message`: what could not be done, naming the file.
    #[must_use]
    pub fn message(&self) -> String {
        if let Some(message) = self.entry_message() {
            return message;
        }
        let action = self.operation.action();
        match (&self.subject, &self.failure) {
            (Subject::Path(path, _), Failure::UnknownMode(mode)) => bn_catalog::fill(
                "file",
                "open.mode.message",
                &[("path", path), ("mode", mode)],
            ),
            (Subject::Path(path, Some(mode)), _) => bn_catalog::fill(
                "file",
                "open.purpose.message",
                &[("path", path), ("purpose", &mode_name(*mode))],
            ),
            (Subject::Path(path, None), _) if self.operation == FsOperation::Exists => {
                bn_catalog::fill("file", "exists.message", &[("path", path)])
            }
            (Subject::Path(path, None) | Subject::File(Some(path)), _) => {
                format!("cannot {action} \"{path}\"")
            }
            (Subject::File(None), _) => format!("cannot {action} a file"),
            (Subject::Pair(..) | Subject::Process, _) => {
                panic!("only the 0.6.12 operations name two paths or none")
            }
        }
    }

    /// The message of a 0.6.12 operation (bucket 0.6.12, FS-CORE-2), from
    /// the catalog; `None` for the older operations.
    fn entry_message(&self) -> Option<String> {
        let (path, destination) = match &self.subject {
            Subject::Path(path, _) | Subject::File(Some(path)) => (path.clone(), String::new()),
            Subject::Pair(source, destination) => (source.clone(), destination.clone()),
            Subject::File(None) | Subject::Process => (String::new(), String::new()),
        };
        Some(match self.operation {
            FsOperation::GetCurrentDirectory => {
                bn_catalog::text("file", "entry.get_current_directory.message").to_owned()
            }
            FsOperation::SetCurrentDirectory => bn_catalog::fill(
                "file",
                "entry.set_current_directory.message",
                &[("path", &path)],
            ),
            FsOperation::CreateDirectory => {
                bn_catalog::fill("file", "entry.create_directory.message", &[("path", &path)])
            }
            FsOperation::RemoveDirectory => {
                bn_catalog::fill("file", "entry.remove_directory.message", &[("path", &path)])
            }
            FsOperation::IsDirectory => {
                bn_catalog::fill("file", "entry.is_directory.message", &[("path", &path)])
            }
            FsOperation::OpenNew => {
                bn_catalog::fill("file", "entry.open_new.message", &[("path", &path)])
            }
            FsOperation::Copy => bn_catalog::fill(
                "file",
                "entry.copy.message",
                &[("source", &path), ("destination", &destination)],
            ),
            FsOperation::Move => bn_catalog::fill(
                "file",
                "entry.move.message",
                &[("source", &path), ("destination", &destination)],
            ),
            _ => return None,
        })
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
            Failure::WrongFamily { binary_in_use } => bn_catalog::fill(
                "file",
                "family.cause",
                &[("family", &if *binary_in_use { "binary" } else { "text" })],
            ),
            Failure::InvalidUtf8(error) => format!("the bytes are not UTF-8: {error}"),
            Failure::WrongMode { writing: true } => {
                "the file is open for READ; open it with FS.WRITE or FS.APPEND to write".into()
            }
            Failure::WrongMode { writing: false } => {
                "the file is open for writing; open it with FS.READ to read".into()
            }
            Failure::NotDirectory { symlink: false } => {
                bn_catalog::text("file", "entry.not_directory.cause").to_owned()
            }
            Failure::NotDirectory { symlink: true } => {
                bn_catalog::text("file", "entry.symlink.cause").to_owned()
            }
            Failure::NotEmpty => bn_catalog::text("file", "entry.not_empty.cause").to_owned(),
            Failure::AlreadyExists => {
                bn_catalog::text("file", "entry.already_exists.cause").to_owned()
            }
            Failure::NotRegularSource => {
                bn_catalog::text("file", "entry.not_regular_source.cause").to_owned()
            }
            Failure::CrossDevice => bn_catalog::text("file", "entry.cross_device.cause").to_owned(),
            Failure::Io(error) => error.to_string(),
        }
    }
}

impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.message(), self.cause())
    }
}
