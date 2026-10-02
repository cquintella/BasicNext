// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` — files under the host's filesystem policy. The
//! semantics are `bn_rt::file`, shared with the native runtime; this provider
//! only converts between `Value`s and that core. `File` handles are
//! `Value::File` ids owned here; `NEW FS.File()` and `RELEASE` reach this
//! provider through `allocate`/`release`.

use std::collections::HashMap;
use std::path::Path;

use bn_diag::Diagnostic;
use bn_rt::file::{FileError, OpenFile};
use bn_source::Span;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};

use bn_interp::{
    index_out_of_bounds_pub as index_out_of_bounds, integer_pub as integer, name_not_found,
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};
use bn_types::IntegerType;

pub const NAME: &str = "FileSystem";

#[derive(Debug)]
pub struct FsProvider {
    files: HashMap<u64, OpenFile>,
    next_file: u64,
}

impl Default for FsProvider {
    fn default() -> Self {
        Self {
            files: HashMap::new(),
            next_file: 1,
        }
    }
}

/// `Ok` maps to the BN value; `Err` (including a policy denial, 0.6.md
/// "`HOST.FileSystem` execution policy") to a BN `Error` with that message.
fn method_result<T>(result: Result<T, FileError>, value: impl FnOnce(T) -> Value) -> Value {
    result.map_or_else(|failure| file_error(&failure), value)
}

/// The BN `Error` of a failed operation: the core's report, unchanged.
fn file_error(failure: &FileError) -> Value {
    Value::error_report(
        failure.code(),
        failure.operation(),
        failure.message(),
        failure.cause(),
    )
}

fn path_argument<'a>(
    arguments: &'a [Value],
    context: &str,
    span: Span,
) -> Result<&'a Path, Diagnostic> {
    let Value::String(path) = &arguments[0] else {
        return Err(type_mismatch("STRING", "non-STRING value", context, span));
    };
    Ok(Path::new(path.as_ref()))
}

impl Provider for FsProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if member.starts_with("File.") {
            return self.file_call(core, member, &arguments, span);
        }
        let name = format!("HOST.FileSystem.{member}");
        let name = name.as_str();
        let arguments = &arguments;
        match member {
            "Exists" => {
                require_arity(name, arguments, 1, span)?;
                let path = path_argument(arguments, "HOST.FileSystem.Exists path", span)?;
                Ok(method_result(
                    bn_rt::file::exists(core.host().filesystem(), path),
                    Value::Boolean,
                ))
            }
            "Open" => {
                require_arity(name, arguments, 2, span)?;
                let path = path_argument(arguments, "HOST.FileSystem.Open path", span)?;
                let (mode, _) = integer(&arguments[1], span)?;
                let mode = match bn_rt::file::open_mode(path, mode) {
                    Ok(mode) => mode,
                    Err(failure) => return Ok(file_error(&failure)),
                };
                let opened = bn_rt::file::open(core.host().filesystem(), path, mode).map(|file| {
                    let id = self.next_file;
                    self.next_file += 1;
                    self.files.insert(id, file);
                    Value::File(id)
                });
                Ok(method_result(opened, |value| value))
            }
            "DeleteFile" => {
                require_arity(name, arguments, 1, span)?;
                let path = path_argument(arguments, "HOST.FileSystem.DeleteFile path", span)?;
                Ok(method_result(
                    bn_rt::file::delete_file(core.host().filesystem(), path),
                    |()| Value::Null,
                ))
            }
            _ => Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }

    /// `NEW FS.File()` — a closed handle until `HOST.FileSystem.Open`.
    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        if class != "File" {
            return None;
        }
        let id = self.next_file;
        self.next_file += 1;
        self.files.insert(id, OpenFile::default());
        Some(Ok(Value::File(id)))
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::File(id) = value else {
            return None;
        };
        Some(if self.files.remove(id).is_some() {
            Ok(())
        } else {
            Err(runtime_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                "file handle was already deleted",
                span,
            ))
        })
    }
}

impl FsProvider {
    fn file_call(
        &mut self,
        core: &mut dyn CoreContext,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let Value::File(id) = arguments
            .first()
            .ok_or_else(|| type_mismatch("FS.File", "missing receiver", "file operation", span))?
        else {
            return Err(type_mismatch(
                "FS.File",
                "non-FS.File value",
                "file operation receiver",
                span,
            ));
        };
        let method = name.rsplit('.').next().unwrap_or_default();
        // Buffer arguments are read from memory before the file is borrowed.
        let bytes = if method == "WriteBytes" {
            Some(write_bytes_argument(core, arguments, span)?)
        } else {
            None
        };
        let file = self.files.get_mut(id).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "file handle is invalid",
                span,
            )
        })?;
        match method {
            "Close" => {
                require_arity(name, arguments, 1, span)?;
                Ok(method_result(file.close(), |()| Value::Null))
            }
            "ReadAll" => {
                require_arity(name, arguments, 1, span)?;
                Ok(method_result(file.read_all(), |text| {
                    Value::String(shared_string(text))
                }))
            }
            "ReadLine" => {
                require_arity(name, arguments, 1, span)?;
                Ok(method_result(file.read_line(), |line| {
                    line.map_or(Value::EndOfFile, |line| Value::String(shared_string(line)))
                }))
            }
            "Write" | "WriteLine" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(text) = &arguments[1] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "FS.File.Write text",
                        span,
                    ));
                };
                Ok(method_result(
                    file.write(text, method == "WriteLine"),
                    |()| Value::Null,
                ))
            }
            "ReadBytes" => {
                require_arity(name, arguments, 2, span)?;
                let Value::Pointer { handle } = arguments[1] else {
                    return Err(type_mismatch(
                        "BYTE buffer",
                        "non-pointer value",
                        "FS.File.ReadBytes buffer",
                        span,
                    ));
                };
                let mut buffer = vec![0; core.memory().len(handle, span)?];
                let count = match file.read_bytes(&mut buffer) {
                    Ok(Some(count)) => count,
                    Ok(None) => return Ok(Value::EndOfFile),
                    Err(failure) => return Ok(file_error(&failure)),
                };
                for (index, byte) in buffer.into_iter().take(count).enumerate() {
                    *core.memory_mut().get_mut(handle, index, span)? =
                        Value::Integer(i128::from(byte), IntegerType::Byte);
                }
                Ok(Value::Integer(
                    i128::try_from(count).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "WriteBytes" => {
                let bytes = bytes.expect("read before the file borrow");
                Ok(method_result(file.write_bytes(&bytes), |()| Value::Null))
            }
            _ => Err(name_not_found(name, "FS.File method", span)),
        }
    }
}

/// `WriteBytes(buffer, count)`: the first `count` BYTE values of `buffer`;
/// `count` outside `0..=LEN(buffer)` is `INDEX_OUT_OF_BOUNDS`.
fn write_bytes_argument(
    core: &dyn CoreContext,
    arguments: &[Value],
    span: Span,
) -> Result<Vec<u8>, Diagnostic> {
    require_arity("FS.File.WriteBytes", arguments, 3, span)?;
    let Value::Pointer { handle } = arguments[1] else {
        return Err(type_mismatch(
            "BYTE buffer",
            "non-pointer value",
            "FS.File.WriteBytes buffer",
            span,
        ));
    };
    let (count, _) = integer(&arguments[2], span)?;
    let len = core.memory().len(handle, span)?;
    let count = usize::try_from(count)
        .ok()
        .filter(|count| *count <= len)
        .ok_or_else(|| index_out_of_bounds(count, len, "BYTE buffer", span))?;
    (0..count)
        .map(|index| {
            let value = core.memory().get(handle, index, span)?;
            let (value, _) = integer(value, span)?;
            u8::try_from(value).map_err(|_| {
                type_mismatch(
                    "BYTE",
                    "non-BYTE value",
                    "FS.File.WriteBytes buffer element",
                    span,
                )
            })
        })
        .collect()
}
