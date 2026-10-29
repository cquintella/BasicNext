// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bn_core_compress` — compression and archive processing domain core.
//!
//! Exposes Deflate, Gzip, and `ZipArchive` functionality without coupling to
//! the Basic Next frontend, AST, IR, or any interpreter/compiler driver.
//!
//! The core never touches a file on its own: every path it reads or writes
//! goes through the caller's [`FileAccess`], which applies the execution
//! policy of the backend (`HOST.FileSystem` in `bni`, `bn_rt`'s policy in
//! compiled programs), as `language/0.6/bncompress.md` requires. Opening
//! through it, not only asking it, keeps a symlink from redirecting a
//! write outside a `--write-root` (bucket 0.6.11, CMP-SEC-1).

use flate2::Compression;
use flate2::read::{DeflateDecoder, GzDecoder};
use flate2::write::{DeflateEncoder, GzEncoder};
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

pub const CODE_OK: i32 = 0;
pub const CODE_INVALID_ARGUMENT: i32 = 1;
pub const CODE_NOT_FOUND: i32 = 2;
pub const CODE_IO_ERROR: i32 = 3;
pub const CODE_CORRUPT_DATA: i32 = 4;
pub const CODE_ZIP_SLIP: i32 = 5;
pub const CODE_POLICY_DENIED: i32 = 6;
pub const CODE_LIMIT_EXCEEDED: i32 = 7;

/// The most bytes one decompression may produce: one call of `Inflate` or
/// `Gunzip`, one extracted entry, or all entries of one `ExtractAll`
/// (bncompress.md, "Limits").
pub const MAX_DECOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;

/// The file access of the calling backend under its execution policy. Both
/// backends implement it with `bn_rt::FsPolicy`, whose rooted operations
/// never follow a symlink.
pub trait FileAccess {
    /// `Ok` when `path` may be read (`write == false`) or written
    /// (`write == true`), otherwise the policy's reason, which becomes the
    /// `Cause` of the `Error`.
    ///
    /// # Errors
    ///
    /// The policy's reason when it denies `path`.
    fn check(&self, path: &Path, write: bool) -> Result<(), String>;
    /// Opens `path` for reading.
    ///
    /// # Errors
    ///
    /// The refusal of the policy or the system error.
    fn open_read(&self, path: &Path) -> io::Result<File>;
    /// Creates or truncates `path` for writing.
    ///
    /// # Errors
    ///
    /// The refusal of the policy or the system error.
    fn create(&self, path: &Path) -> io::Result<File>;
    /// Creates the directory `path` and its missing ancestors.
    ///
    /// # Errors
    ///
    /// The refusal of the policy or the system error.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Removes the regular file `path`.
    ///
    /// # Errors
    ///
    /// The refusal of the policy or the system error.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Whether `error` is a refusal of the policy (a symlink, a path outside
    /// the roots), reported as `POLICY_DENIED`, not an I/O error.
    fn is_refusal(&self, error: &io::Error) -> bool;
}

fn catalog(key: &str) -> String {
    bn_catalog::text("compress", key).to_owned()
}

/// A `BNCompress` failure: a stable `code` (the module's `INTEGER`
/// constants), a catalog `message`, and the underlying `cause`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressError {
    pub code: i32,
    pub message: String,
    pub cause: String,
}

impl CompressError {
    #[must_use]
    pub fn new(code: i32, message: impl Into<String>, cause: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: cause.into(),
        }
    }

    #[must_use]
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::new(CODE_INVALID_ARGUMENT, message.clone(), message)
    }

    /// A `Bytes` handle that is unknown or already released.
    #[must_use]
    pub fn missing_buffer() -> Self {
        let message = catalog("error.missing_buffer.message");
        Self::new(CODE_NOT_FOUND, message.clone(), message)
    }

    /// A `ZipArchive` handle that is unknown or already released.
    #[must_use]
    pub fn missing_archive() -> Self {
        let message = catalog("error.missing_archive.message");
        Self::new(CODE_NOT_FOUND, message.clone(), message)
    }

    /// An entry index that is negative or past the last entry.
    #[must_use]
    pub fn invalid_index() -> Self {
        Self::invalid_argument(catalog("error.invalid_index.message"))
    }

    /// A buffer that is not valid UTF-8 where text is required.
    #[must_use]
    pub fn invalid_utf8(cause: impl Into<String>) -> Self {
        Self::corrupt("error.invalid_utf8.message", cause)
    }

    /// A text argument that reaches the C ABI null or not valid UTF-8.
    #[must_use]
    pub fn invalid_text() -> Self {
        Self::invalid_argument(catalog("error.invalid_text.message"))
    }

    /// Text that is not an even-length hexadecimal string.
    #[must_use]
    pub fn malformed_hex() -> Self {
        Self::invalid_argument(catalog("error.malformed_hex.message"))
    }

    fn policy_denied(path: &Path, reason: String) -> Self {
        Self::new(
            CODE_POLICY_DENIED,
            bn_catalog::fill(
                "compress",
                "error.policy_denied.message",
                &[("path", &path.display())],
            ),
            reason,
        )
    }

    fn limit_exceeded(limit: u64) -> Self {
        let message = bn_catalog::fill(
            "compress",
            "error.limit_exceeded.message",
            &[("limit", &limit)],
        );
        Self::new(CODE_LIMIT_EXCEEDED, message.clone(), message)
    }

    fn zip_slip(entry: &str) -> Self {
        let message = bn_catalog::fill("compress", "error.zip_slip.message", &[("entry", &entry)]);
        Self::new(CODE_ZIP_SLIP, message.clone(), message)
    }

    fn corrupt(key: &str, cause: impl Into<String>) -> Self {
        Self::new(CODE_CORRUPT_DATA, catalog(key), cause)
    }

    fn state(key: &str) -> Self {
        Self::invalid_argument(catalog(key))
    }
}

impl std::fmt::Display for CompressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CompressError {}

impl From<io::Error> for CompressError {
    fn from(err: io::Error) -> Self {
        let code = if err.kind() == io::ErrorKind::NotFound {
            CODE_NOT_FOUND
        } else {
            CODE_IO_ERROR
        };
        let text = err.to_string();
        Self::new(code, text.clone(), text)
    }
}

impl From<zip::result::ZipError> for CompressError {
    fn from(err: zip::result::ZipError) -> Self {
        match err {
            zip::result::ZipError::FileNotFound => {
                let message = catalog("error.entry_not_found.message");
                Self::new(CODE_NOT_FOUND, message.clone(), message)
            }
            zip::result::ZipError::InvalidArchive(cause) => {
                Self::corrupt("error.invalid_archive.message", cause.to_string())
            }
            zip::result::ZipError::UnsupportedArchive(cause) => {
                Self::corrupt("error.unsupported_archive.message", cause.to_string())
            }
            zip::result::ZipError::Io(error) => Self::from(error),
            other => {
                let text = other.to_string();
                Self::new(CODE_IO_ERROR, text.clone(), text)
            }
        }
    }
}

/// Applies the policy to `path`, turning a denial into `POLICY_DENIED`.
fn authorize(access: &dyn FileAccess, path: &Path, write: bool) -> Result<(), CompressError> {
    access
        .check(path, write)
        .map_err(|reason| CompressError::policy_denied(path, reason))
}

/// Creates the missing parent directories of `path` after the policy allows
/// writing the nearest one.
fn create_parent(access: &dyn FileAccess, path: &Path) -> Result<(), CompressError> {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() && !parent.exists() => {
            authorize(access, parent, true)?;
            performed(access, parent, access.create_dir_all(parent))
        }
        _ => Ok(()),
    }
}

/// The outcome of an operation of `access` on `path`: a refusal of the
/// policy (a symlink on the way, a path outside the roots) is
/// `POLICY_DENIED`; any other failure is the I/O error.
fn performed<T>(
    access: &dyn FileAccess,
    path: &Path,
    result: io::Result<T>,
) -> Result<T, CompressError> {
    result.map_err(|error| {
        if access.is_refusal(&error) {
            CompressError::policy_denied(path, error.to_string())
        } else {
            error.into()
        }
    })
}

/// Creates or truncates `path` for writing, with its missing parents.
fn create_file(access: &dyn FileAccess, path: &Path) -> Result<File, CompressError> {
    create_parent(access, path)?;
    authorize(access, path, true)?;
    performed(access, path, access.create(path))
}

/// Reads all of `path`.
fn read_file(access: &dyn FileAccess, path: &Path) -> Result<Vec<u8>, CompressError> {
    authorize(access, path, false)?;
    let mut content = Vec::new();
    performed(access, path, access.open_read(path))?.read_to_end(&mut content)?;
    Ok(content)
}

fn encode(mut encoder: impl Write, data: &[u8]) -> Result<(), CompressError> {
    encoder
        .write_all(data)
        .map_err(|e| CompressError::corrupt("error.compress_failed.message", e.to_string()))
}

/// Reads `reader` to its end, refusing to hold more than `limit` bytes: one
/// byte past the limit is read only to detect it, never kept.
fn read_bounded(
    reader: impl Read,
    limit: u64,
) -> Result<Result<Vec<u8>, CompressError>, io::Error> {
    let mut out = Vec::new();
    let read = reader.take(limit.saturating_add(1)).read_to_end(&mut out)?;
    Ok(if u64::try_from(read).map_or(true, |read| read > limit) {
        Err(CompressError::limit_exceeded(limit))
    } else {
        Ok(out)
    })
}

fn decode(decoder: impl Read, limit: u64) -> Result<Vec<u8>, CompressError> {
    read_bounded(decoder, limit)
        .map_err(|e| CompressError::corrupt("error.decompress_failed.message", e.to_string()))?
}

/// Compresses `data` using standard DEFLATE.
///
/// # Errors
///
/// `CORRUPT_DATA` when the encoder fails.
pub fn deflate(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encode(&mut encoder, data)?;
    encoder
        .finish()
        .map_err(|e| CompressError::corrupt("error.compress_failed.message", e.to_string()))
}

/// Decompresses DEFLATE `data`.
///
/// # Errors
///
/// `CORRUPT_DATA` when `data` is not a valid DEFLATE stream.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    decode(DeflateDecoder::new(data), MAX_DECOMPRESSED_BYTES)
}

/// Compresses `data` using standard Gzip.
///
/// # Errors
///
/// `CORRUPT_DATA` when the encoder fails.
pub fn gzip(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encode(&mut encoder, data)?;
    encoder
        .finish()
        .map_err(|e| CompressError::corrupt("error.compress_failed.message", e.to_string()))
}

/// Decompresses Gzip `data`.
///
/// # Errors
///
/// `CORRUPT_DATA` when `data` is not a valid Gzip stream.
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    decode(GzDecoder::new(data), MAX_DECOMPRESSED_BYTES)
}

/// Decodes hexadecimal ASCII into bytes; `None` for odd length or a non-hex digit.
#[must_use]
pub fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
        })
        .collect()
}

/// Encodes bytes into lowercase hexadecimal ASCII.
#[must_use]
pub fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// The state of an archive handle.
#[derive(Debug)]
pub enum ZipMode {
    Reading(Box<zip::ZipArchive<File>>),
    Writing(Option<Box<zip::ZipWriter<File>>>),
    Closed,
}

/// An open ZIP archive, read or written through the caller's policy.
pub struct ZipArchiveHandle {
    mode: ZipMode,
    path: PathBuf,
}

impl std::fmt::Debug for ZipArchiveHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mode = match self.mode {
            ZipMode::Reading(_) => "reading",
            ZipMode::Writing(_) => "writing",
            ZipMode::Closed => "closed",
        };
        f.debug_struct("ZipArchiveHandle")
            .field("mode", &mode)
            .field("path", &self.path)
            .finish()
    }
}

impl ZipArchiveHandle {
    /// A handle that holds no file: the state of an object made by `NEW`
    /// before `Create` or `Open`, and of every handle after `Close`.
    #[must_use]
    pub const fn closed() -> Self {
        Self {
            mode: ZipMode::Closed,
            path: PathBuf::new(),
        }
    }

    /// Creates (or truncates) the archive at `path` for writing.
    ///
    /// # Errors
    ///
    /// `POLICY_DENIED` when the policy does not allow writing `path` or its
    /// missing parent directory; `IO_ERROR` / `NOT_FOUND` from the system.
    pub fn create(path: impl AsRef<Path>, access: &dyn FileAccess) -> Result<Self, CompressError> {
        let path = path.as_ref().to_path_buf();
        let file = create_file(access, &path)?;
        Ok(Self {
            mode: ZipMode::Writing(Some(Box::new(zip::ZipWriter::new(file)))),
            path,
        })
    }

    /// Opens the existing archive at `path` for reading.
    ///
    /// # Errors
    ///
    /// `POLICY_DENIED` when the policy does not allow reading `path`;
    /// `NOT_FOUND`, `IO_ERROR` or `CORRUPT_DATA` otherwise.
    pub fn open(path: impl AsRef<Path>, access: &dyn FileAccess) -> Result<Self, CompressError> {
        let path = path.as_ref().to_path_buf();
        authorize(access, &path, false)?;
        let file = performed(access, &path, access.open_read(&path))?;
        let archive = zip::ZipArchive::new(file)?;
        Ok(Self {
            mode: ZipMode::Reading(Box::new(archive)),
            path,
        })
    }

    fn writer(&mut self) -> Result<&mut zip::ZipWriter<File>, CompressError> {
        match &mut self.mode {
            ZipMode::Writing(Some(writer)) => Ok(writer.as_mut()),
            ZipMode::Writing(None) | ZipMode::Closed => {
                Err(CompressError::state("error.archive_closed.message"))
            }
            ZipMode::Reading(_) => Err(CompressError::state("error.archive_reading.message")),
        }
    }

    fn reader(&mut self) -> Result<&mut zip::ZipArchive<File>, CompressError> {
        match &mut self.mode {
            ZipMode::Reading(archive) => Ok(archive),
            ZipMode::Writing(_) => Err(CompressError::state("error.archive_writing.message")),
            ZipMode::Closed => Err(CompressError::state("error.archive_closed.message")),
        }
    }

    /// Adds `data` as `entry_name`, STORED when `stored`, otherwise DEFLATE.
    ///
    /// # Errors
    ///
    /// `INVALID_ARGUMENT` when the archive is not open for writing;
    /// `IO_ERROR` from the writer.
    pub fn add_bytes(
        &mut self,
        entry_name: &str,
        data: &[u8],
        stored: bool,
    ) -> Result<(), CompressError> {
        let method = if stored {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let writer = self.writer()?;
        writer.start_file(
            entry_name,
            SimpleFileOptions::default().compression_method(method),
        )?;
        writer.write_all(data)?;
        Ok(())
    }

    /// Adds the file at `disk_path` as `entry_name` (DEFLATE).
    ///
    /// # Errors
    ///
    /// `POLICY_DENIED` when the policy does not allow reading `disk_path`;
    /// otherwise as [`Self::add_bytes`] and the read.
    pub fn add_file(
        &mut self,
        entry_name: &str,
        disk_path: impl AsRef<Path>,
        access: &dyn FileAccess,
    ) -> Result<(), CompressError> {
        let disk_path = disk_path.as_ref();
        self.writer()?;
        let content = read_file(access, disk_path)?;
        self.add_bytes(entry_name, &content, false)
    }

    /// Adds `content` as a DEFLATE text entry.
    ///
    /// # Errors
    ///
    /// As [`Self::add_bytes`].
    pub fn add_text(&mut self, entry_name: &str, content: &str) -> Result<(), CompressError> {
        self.add_bytes(entry_name, content.as_bytes(), false)
    }

    /// Adds `content` as a STORED text entry.
    ///
    /// # Errors
    ///
    /// As [`Self::add_bytes`].
    pub fn add_stored_text(
        &mut self,
        entry_name: &str,
        content: &str,
    ) -> Result<(), CompressError> {
        self.add_bytes(entry_name, content.as_bytes(), true)
    }

    /// Adds `data` as a STORED entry.
    ///
    /// # Errors
    ///
    /// As [`Self::add_bytes`].
    pub fn add_stored_bytes(&mut self, entry_name: &str, data: &[u8]) -> Result<(), CompressError> {
        self.add_bytes(entry_name, data, true)
    }

    /// The number of entries of an archive open for reading.
    ///
    /// # Errors
    ///
    /// `INVALID_ARGUMENT` when the archive is not open for reading.
    pub fn entry_count(&mut self) -> Result<usize, CompressError> {
        Ok(self.reader()?.len())
    }

    /// The name of the entry at `index`.
    ///
    /// # Errors
    ///
    /// `INVALID_ARGUMENT` when the archive is not open for reading;
    /// `NOT_FOUND` for an index past the last entry.
    pub fn entry_name(&mut self, index: usize) -> Result<String, CompressError> {
        Ok(self.reader()?.by_index(index)?.name().to_string())
    }

    /// The uncompressed bytes of `entry_name`.
    ///
    /// The size declared in the entry's header is not trusted for
    /// pre-allocation: a forged header could claim any size.
    ///
    /// # Errors
    ///
    /// `INVALID_ARGUMENT` when the archive is not open for reading;
    /// `NOT_FOUND` for a missing entry; `CORRUPT_DATA` / `IO_ERROR` when
    /// reading fails.
    pub fn extract_bytes(&mut self, entry_name: &str) -> Result<Vec<u8>, CompressError> {
        let entry = self.reader()?.by_name(entry_name)?;
        read_bounded(entry, MAX_DECOMPRESSED_BYTES)?
    }

    /// The text of `entry_name`, decoded as UTF-8.
    ///
    /// # Errors
    ///
    /// As [`Self::extract_bytes`]; `CORRUPT_DATA` when the entry is not UTF-8.
    pub fn extract_text(&mut self, entry_name: &str) -> Result<String, CompressError> {
        String::from_utf8(self.extract_bytes(entry_name)?)
            .map_err(|e| CompressError::corrupt("error.invalid_utf8.message", e.to_string()))
    }

    /// Writes the entry `entry_name` to `disk_path`.
    ///
    /// # Errors
    ///
    /// `POLICY_DENIED` when the policy does not allow writing `disk_path` or
    /// its missing parent directory; otherwise as [`Self::extract_bytes`] and
    /// the write.
    pub fn extract_file(
        &mut self,
        entry_name: &str,
        disk_path: impl AsRef<Path>,
        access: &dyn FileAccess,
    ) -> Result<(), CompressError> {
        let path = disk_path.as_ref();
        let bytes = self.extract_bytes(entry_name)?;
        create_file(access, path)?.write_all(&bytes)?;
        Ok(())
    }

    /// Extracts every entry into `destination_dir`. An entry whose name
    /// would leave the directory (Zip Slip) is `ZIP_SLIP`, and every
    /// directory and file written goes through `access`.
    ///
    /// # Errors
    ///
    /// `ZIP_SLIP`, `POLICY_DENIED`, `INVALID_ARGUMENT` when the archive is not
    /// open for reading, and the errors of reading and writing.
    pub fn extract_all(
        &mut self,
        destination_dir: impl AsRef<Path>,
        access: &dyn FileAccess,
    ) -> Result<(), CompressError> {
        self.extract_all_within(destination_dir.as_ref(), access, MAX_DECOMPRESSED_BYTES)
    }

    /// [`Self::extract_all`] with `limit` bytes for all entries together. The
    /// Unix mode an entry records is never applied: a created file or
    /// directory has the mode the process umask leaves, so an archive cannot
    /// install a setuid program (bucket 0.6.11, CMP-SEC-2). Entries already
    /// written stay when a later one fails; the file that passes `limit` is
    /// removed.
    fn extract_all_within(
        &mut self,
        requested: &Path,
        access: &dyn FileAccess,
        limit: u64,
    ) -> Result<(), CompressError> {
        let dest = requested
            .canonicalize()
            .unwrap_or_else(|_| requested.to_path_buf());
        let mut budget = limit;
        let archive = self.reader()?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            let enclosed = file
                .enclosed_name()
                .ok_or_else(|| CompressError::zip_slip(file.name()))?;
            let outpath = dest.join(enclosed);
            if !outpath.starts_with(&dest) {
                return Err(CompressError::zip_slip(file.name()));
            }
            if file.is_dir() {
                if !outpath.exists() {
                    authorize(access, &outpath, true)?;
                    performed(access, &outpath, access.create_dir_all(&outpath))?;
                }
            } else {
                let mut outfile = create_file(access, &outpath)?;
                let written = io::copy(
                    &mut (&mut file).take(budget.saturating_add(1)),
                    &mut outfile,
                )?;
                if written > budget {
                    drop(outfile);
                    let _ = access.remove_file(&outpath);
                    return Err(CompressError::limit_exceeded(limit));
                }
                budget -= written;
            }
        }
        Ok(())
    }

    /// Finishes an archive being written (central directory) and closes the
    /// handle; closing a handle open for reading or already closed succeeds.
    ///
    /// # Errors
    ///
    /// `IO_ERROR` when the central directory cannot be written.
    pub fn close(&mut self) -> Result<(), CompressError> {
        let mode = std::mem::replace(&mut self.mode, ZipMode::Closed);
        if let ZipMode::Writing(Some(writer)) = mode {
            writer.finish()?;
        }
        Ok(())
    }
}

impl Drop for ZipArchiveHandle {
    fn drop(&mut self) {
        // A release without `Close()` still finishes the archive; its error
        // has no caller left to receive it (bucket 0.6.11, S6.7).
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Plain `std::fs` access that allows every path.
    struct Unrestricted;

    impl FileAccess for Unrestricted {
        fn check(&self, _: &Path, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn open_read(&self, path: &Path) -> io::Result<File> {
            File::open(path)
        }
        fn create(&self, path: &Path) -> io::Result<File> {
            File::create(path)
        }
        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            fs::create_dir_all(path)
        }
        fn remove_file(&self, path: &Path) -> io::Result<()> {
            fs::remove_file(path)
        }
        fn is_refusal(&self, _: &io::Error) -> bool {
            false
        }
    }

    /// A policy that allows nothing: no file is ever opened.
    struct DenyAll;

    impl FileAccess for DenyAll {
        fn check(&self, _: &Path, _: bool) -> Result<(), String> {
            Err("denied for the test".to_owned())
        }
        fn open_read(&self, _: &Path) -> io::Result<File> {
            unreachable!("a denied path is never opened")
        }
        fn create(&self, _: &Path) -> io::Result<File> {
            unreachable!("a denied path is never created")
        }
        fn create_dir_all(&self, _: &Path) -> io::Result<()> {
            unreachable!("a denied directory is never created")
        }
        fn remove_file(&self, _: &Path) -> io::Result<()> {
            unreachable!("a denied path is never removed")
        }
        fn is_refusal(&self, _: &io::Error) -> bool {
            false
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("bn_test_zip_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn deflate_inflate_roundtrip() {
        let input = b"Hello world! Testing Basic Next BNCompress Deflate and Inflate.";
        let compressed = deflate(input).expect("deflate failed");
        assert_ne!(compressed, input);
        assert_eq!(inflate(&compressed).expect("inflate failed"), input);
    }

    #[test]
    fn gzip_gunzip_roundtrip() {
        let input = b"Gzip compression in Basic Next standard library BNCompress.";
        let compressed = gzip(input).expect("gzip failed");
        assert_eq!(compressed[0], 0x1f);
        assert_eq!(compressed[1], 0x8b);
        assert_eq!(gunzip(&compressed).expect("gunzip failed"), input);
    }

    #[test]
    fn corrupt_streams_are_errors() {
        assert_eq!(
            inflate(b"not deflate").map_err(|e| e.code),
            Err(CODE_CORRUPT_DATA)
        );
        assert_eq!(
            gunzip(b"not gzip").map_err(|e| e.code),
            Err(CODE_CORRUPT_DATA)
        );
    }

    #[test]
    fn hex_rejects_odd_length_and_non_hex() {
        assert_eq!(decode_hex("0aff"), Some(vec![0x0a, 0xff]));
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(decode_hex("zz"), None);
        assert_eq!(decode_hex("é0"), None);
        assert_eq!(encode_hex(&[0x0a, 0xff]), "0aff");
    }

    #[test]
    fn zip_archive_create_and_extract() {
        let dir = temp_dir("roundtrip");
        let zip_path = dir.join("test.zip");
        {
            let mut archive = ZipArchiveHandle::create(&zip_path, &Unrestricted).expect("create");
            archive
                .add_stored_text("mimetype", "application/epub+zip")
                .expect("add stored text");
            archive
                .add_text("chapter1.html", "<h1>Chapter 1</h1>")
                .expect("add text");
            archive
                .add_bytes("data.bin", &[1, 2, 3, 4, 5], false)
                .expect("add bytes");
            archive.close().expect("close zip");
        }
        let mut archive = ZipArchiveHandle::open(&zip_path, &Unrestricted).expect("open zip");
        assert_eq!(archive.entry_count().expect("count"), 3);
        assert_eq!(archive.entry_name(0).expect("name"), "mimetype");
        assert_eq!(archive.entry_name(1).expect("name"), "chapter1.html");
        assert_eq!(archive.entry_name(2).expect("name"), "data.bin");
        assert_eq!(
            archive.extract_text("mimetype").expect("text"),
            "application/epub+zip"
        );
        assert_eq!(
            archive.extract_bytes("data.bin").expect("bytes"),
            vec![1, 2, 3, 4, 5]
        );
        let extract_dir = dir.join("extracted");
        archive
            .extract_all(&extract_dir, &Unrestricted)
            .expect("extract all");
        assert_eq!(
            fs::read_to_string(extract_dir.join("chapter1.html")).expect("read extracted"),
            "<h1>Chapter 1</h1>"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn denied_paths_touch_nothing() {
        let dir = temp_dir("denied");
        let zip_path = dir.join("denied.zip");
        let denied = ZipArchiveHandle::create(&zip_path, &DenyAll).map(|_| ());
        assert_eq!(denied.map_err(|e| e.code), Err(CODE_POLICY_DENIED));
        assert!(
            !zip_path.exists(),
            "a denied Create must not create the file"
        );

        let mut writer = ZipArchiveHandle::create(&zip_path, &Unrestricted).expect("create");
        writer.add_text("a.txt", "a").expect("add");
        let source = dir.join("source.txt");
        fs::write(&source, "secret").expect("write source");
        assert_eq!(
            writer
                .add_file("s.txt", &source, &DenyAll)
                .map_err(|e| e.code),
            Err(CODE_POLICY_DENIED)
        );
        writer.close().expect("close");

        assert_eq!(
            ZipArchiveHandle::open(&zip_path, &DenyAll)
                .map(|_| ())
                .map_err(|e| e.code),
            Err(CODE_POLICY_DENIED)
        );
        let mut reader = ZipArchiveHandle::open(&zip_path, &Unrestricted).expect("open");
        let target = dir.join("out").join("a.txt");
        assert_eq!(
            reader
                .extract_file("a.txt", &target, &DenyAll)
                .map_err(|e| e.code),
            Err(CODE_POLICY_DENIED)
        );
        assert!(!target.exists());
        let all = dir.join("all");
        assert_eq!(
            reader.extract_all(&all, &DenyAll).map_err(|e| e.code),
            Err(CODE_POLICY_DENIED)
        );
        assert!(!all.join("a.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn decompression_stops_at_the_limit() {
        let expanded = vec![0_u8; 1000];
        let compressed = gzip(&expanded).expect("gzip");
        assert_eq!(
            decode(GzDecoder::new(compressed.as_slice()), 1000).map(|bytes| bytes.len()),
            Ok(1000)
        );
        assert_eq!(
            decode(GzDecoder::new(compressed.as_slice()), 999).map_err(|e| e.code),
            Err(CODE_LIMIT_EXCEEDED)
        );
    }

    /// Writes `entries` (name, mode, content; a name ending in `/` is a
    /// directory) into a new archive at `path`.
    fn archive_with_modes(path: &Path, entries: &[(&str, u32, &[u8])]) {
        let mut writer = zip::ZipWriter::new(File::create(path).expect("create archive"));
        for (name, mode, content) in entries {
            let options = SimpleFileOptions::default().unix_permissions(*mode);
            if name.ends_with('/') {
                writer.add_directory(*name, options).expect("add directory");
            } else {
                writer.start_file(*name, options).expect("start file");
                writer.write_all(content).expect("write entry");
            }
        }
        writer.finish().expect("finish archive");
    }

    #[cfg(unix)]
    #[test]
    fn extraction_never_applies_the_entry_mode() {
        use std::os::unix::fs::PermissionsExt as _;
        const SETUID_ROOT_MODE: u32 = 0o6777;
        const SPECIAL_BITS: u32 = 0o7000;
        const EXECUTE_BITS: u32 = 0o111;

        let dir = temp_dir("entry_mode");
        let zip_path = dir.join("modes.zip");
        archive_with_modes(
            &zip_path,
            &[
                ("bin/", SETUID_ROOT_MODE, b""),
                ("bin/tool", SETUID_ROOT_MODE, b"#!/bin/sh\n"),
            ],
        );
        let out = dir.join("out");
        let single = dir.join("single");
        let mut reader = ZipArchiveHandle::open(&zip_path, &Unrestricted).expect("open");
        reader
            .extract_all(&out, &Unrestricted)
            .expect("extract all");
        reader
            .extract_file("bin/tool", &single, &Unrestricted)
            .expect("extract file");

        for path in [out.join("bin"), out.join("bin/tool"), single.clone()] {
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & SPECIAL_BITS, 0, "{}: {mode:o}", path.display());
        }
        for path in [out.join("bin/tool"), single] {
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & EXECUTE_BITS, 0, "{}: {mode:o}", path.display());
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extraction_past_the_limit_removes_only_the_entry_that_passed_it() {
        const LIMIT_BYTES: u64 = 8;
        const MODE: u32 = 0o644;

        let dir = temp_dir("extract_limit");
        let zip_path = dir.join("limit.zip");
        archive_with_modes(
            &zip_path,
            &[
                ("first.txt", MODE, b"12345"),
                ("second.txt", MODE, b"67890"),
            ],
        );
        let out = dir.join("out");
        let mut reader = ZipArchiveHandle::open(&zip_path, &Unrestricted).expect("open");
        assert_eq!(
            reader
                .extract_all_within(&out, &Unrestricted, LIMIT_BYTES)
                .map_err(|e| e.code),
            Err(CODE_LIMIT_EXCEEDED)
        );
        assert_eq!(
            fs::read_to_string(out.join("first.txt")).expect("first stays"),
            "12345"
        );
        assert!(!out.join("second.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn closed_handle_rejects_every_operation() {
        let mut handle = ZipArchiveHandle::closed();
        assert_eq!(
            handle.add_text("a", "b").map_err(|e| e.code),
            Err(CODE_INVALID_ARGUMENT)
        );
        assert_eq!(
            handle.entry_count().map_err(|e| e.code),
            Err(CODE_INVALID_ARGUMENT)
        );
        assert!(handle.close().is_ok());
    }
}
