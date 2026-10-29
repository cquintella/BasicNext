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

/// One open or closed file and the references that keep it alive.
#[derive(Debug)]
struct FileSlot {
    file: OpenFile,
    references: u32,
}

#[derive(Debug)]
pub struct FsProvider {
    files: HashMap<u64, FileSlot>,
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
    path_at(arguments, 0, context, span)
}

/// The path argument at `index`; `context` names the operation in a type
/// mismatch.
fn path_at<'a>(
    arguments: &'a [Value],
    index: usize,
    context: &str,
    span: Span,
) -> Result<&'a Path, Diagnostic> {
    let Value::String(path) = &arguments[index] else {
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
                let opened = bn_rt::file::open(core.host().filesystem(), path, mode);
                Ok(method_result(opened, |file| self.insert_file(file)))
            }
            "DeleteFile" => {
                require_arity(name, arguments, 1, span)?;
                let path = path_argument(arguments, "HOST.FileSystem.DeleteFile path", span)?;
                Ok(method_result(
                    bn_rt::file::delete_file(core.host().filesystem(), path),
                    |()| Value::Null,
                ))
            }
            _ => self.directory_call(core, member, name, arguments, span),
        }
    }

    /// `NEW FS.File()` — a closed handle until `HOST.FileSystem.Open`.
    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        if class != "File" {
            return None;
        }
        let id = self.next_file;
        self.next_file += 1;
        self.files.insert(
            id,
            FileSlot {
                file: OpenFile::default(),
                references: 1,
            },
        );
        Some(Ok(Value::File(id)))
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::File(id) = value else {
            return None;
        };
        Some(match self.files.get_mut(id) {
            Some(slot) if slot.references > 1 => {
                slot.references -= 1;
                Ok(())
            }
            Some(_) => {
                self.files.remove(id);
                Ok(())
            }
            None => Err(runtime_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                "file handle was already deleted",
                span,
            )),
        })
    }

    fn retain(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::File(id) = value else {
            return None;
        };
        Some(match self.files.get_mut(id) {
            Some(slot) => match slot.references.checked_add(1) {
                Some(count) => {
                    slot.references = count;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("file", "msg.file_handle_count_overflowed"),
                    span,
                )),
            },
            None => Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("file", "msg.file_handle_is_invalid"),
                span,
            )),
        })
    }
}

impl FsProvider {
    /// Stores an open file under a new handle with one reference.
    fn insert_file(&mut self, file: OpenFile) -> Value {
        let id = self.next_file;
        self.next_file += 1;
        self.files.insert(
            id,
            FileSlot {
                file,
                references: 1,
            },
        );
        Value::File(id)
    }

    /// The directory, private-create, copy and move methods (host.md,
    /// "Directories, private files, copy and move"; bucket 0.6.12): argument
    /// conversion only, over `bn_rt::file` with the interpreter's policy.
    fn directory_call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        name: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let policy = core.host().filesystem();
        let unit = |()| Value::Null;
        match member {
            "GetCurrentDirectory" => {
                require_arity(name, arguments, 0, span)?;
                Ok(method_result(
                    bn_rt::file::get_current_directory(policy),
                    |text| Value::String(shared_string(text)),
                ))
            }
            "SetCurrentDirectory"
            | "CreateDirectory"
            | "RemoveDirectory"
            | "IsDirectory"
            | "OpenNew" => {
                require_arity(name, arguments, 1, span)?;
                let path = path_at(arguments, 0, name, span)?;
                Ok(match member {
                    "SetCurrentDirectory" => {
                        method_result(bn_rt::file::set_current_directory(policy, path), unit)
                    }
                    "CreateDirectory" => {
                        method_result(bn_rt::file::create_directory(policy, path), unit)
                    }
                    "RemoveDirectory" => {
                        method_result(bn_rt::file::remove_directory(policy, path), unit)
                    }
                    "IsDirectory" => {
                        method_result(bn_rt::file::is_directory(policy, path), Value::Boolean)
                    }
                    _ => {
                        let created = bn_rt::file::open_new(policy, path);
                        method_result(created, |file| self.insert_file(file))
                    }
                })
            }
            "Copy" | "Move" => {
                require_arity(name, arguments, 2, span)?;
                let source = path_at(arguments, 0, name, span)?;
                let destination = path_at(arguments, 1, name, span)?;
                let result = if member == "Copy" {
                    bn_rt::file::copy(policy, source, destination)
                } else {
                    bn_rt::file::move_entry(policy, source, destination)
                };
                Ok(method_result(result, unit))
            }
            _ => Err(runtime_error(
                bn_diag::DiagId::HOST_CAPABILITY_UNAVAILABLE,
                format!("host function '{name}' is not available"),
                span,
            )),
        }
    }

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
        let file = self
            .files
            .get_mut(id)
            .map(|slot| &mut slot.file)
            .ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("file", "msg.file_handle_is_invalid"),
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
                let Value::Pointer { handle, .. } = arguments[1] else {
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
    let Value::Pointer { handle, .. } = arguments[1] else {
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
