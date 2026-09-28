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
use bn_rt::file::{FsError, OpenFile};
use bn_rt::secure_fs::OpenMode;
use bn_source::Span;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};

use bn_interp::{
    index_out_of_bounds_pub as index_out_of_bounds, integer_pub as integer, name_not_found,
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};
use bn_types::IntegerType;

pub const NAME: &str = "FileSystem";

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

fn error(message: impl Into<String>) -> Value {
    Value::Error {
        code: 1,
        message: shared_string(message.into()),
    }
}

/// A capability result: policy denial is a runtime diagnostic, any other
/// failure a BN `Error`.
fn capability_result(
    result: Result<Value, FsError>,
    denied: &str,
    span: Span,
) -> Result<Value, Diagnostic> {
    match result {
        Ok(value) => Ok(value),
        Err(FsError::Denied) => Err(runtime_error(
            bn_diag::DiagId::EXECUTION_POLICY_DENIED,
            denied,
            span,
        )),
        Err(FsError::Failed(message)) => Ok(error(message)),
    }
}

/// A method result: `Ok` maps to the BN value, `Err` to a BN `Error`.
fn method_result<T>(result: Result<T, String>, value: impl FnOnce(T) -> Value) -> Value {
    result.map_or_else(error, value)
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
                capability_result(
                    bn_rt::file::exists(core.host().filesystem(), path).map(Value::Boolean),
                    "filesystem read is outside the execution policy",
                    span,
                )
            }
            "Open" => {
                require_arity(name, arguments, 2, span)?;
                let path = path_argument(arguments, "HOST.FileSystem.Open path", span)?;
                let (mode, _) = integer(&arguments[1], span)?;
                let mode = match mode {
                    0 => OpenMode::Read,
                    1 => OpenMode::Write,
                    2 => OpenMode::Append,
                    _ => return Ok(error("unknown file mode")),
                };
                let opened = bn_rt::file::open(core.host().filesystem(), path, mode).map(|file| {
                    let id = self.next_file;
                    self.next_file += 1;
                    self.files.insert(id, file);
                    Value::File(id)
                });
                capability_result(
                    opened,
                    "filesystem path is outside the execution policy",
                    span,
                )
            }
            "DeleteFile" => {
                require_arity(name, arguments, 1, span)?;
                let path = path_argument(arguments, "HOST.FileSystem.DeleteFile path", span)?;
                capability_result(
                    bn_rt::file::delete_file(core.host().filesystem(), path).map(|()| Value::Null),
                    "filesystem deletion is outside the execution policy",
                    span,
                )
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
                    Err(message) => return Ok(error(message)),
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
