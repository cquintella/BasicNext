// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `bn_core_compress` — compression and archive processing domain core.
//!
//! Exposes Deflate, Gzip, and ZipArchive functionality without coupling to
//! the Basic Next frontend, AST, IR, or any interpreter/compiler driver.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use flate2::read::{DeflateDecoder, GzDecoder};
use flate2::write::{DeflateEncoder, GzEncoder};
use flate2::Compression;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

pub const CODE_OK: i32 = 0;
pub const CODE_INVALID_ARGUMENT: i32 = 1;
pub const CODE_NOT_FOUND: i32 = 2;
pub const CODE_IO_ERROR: i32 = 3;
pub const CODE_CORRUPT_DATA: i32 = 4;
pub const CODE_ZIP_SLIP: i32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressError {
    pub code: i32,
    pub message: String,
}

impl CompressError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::new(CODE_INVALID_ARGUMENT, msg)
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(CODE_NOT_FOUND, msg)
    }

    pub fn io_error(msg: impl Into<String>) -> Self {
        Self::new(CODE_IO_ERROR, msg)
    }

    pub fn corrupt_data(msg: impl Into<String>) -> Self {
        Self::new(CODE_CORRUPT_DATA, msg)
    }

    pub fn zip_slip(msg: impl Into<String>) -> Self {
        Self::new(CODE_ZIP_SLIP, msg)
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
        if err.kind() == io::ErrorKind::NotFound {
            Self::not_found(err.to_string())
        } else {
            Self::io_error(err.to_string())
        }
    }
}

impl From<zip::result::ZipError> for CompressError {
    fn from(err: zip::result::ZipError) -> Self {
        match err {
            zip::result::ZipError::FileNotFound => Self::not_found("File not found in archive"),
            zip::result::ZipError::InvalidArchive(e) => Self::corrupt_data(format!("Invalid archive: {e}")),
            zip::result::ZipError::UnsupportedArchive(e) => Self::corrupt_data(format!("Unsupported archive: {e}")),
            zip::result::ZipError::Io(e) => Self::from(e),
            other => Self::io_error(other.to_string()),
        }
    }
}

/// Compresses `data` using standard DEFLATE.
pub fn deflate(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| CompressError::corrupt_data(e.to_string()))?;
    encoder.finish().map_err(|e| CompressError::corrupt_data(e.to_string()))
}

/// Decompresses DEFLATE `data`.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut decoder = DeflateDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|e| CompressError::corrupt_data(e.to_string()))?;
    Ok(out)
}

/// Compresses `data` using standard Gzip.
pub fn gzip(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| CompressError::corrupt_data(e.to_string()))?;
    encoder.finish().map_err(|e| CompressError::corrupt_data(e.to_string()))
}

/// Decompresses Gzip `data`.
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut decoder = GzDecoder::new(data);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|e| CompressError::corrupt_data(e.to_string()))?;
    Ok(out)
}

/// Decodes hexadecimal ASCII into bytes.
pub fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

/// Encodes bytes into lowercase hexadecimal ASCII.
pub fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[derive(Debug)]
pub enum ZipMode {
    Reading(zip::ZipArchive<File>),
    Writing(Option<zip::ZipWriter<File>>),
    Closed,
}

pub struct ZipArchiveHandle {
    mode: ZipMode,
    path: PathBuf,
}

impl std::fmt::Debug for ZipArchiveHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipArchiveHandle")
            .field("path", &self.path)
            .finish()
    }
}

impl ZipArchiveHandle {
    pub fn create(path: impl AsRef<Path>) -> Result<Self, CompressError> {
        let p = path.as_ref().to_path_buf();
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(&p)?;
        let writer = zip::ZipWriter::new(file);
        Ok(Self {
            mode: ZipMode::Writing(Some(writer)),
            path: p,
        })
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, CompressError> {
        let p = path.as_ref().to_path_buf();
        let file = File::open(&p)?;
        let archive = zip::ZipArchive::new(file)?;
        Ok(Self {
            mode: ZipMode::Reading(archive),
            path: p,
        })
    }

    pub fn add_bytes(&mut self, entry_name: &str, data: &[u8], stored: bool) -> Result<(), CompressError> {
        match &mut self.mode {
            ZipMode::Writing(Some(writer)) => {
                let method = if stored {
                    CompressionMethod::Stored
                } else {
                    CompressionMethod::Deflated
                };
                let options = SimpleFileOptions::default().compression_method(method);
                writer.start_file(entry_name, options)?;
                writer.write_all(data)?;
                Ok(())
            }
            ZipMode::Writing(None) | ZipMode::Closed => Err(CompressError::invalid_argument("Archive is closed")),
            ZipMode::Reading(_) => Err(CompressError::invalid_argument("Archive is open for reading, not writing")),
        }
    }

    pub fn add_file(&mut self, entry_name: &str, disk_path: impl AsRef<Path>) -> Result<(), CompressError> {
        let content = fs::read(disk_path)?;
        self.add_bytes(entry_name, &content, false)
    }

    pub fn add_text(&mut self, entry_name: &str, content: &str) -> Result<(), CompressError> {
        self.add_bytes(entry_name, content.as_bytes(), false)
    }

    pub fn add_stored_text(&mut self, entry_name: &str, content: &str) -> Result<(), CompressError> {
        self.add_bytes(entry_name, content.as_bytes(), true)
    }

    pub fn add_stored_bytes(&mut self, entry_name: &str, data: &[u8]) -> Result<(), CompressError> {
        self.add_bytes(entry_name, data, true)
    }

    pub fn entry_count(&mut self) -> Result<usize, CompressError> {
        match &self.mode {
            ZipMode::Reading(archive) => Ok(archive.len()),
            _ => Err(CompressError::invalid_argument("Entry count is only available when opened for reading")),
        }
    }

    pub fn entry_name(&mut self, index: usize) -> Result<String, CompressError> {
        match &mut self.mode {
            ZipMode::Reading(archive) => {
                let file = archive.by_index(index)?;
                Ok(file.name().to_string())
            }
            _ => Err(CompressError::invalid_argument("Entry name is only available when opened for reading")),
        }
    }

    pub fn extract_bytes(&mut self, entry_name: &str) -> Result<Vec<u8>, CompressError> {
        match &mut self.mode {
            ZipMode::Reading(archive) => {
                let mut file = archive.by_name(entry_name)?;
                let mut data = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut data)?;
                Ok(data)
            }
            _ => Err(CompressError::invalid_argument("Extract is only available when opened for reading")),
        }
    }

    pub fn extract_text(&mut self, entry_name: &str) -> Result<String, CompressError> {
        let bytes = self.extract_bytes(entry_name)?;
        String::from_utf8(bytes).map_err(|e| CompressError::corrupt_data(format!("Invalid UTF-8: {e}")))
    }

    pub fn extract_file(&mut self, entry_name: &str, disk_path: impl AsRef<Path>) -> Result<(), CompressError> {
        let bytes = self.extract_bytes(entry_name)?;
        let path = disk_path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Extracts all files to `destination_dir`, strictly preventing Zip Slip vulnerabilities.
    pub fn extract_all(&mut self, destination_dir: impl AsRef<Path>) -> Result<(), CompressError> {
        let dest = destination_dir.as_ref().canonicalize().unwrap_or_else(|_| destination_dir.as_ref().to_path_buf());
        match &mut self.mode {
            ZipMode::Reading(archive) => {
                for i in 0..archive.len() {
                    let mut file = archive.by_index(i)?;
                    let enclosed_name = file.enclosed_name().ok_or_else(|| {
                        CompressError::zip_slip(format!("Entry '{}' escapes destination path", file.name()))
                    })?;
                    let outpath = dest.join(enclosed_name);

                    // Extra check against zip slip
                    if !outpath.starts_with(&dest) {
                        return Err(CompressError::zip_slip(format!(
                            "Entry '{}' escapes destination path",
                            file.name()
                        )));
                    }

                    if file.is_dir() {
                        fs::create_dir_all(&outpath)?;
                    } else {
                        if let Some(p) = outpath.parent() {
                            if !p.exists() {
                                fs::create_dir_all(p)?;
                            }
                        }
                        let mut outfile = File::create(&outpath)?;
                        io::copy(&mut file, &mut outfile)?;
                    }
                }
                Ok(())
            }
            _ => Err(CompressError::invalid_argument("ExtractAll is only available when opened for reading")),
        }
    }

    pub fn close(&mut self) -> Result<(), CompressError> {
        match &mut self.mode {
            ZipMode::Writing(writer_slot) => {
                if let Some(writer) = writer_slot.take() {
                    writer.finish()?;
                }
                self.mode = ZipMode::Closed;
                Ok(())
            }
            ZipMode::Reading(_) => {
                self.mode = ZipMode::Closed;
                Ok(())
            }
            ZipMode::Closed => Ok(()),
        }
    }
}

impl Drop for ZipArchiveHandle {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deflate_inflate_roundtrip() {
        let input = b"Hello world! Testing Basic Next BNCompress Deflate and Inflate.";
        let compressed = deflate(input).expect("deflate failed");
        assert_ne!(compressed, input);
        let decompressed = inflate(&compressed).expect("inflate failed");
        assert_eq!(decompressed, input);
    }

    #[test]
    fn test_gzip_gunzip_roundtrip() {
        let input = b"Gzip compression in Basic Next standard library BNCompress.";
        let compressed = gzip(input).expect("gzip failed");
        assert_eq!(compressed[0], 0x1f);
        assert_eq!(compressed[1], 0x8b);
        let decompressed = gunzip(&compressed).expect("gunzip failed");
        assert_eq!(decompressed, input);
    }

    #[test]
    fn test_zip_archive_create_and_extract() {
        let temp_dir = std::env::temp_dir().join(format!("bn_test_zip_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let zip_path = temp_dir.join("test.zip");

        // 1. Create zip with stored and compressed entries
        {
            let mut archive = ZipArchiveHandle::create(&zip_path).expect("create zip");
            archive.add_stored_text("mimetype", "application/epub+zip").expect("add stored text");
            archive.add_text("chapter1.html", "<h1>Chapter 1</h1>").expect("add text");
            archive.add_bytes("data.bin", &[1, 2, 3, 4, 5], false).expect("add bytes");
            archive.close().expect("close zip");
        }

        // 2. Open zip and verify entries
        {
            let mut archive = ZipArchiveHandle::open(&zip_path).expect("open zip");
            assert_eq!(archive.entry_count().unwrap(), 3);
            assert_eq!(archive.entry_name(0).unwrap(), "mimetype");
            assert_eq!(archive.entry_name(1).unwrap(), "chapter1.html");
            assert_eq!(archive.entry_name(2).unwrap(), "data.bin");

            let mime = archive.extract_text("mimetype").expect("extract text");
            assert_eq!(mime, "application/epub+zip");

            let ch1 = archive.extract_text("chapter1.html").expect("extract ch1");
            assert_eq!(ch1, "<h1>Chapter 1</h1>");

            let bin = archive.extract_bytes("data.bin").expect("extract bin");
            assert_eq!(bin, vec![1, 2, 3, 4, 5]);

            // Test extract all
            let extract_dir = temp_dir.join("extracted");
            archive.extract_all(&extract_dir).expect("extract all");

            let extracted_mime = fs::read_to_string(extract_dir.join("mimetype")).expect("read extracted mime");
            assert_eq!(extracted_mime, "application/epub+zip");

            let extracted_ch1 = fs::read_to_string(extract_dir.join("chapter1.html")).expect("read extracted ch1");
            assert_eq!(extracted_ch1, "<h1>Chapter 1</h1>");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }
}

