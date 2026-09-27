// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.FileSystem` — files under the host's filesystem policy. `File`
//! handles are `Value::File` ids owned here; `NEW FS.File()` and `RELEASE`
//! reach this provider through `allocate`/`release`.

#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // Moved verbatim from the core (bucket 0.5.1d 1.5).

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use std::io::{Read, Write};

use bn_interp::{
    index_out_of_bounds_pub as index_out_of_bounds, integer_pub as integer, name_not_found,
    require_arity_pub as require_arity, runtime_error_pub as runtime_error, type_mismatch,
};
use bn_types::IntegerType;

pub const NAME: &str = "FileSystem";

struct FileResource {
    file: Option<std::fs::File>,
    family: Option<bool>, // ponytail: one bit for text/binary; expand only if modes grow.
}

pub struct FsProvider {
    files: HashMap<u64, FileResource>,
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
                let Value::String(path) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.FileSystem.Exists path",
                        span,
                    ));
                };
                match core.host().filesystem().open(
                    std::path::Path::new(path.as_ref()),
                    bn_rt::secure_fs::OpenMode::Read,
                ) {
                    Ok(file) => Ok(Value::Boolean(
                        file.metadata().is_ok_and(|meta| meta.is_file()),
                    )),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(Value::Boolean(false))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        Err(runtime_error(
                            bn_diag::DiagId::EXECUTION_POLICY_DENIED,
                            "filesystem read is outside the execution policy",
                            span,
                        ))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    }),
                }
            }
            "Open" => {
                require_arity(name, arguments, 2, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.FileSystem.Open path",
                        span,
                    ));
                };
                let (mode, _) = integer(&arguments[1], span)?;
                let open_mode = match mode {
                    0 => bn_rt::secure_fs::OpenMode::Read,
                    1 => bn_rt::secure_fs::OpenMode::Write,
                    2 => bn_rt::secure_fs::OpenMode::Append,
                    _ => {
                        return Ok(Value::Error {
                            code: 1,
                            message: "unknown file mode".into(),
                        });
                    }
                };
                let result = core
                    .host()
                    .filesystem()
                    .open(std::path::Path::new(path.as_ref()), open_mode);
                match result {
                    Ok(file) => {
                        if file.metadata().is_ok_and(|meta| meta.is_dir()) {
                            return Ok(Value::Error {
                                code: 1,
                                message: "path is a directory".into(),
                            });
                        }
                        let id = self.next_file;
                        self.next_file += 1;
                        self.files.insert(
                            id,
                            FileResource {
                                file: Some(file),
                                family: None,
                            },
                        );
                        Ok(Value::File(id))
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        Err(runtime_error(
                            bn_diag::DiagId::EXECUTION_POLICY_DENIED,
                            "filesystem path is outside the execution policy",
                            span,
                        ))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    }),
                }
            }
            "DeleteFile" => {
                require_arity(name, arguments, 1, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(type_mismatch(
                        "STRING",
                        "non-STRING value",
                        "HOST.FileSystem.DeleteFile path",
                        span,
                    ));
                };
                match core
                    .host()
                    .filesystem()
                    .remove_file(std::path::Path::new(path.as_ref()))
                {
                    Ok(()) => Ok(Value::Null),
                    Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                        Err(runtime_error(
                            bn_diag::DiagId::EXECUTION_POLICY_DENIED,
                            "filesystem deletion is outside the execution policy",
                            span,
                        ))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    }),
                }
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
        self.files.insert(
            id,
            FileResource {
                file: None,
                family: None,
            },
        );
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
        match name.rsplit('.').next().unwrap_or_default() {
            "ReadBytes" => return self.file_read_bytes(core, *id, arguments, span),
            "WriteBytes" => return self.file_write_bytes(core, *id, arguments, span),
            _ => {}
        }
        let resource = self.files.get_mut(id).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "file handle is invalid",
                span,
            )
        })?;
        match name.rsplit('.').next().unwrap_or_default() {
            "Close" => {
                require_arity(name, arguments, 1, span)?;
                let Some(file) = resource.file.take() else {
                    return Ok(Value::Null);
                };
                resource.family = None;
                match file.sync_all() {
                    Ok(()) => Ok(Value::Null),
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    }),
                }
            }
            "ReadAll" => {
                require_arity(name, arguments, 1, span)?;
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let mut text = String::new();
                match file.read_to_string(&mut text) {
                    Ok(_) => {
                        resource.family = Some(true);
                        Ok(Value::String(shared_string(text)))
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    }),
                }
            }
            "ReadLine" => {
                require_arity(name, arguments, 1, span)?;
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                let mut bytes = Vec::new();
                let mut read_any = false;
                let mut one = [0_u8; 1];
                loop {
                    match file.read(&mut one) {
                        Ok(0) => break,
                        Ok(_) if one[0] == b'\n' => {
                            read_any = true;
                            break;
                        }
                        Ok(_) => {
                            read_any = true;
                            bytes.push(one[0]);
                        }
                        Err(error) => {
                            return Ok(Value::Error {
                                code: 1,
                                message: shared_string(error.to_string()),
                            });
                        }
                    }
                }
                if !read_any {
                    resource.family = Some(true);
                    return Ok(Value::EndOfFile);
                }
                if bytes.last() == Some(&b'\r') {
                    bytes.pop();
                }
                Ok(String::from_utf8(bytes).map_or_else(
                    |error| Value::Error {
                        code: 1,
                        message: shared_string(format!("INVALID_UTF8: {error}")),
                    },
                    |text| {
                        resource.family = Some(true);
                        Value::String(shared_string(text))
                    },
                ))
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
                let Some(file) = resource.file.as_mut() else {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is closed".into(),
                    });
                };
                if resource.family == Some(false) {
                    return Ok(Value::Error {
                        code: 1,
                        message: "file is in binary mode".into(),
                    });
                }
                let text = if name.ends_with("WriteLine") {
                    format!("{text}\n")
                } else {
                    text.to_string()
                };
                match file.write_all(text.as_bytes()) {
                    Ok(()) => {
                        resource.family = Some(true);
                        Ok(Value::Null)
                    }
                    Err(error) => Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    }),
                }
            }
            _ => Err(name_not_found(name, "FS.File method", span)),
        }
    }

    fn file_read_bytes(
        &mut self,
        core: &mut dyn CoreContext,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity("FS.File.ReadBytes", arguments, 2, span)?;
        let Value::Pointer { handle } = arguments[1] else {
            return Err(type_mismatch(
                "BYTE buffer",
                "non-pointer value",
                "FS.File.ReadBytes buffer",
                span,
            ));
        };
        let (bytes, eof) = {
            let resource = self.files.get_mut(&id).ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "file handle is invalid",
                    span,
                )
            })?;
            if resource.family == Some(true) {
                return Ok(Value::Error {
                    code: 1,
                    message: "file is in text mode".into(),
                });
            }
            let Some(file) = resource.file.as_mut() else {
                return Ok(Value::Error {
                    code: 1,
                    message: "file is closed".into(),
                });
            };
            let len = core.memory().len(handle, span)?;
            let mut bytes = vec![0; len];
            let count = match file.read(&mut bytes) {
                Ok(count) => count,
                Err(error) => {
                    return Ok(Value::Error {
                        code: 1,
                        message: shared_string(error.to_string()),
                    });
                }
            };
            resource.family = Some(false);
            (bytes, count)
        };
        if eof == 0 {
            return Ok(Value::EndOfFile);
        }
        for (index, byte) in bytes.into_iter().take(eof).enumerate() {
            *core.memory_mut().get_mut(handle, index, span)? =
                Value::Integer(i128::from(byte), IntegerType::Byte);
        }
        Ok(Value::Integer(
            i128::try_from(eof).unwrap_or(i128::MAX),
            IntegerType::Int32,
        ))
    }

    fn file_write_bytes(
        &mut self,
        core: &mut dyn CoreContext,
        id: u64,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
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
        let bytes = (0..count)
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
            .collect::<Result<Vec<_>, _>>()?;
        let resource = self.files.get_mut(&id).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "file handle is invalid",
                span,
            )
        })?;
        if resource.family == Some(true) {
            return Ok(Value::Error {
                code: 1,
                message: "file is in text mode".into(),
            });
        }
        let Some(file) = resource.file.as_mut() else {
            return Ok(Value::Error {
                code: 1,
                message: "file is closed".into(),
            });
        };
        if let Err(error) = file.write_all(&bytes) {
            return Ok(Value::Error {
                code: 1,
                message: shared_string(error.to_string()),
            });
        }
        resource.family = Some(false);
        Ok(Value::Null)
    }
}
