// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCompress` — external library module (`IMPORT BNCompress`), served through
//! the provider seam. Owns interpreter-side `Bytes` and `ZipArchive` handles.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::Path;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_types::IntegerType;
use bn_value::{Value, shared_string};

use bn_core_compress::{
    CompressError, FileAccess, ZipArchiveHandle, decode_hex, deflate, encode_hex, gunzip, gzip,
    inflate,
};
use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    name_not_found, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};
use bn_rt::secure_fs::OpenMode;

pub const NAME: &str = "BNCompress";

#[derive(Default)]
pub struct CompressProvider {
    buffers: HashMap<u64, Vec<u8>>,
    holders: HashMap<u64, usize>,
    archives: HashMap<u64, ZipArchiveHandle>,
    archive_holders: HashMap<u64, usize>,
    next: u64,
}

impl std::fmt::Debug for CompressProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompressProvider")
            .field("buffers_count", &self.buffers.len())
            .field("archives_count", &self.archives.len())
            .finish_non_exhaustive()
    }
}

impl CompressProvider {
    fn insert_buffer(&mut self, bytes: Vec<u8>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.buffers.insert(handle, bytes);
        self.holders.insert(handle, 1);
        Value::CompressBytes(handle)
    }

    fn insert_archive(&mut self, archive: ZipArchiveHandle) -> Value {
        self.next += 1;
        let handle = self.next;
        self.archives.insert(handle, archive);
        self.archive_holders.insert(handle, 1);
        Value::CompressZip(handle)
    }

    fn buffer(&self, handle: u64, span: Span) -> Result<&Vec<u8>, Diagnostic> {
        self.buffers.get(&handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("compress", "msg.bytes_was_already_released"),
                span,
            )
        })
    }

    fn bytes_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Vec<u8>, Diagnostic> {
        match arguments.get(index) {
            Some(Value::CompressBytes(handle)) => Ok(self.buffer(*handle, span)?.clone()),
            Some(Value::CryptoBytes(_)) => {
                // If passed CryptoBytes, we reject unless it's in a shared store or we support it.
                Err(type_mismatch(
                    "BNCompress.Bytes",
                    "BNCrypto.Bytes value",
                    member,
                    span,
                ))
            }
            _ => Err(type_mismatch(
                "BNCompress.Bytes",
                "non-Bytes value",
                member,
                span,
            )),
        }
    }

    fn string_at(
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<String, Diagnostic> {
        match arguments.get(index) {
            Some(Value::String(s)) => Ok(s.to_string()),
            _ => Err(type_mismatch("STRING", "non-STRING value", member, span)),
        }
    }

    fn archive_mut(
        &mut self,
        handle: u64,
        span: Span,
    ) -> Result<&mut ZipArchiveHandle, Diagnostic> {
        self.archives.get_mut(&handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("compress", "msg.archive_was_already_released"),
                span,
            )
        })
    }
}

fn failure(err: &CompressError, operation: &str) -> Value {
    Value::error_report(err.code, operation, err.message.clone(), err.cause.clone())
}

/// `Ok(value)` for a success, an `Error` value for a `BNCompress` failure.
fn outcome<T>(
    result: Result<T, CompressError>,
    member: &str,
    success: impl FnOnce(T) -> Value,
) -> Value {
    result.map_or_else(|error| failure(&error, member), success)
}

fn text_value(bytes: Vec<u8>, member: &str) -> Value {
    match String::from_utf8(bytes) {
        Ok(text) => Value::String(shared_string(text)),
        Err(error) => failure(&CompressError::invalid_utf8(error.to_string()), member),
    }
}

fn count_value(count: usize) -> Value {
    Value::Integer(
        i128::try_from(count).unwrap_or(i128::MAX),
        IntegerType::Int32,
    )
}

impl CompressProvider {
    /// The functions on `Bytes` and the in-memory codecs.
    fn call_buffer(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Option<Result<Value, Diagnostic>> {
        let codec: fn(&[u8]) -> Result<Vec<u8>, CompressError> = match method {
            "Deflate" | "DeflateText" => deflate,
            "Inflate" | "InflateText" => inflate,
            "Gzip" | "GzipText" => gzip,
            "Gunzip" | "GunzipText" => gunzip,
            "FromText" | "FromHex" | "Length" | "ToHex" | "ToText" => |_| Ok(Vec::new()),
            _ => return None,
        };
        Some(self.buffer_operation(method, member, arguments, codec, span))
    }

    fn buffer_operation(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        codec: fn(&[u8]) -> Result<Vec<u8>, CompressError>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        Ok(match method {
            "FromText" => {
                let text = Self::string_at(arguments, 0, member, span)?;
                self.insert_buffer(text.into_bytes())
            }
            "FromHex" => {
                let hex = Self::string_at(arguments, 0, member, span)?;
                match decode_hex(&hex) {
                    Some(bytes) => self.insert_buffer(bytes),
                    None => failure(&CompressError::malformed_hex(), member),
                }
            }
            "DeflateText" | "GzipText" => {
                let text = Self::string_at(arguments, 0, member, span)?;
                let result = codec(text.as_bytes());
                outcome(result, member, |bytes| self.insert_buffer(bytes))
            }
            "InflateText" | "GunzipText" => {
                let data = self.bytes_at(arguments, 0, member, span)?;
                outcome(codec(&data), member, |bytes| text_value(bytes, member))
            }
            "Length" => count_value(self.bytes_at(arguments, 0, member, span)?.len()),
            "ToHex" => {
                let data = self.bytes_at(arguments, 0, member, span)?;
                Value::String(shared_string(encode_hex(&data)))
            }
            "ToText" => text_value(self.bytes_at(arguments, 0, member, span)?, member),
            _ => {
                let data = self.bytes_at(arguments, 0, member, span)?;
                let result = codec(&data);
                outcome(result, member, |bytes| self.insert_buffer(bytes))
            }
        })
    }

    /// The handle of the `ZipArchive` a method was called on.
    fn archive_handle(arguments: &[Value], member: &str, span: Span) -> Result<u64, Diagnostic> {
        match arguments.first() {
            Some(Value::CompressZip(handle)) => Ok(*handle),
            _ => Err(type_mismatch(
                "ZipArchive",
                "non-ZipArchive value",
                member,
                span,
            )),
        }
    }

    /// The methods of `ZipArchive`; `None` for any other name.
    fn call_archive(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        check: &dyn FileAccess,
        span: Span,
    ) -> Option<Result<Value, Diagnostic>> {
        let arity = match method {
            "AddFile" | "AddText" | "AddStoredText" | "AddBytes" | "AddStoredBytes"
            | "ExtractFile" => 3,
            "ExtractAll" | "ExtractText" | "ExtractBytes" | "EntryName" => 2,
            "EntryCount" | "Close" => 1,
            _ => return None,
        };
        Some(
            require_arity(member, arguments, arity, span)
                .and_then(|()| self.archive_method(method, member, arguments, check, span)),
        )
    }

    fn archive_method(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        check: &dyn FileAccess,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let handle = Self::archive_handle(arguments, member, span)?;
        let unit = |()| Value::Null;
        let value = match method {
            "AddFile" | "ExtractFile" => {
                let entry = Self::string_at(arguments, 1, member, span)?;
                let path = Self::string_at(arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                let result = if method == "AddFile" {
                    archive.add_file(&entry, path, check)
                } else {
                    archive.extract_file(&entry, path, check)
                };
                outcome(result, member, unit)
            }
            "AddText" | "AddStoredText" => {
                let entry = Self::string_at(arguments, 1, member, span)?;
                let content = Self::string_at(arguments, 2, member, span)?;
                let stored = method == "AddStoredText";
                let archive = self.archive_mut(handle, span)?;
                outcome(
                    archive.add_bytes(&entry, content.as_bytes(), stored),
                    member,
                    unit,
                )
            }
            "AddBytes" | "AddStoredBytes" => {
                let entry = Self::string_at(arguments, 1, member, span)?;
                let data = self.bytes_at(arguments, 2, member, span)?;
                let stored = method == "AddStoredBytes";
                let archive = self.archive_mut(handle, span)?;
                outcome(archive.add_bytes(&entry, &data, stored), member, unit)
            }
            "ExtractAll" => {
                let destination = Self::string_at(arguments, 1, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                outcome(archive.extract_all(destination, check), member, unit)
            }
            "ExtractText" => {
                let entry = Self::string_at(arguments, 1, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                outcome(archive.extract_text(&entry), member, |text| {
                    Value::String(shared_string(text))
                })
            }
            "ExtractBytes" => {
                let entry = Self::string_at(arguments, 1, member, span)?;
                let result = self.archive_mut(handle, span)?.extract_bytes(&entry);
                outcome(result, member, |bytes| self.insert_buffer(bytes))
            }
            "EntryName" => {
                let Some(Value::Integer(index, _)) = arguments.get(1) else {
                    return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                };
                let Ok(index) = usize::try_from(*index) else {
                    return Ok(failure(&CompressError::invalid_index(), member));
                };
                let archive = self.archive_mut(handle, span)?;
                outcome(archive.entry_name(index), member, |name| {
                    Value::String(shared_string(name))
                })
            }
            "EntryCount" => outcome(
                self.archive_mut(handle, span)?.entry_count(),
                member,
                count_value,
            ),
            _ => outcome(self.archive_mut(handle, span)?.close(), member, unit),
        };
        Ok(value)
    }
}

impl Provider for CompressProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = member.rsplit('.').next().unwrap_or(member);
        if let Some(result) = self.call_buffer(method, member, &arguments, span) {
            return result;
        }
        // Every file BNCompress touches obeys the HOST.FileSystem policy
        // (language/0.6/bncompress.md, "File access and execution policy").
        let check = HostFiles(core.host().filesystem());
        if let Some(result) = self.call_archive(method, member, &arguments, &check, span) {
            return result;
        }
        match method {
            "Create" | "Open" => {
                require_arity(member, &arguments, 1, span)?;
                let path = Self::string_at(&arguments, 0, member, span)?;
                let result = if method == "Create" {
                    ZipArchiveHandle::create(path, &check)
                } else {
                    ZipArchiveHandle::open(path, &check)
                };
                Ok(outcome(result, member, |archive| {
                    self.insert_archive(archive)
                }))
            }
            "CONSTRUCTOR" => Ok(Value::Null),
            _ => Err(name_not_found(method, "BNCompress function", span)),
        }
    }

    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        let base = class.rsplit('.').next();
        if base == Some("Bytes") {
            Some(Ok(self.insert_buffer(Vec::new())))
        } else if base == Some("ZipArchive") {
            // `NEW` makes an archive that holds no file until `Create` or `Open`.
            Some(Ok(self.insert_archive(ZipArchiveHandle::closed())))
        } else {
            None
        }
    }

    fn retain(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        match value {
            Value::CompressBytes(handle) => Some(match self.holders.get_mut(handle) {
                Some(h) => {
                    *h += 1;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("compress", "msg.bytes_was_already_released"),
                    span,
                )),
            }),
            Value::CompressZip(handle) => Some(match self.archive_holders.get_mut(handle) {
                Some(h) => {
                    *h += 1;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("compress", "msg.archive_was_already_released"),
                    span,
                )),
            }),
            _ => None,
        }
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        match value {
            Value::CompressBytes(handle) => Some(if let Some(h) = self.holders.get_mut(handle) {
                *h -= 1;
                if *h == 0 {
                    self.holders.remove(handle);
                    self.buffers.remove(handle);
                }
                Ok(())
            } else {
                Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("compress", "msg.bytes_was_already_released"),
                    span,
                ))
            }),
            Value::CompressZip(handle) => {
                Some(if let Some(h) = self.archive_holders.get_mut(handle) {
                    *h -= 1;
                    if *h == 0 {
                        self.archive_holders.remove(handle);
                        if let Some(mut archive) = self.archives.remove(handle) {
                            let _ = archive.close();
                        }
                    }
                    Ok(())
                } else {
                    Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        bn_catalog::text("compress", "msg.archive_was_already_released"),
                        span,
                    ))
                })
            }
            _ => None,
        }
    }
}

/// The interpreter's `HOST.FileSystem` policy, through which every file
/// `BNCompress` touches is checked and opened; its rooted operations never
/// follow a symlink (bucket 0.6.11, CMP-SEC-1).
struct HostFiles<'a>(&'a bn_rt::FsPolicy);

impl FileAccess for HostFiles<'_> {
    fn check(&self, path: &Path, write: bool) -> Result<(), String> {
        if self.0.allows_path(path, write) {
            Ok(())
        } else {
            Err(self.0.denial_reason(write).to_string())
        }
    }
    fn open_read(&self, path: &Path) -> io::Result<File> {
        self.0.open(path, OpenMode::Read)
    }
    fn create(&self, path: &Path) -> io::Result<File> {
        self.0.open(path, OpenMode::Write)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.0.create_dir_all(path)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.0.remove_file(path)
    }
    fn is_refusal(&self, error: &io::Error) -> bool {
        bn_rt::secure_fs::is_policy_refusal(error)
    }
}
