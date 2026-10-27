// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCompress` — external library module (`IMPORT BNCompress`), served through
//! the provider seam. Owns interpreter-side `Bytes` and `ZipArchive` handles.

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_types::IntegerType;
use bn_value::{shared_string, Value};

use bn_core_compress::{
    decode_hex, deflate, encode_hex, gunzip, gzip, inflate, CompressError, ZipArchiveHandle,
};
use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    name_not_found, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};

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
            .finish()
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
                "BNCompress.Bytes was already released",
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
                Err(type_mismatch("BNCompress.Bytes", "BNCrypto.Bytes value", member, span))
            }
            _ => Err(type_mismatch("BNCompress.Bytes", "non-Bytes value", member, span)),
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
                "BNCompress.ZipArchive was already closed or released",
                span,
            )
        })
    }
}

fn failure(err: &CompressError, operation: &str) -> Value {
    Value::error_report(
        err.code,
        operation,
        err.message.clone(),
        err.message.clone(),
    )
}

impl Provider for CompressProvider {
    fn call(
        &mut self,
        _core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = member.rsplit('.').next().unwrap_or(member);
        match method {
            "FromText" => {
                require_arity(member, &arguments, 1, span)?;
                let text = Self::string_at(&arguments, 0, member, span)?;
                Ok(self.insert_buffer(text.into_bytes()))
            }
            "FromHex" => {
                require_arity(member, &arguments, 1, span)?;
                let hex = Self::string_at(&arguments, 0, member, span)?;
                match decode_hex(&hex) {
                    Some(bytes) => Ok(self.insert_buffer(bytes)),
                    None => Ok(failure(
                        &CompressError::invalid_argument("Malformed hexadecimal string"),
                        member,
                    )),
                }
            }
            "Deflate" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match deflate(&data) {
                    Ok(compressed) => Ok(self.insert_buffer(compressed)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "Inflate" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match inflate(&data) {
                    Ok(decompressed) => Ok(self.insert_buffer(decompressed)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "Gzip" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match gzip(&data) {
                    Ok(compressed) => Ok(self.insert_buffer(compressed)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "Gunzip" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match gunzip(&data) {
                    Ok(decompressed) => Ok(self.insert_buffer(decompressed)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "DeflateText" => {
                require_arity(member, &arguments, 1, span)?;
                let text = Self::string_at(&arguments, 0, member, span)?;
                match deflate(text.as_bytes()) {
                    Ok(compressed) => Ok(self.insert_buffer(compressed)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "InflateText" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match inflate(&data) {
                    Ok(decompressed) => match String::from_utf8(decompressed) {
                        Ok(s) => Ok(Value::String(shared_string(s))),
                        Err(e) => Ok(failure(&CompressError::corrupt_data(e.to_string()), member)),
                    },
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "GzipText" => {
                require_arity(member, &arguments, 1, span)?;
                let text = Self::string_at(&arguments, 0, member, span)?;
                match gzip(text.as_bytes()) {
                    Ok(compressed) => Ok(self.insert_buffer(compressed)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "GunzipText" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match gunzip(&data) {
                    Ok(decompressed) => match String::from_utf8(decompressed) {
                        Ok(s) => Ok(Value::String(shared_string(s))),
                        Err(e) => Ok(failure(&CompressError::corrupt_data(e.to_string()), member)),
                    },
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "Length" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                Ok(Value::Integer(data.len() as i128, IntegerType::Int32))
            }
            "ToHex" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                Ok(Value::String(shared_string(encode_hex(&data))))
            }
            "ToText" => {
                require_arity(member, &arguments, 1, span)?;
                let data = self.bytes_at(&arguments, 0, member, span)?;
                match String::from_utf8(data) {
                    Ok(s) => Ok(Value::String(shared_string(s))),
                    Err(e) => Ok(failure(&CompressError::corrupt_data(e.to_string()), member)),
                }
            }
            // ZipArchive factories
            "Create" => {
                require_arity(member, &arguments, 1, span)?;
                let path = Self::string_at(&arguments, 0, member, span)?;
                match ZipArchiveHandle::create(path) {
                    Ok(archive) => Ok(self.insert_archive(archive)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "Open" => {
                require_arity(member, &arguments, 1, span)?;
                let path = Self::string_at(&arguments, 0, member, span)?;
                match ZipArchiveHandle::open(path) {
                    Ok(archive) => Ok(self.insert_archive(archive)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            // ZipArchive methods
            "AddFile" => {
                require_arity(member, &arguments, 3, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let disk_path = Self::string_at(&arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.add_file(&entry_name, disk_path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "AddText" => {
                require_arity(member, &arguments, 3, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let content = Self::string_at(&arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.add_text(&entry_name, &content) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "AddStoredText" => {
                require_arity(member, &arguments, 3, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let content = Self::string_at(&arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.add_stored_text(&entry_name, &content) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "AddBytes" => {
                require_arity(member, &arguments, 3, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let data = self.bytes_at(&arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.add_bytes(&entry_name, &data, false) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "AddStoredBytes" => {
                require_arity(member, &arguments, 3, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let data = self.bytes_at(&arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.add_stored_bytes(&entry_name, &data) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "ExtractAll" => {
                require_arity(member, &arguments, 2, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let dest = Self::string_at(&arguments, 1, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.extract_all(dest) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "ExtractFile" => {
                require_arity(member, &arguments, 3, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let disk_path = Self::string_at(&arguments, 2, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.extract_file(&entry_name, disk_path) {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "ExtractText" => {
                require_arity(member, &arguments, 2, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.extract_text(&entry_name) {
                    Ok(text) => Ok(Value::String(shared_string(text))),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "ExtractBytes" => {
                require_arity(member, &arguments, 2, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let entry_name = Self::string_at(&arguments, 1, member, span)?;
                let archive = self.archive_mut(handle, span)?;
                match archive.extract_bytes(&entry_name) {
                    Ok(bytes) => Ok(self.insert_buffer(bytes)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "EntryCount" => {
                require_arity(member, &arguments, 1, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let archive = self.archive_mut(handle, span)?;
                match archive.entry_count() {
                    Ok(count) => Ok(Value::Integer(count as i128, IntegerType::Int32)),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "EntryName" => {
                require_arity(member, &arguments, 2, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let idx = match arguments.get(1) {
                    Some(Value::Integer(i, _)) => *i as usize,
                    _ => return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span)),
                };
                let archive = self.archive_mut(handle, span)?;
                match archive.entry_name(idx) {
                    Ok(name) => Ok(Value::String(shared_string(name))),
                    Err(e) => Ok(failure(&e, member)),
                }
            }
            "Close" => {
                require_arity(member, &arguments, 1, span)?;
                let Some(Value::CompressZip(h)) = arguments.first() else {
                    return Err(type_mismatch("ZipArchive", "non-ZipArchive value", member, span));
                };
                let handle = *h;
                let archive = self.archive_mut(handle, span)?;
                match archive.close() {
                    Ok(()) => Ok(Value::Null),
                    Err(e) => Ok(failure(&e, member)),
                }
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
            // Allocate a closed dummy placeholder, actual opening is via Create/Open
            Some(Ok(self.insert_archive(ZipArchiveHandle::open("").unwrap_or_else(|_| {
                ZipArchiveHandle::create("/dev/null").expect("placeholder")
            }))))
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
                    "BNCompress.Bytes was already released",
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
                    "BNCompress.ZipArchive was already released",
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
                    "BNCompress.Bytes was already released",
                    span,
                ))
            }),
            Value::CompressZip(handle) => Some(if let Some(h) = self.archive_holders.get_mut(handle) {
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
                    "BNCompress.ZipArchive was already released",
                    span,
                ))
            }),
            _ => None,
        }
    }
}
