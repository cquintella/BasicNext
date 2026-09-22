// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto` — an external library module (`IMPORT BNCrypto`), served through
//! the provider seam. This file marshals `Value` arguments and diagnostics and
//! owns the interpreter-side `Bytes` registry; the digests and hex codecs live
//! in `bn_rt::crypto`, shared with the compiled path so the two backends cannot
//! drift. Nothing here is language.

use std::collections::HashMap;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_types::IntegerType;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    name_not_found, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};

pub const NAME: &str = "BNCrypto";

/// Interpreter-side owner of `BNCrypto.Bytes` buffers. Compiled programs use
/// the parallel table in `bn_rt::crypto`; neither observes the other, and both
/// hand out opaque `u64` handles.
#[derive(Default)]
pub struct CryptoProvider {
    buffers: HashMap<u64, Vec<u8>>,
    next: u64,
}

impl CryptoProvider {
    fn insert(&mut self, bytes: Vec<u8>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.buffers.insert(handle, bytes);
        Value::CryptoBytes(handle)
    }

    /// Seals or opens under one of the two AEAD algorithms. Every operand is a
    /// `Bytes` handle; a rejected key, nonce or tag becomes an `Error` value the
    /// caller must handle, never a partial or unauthenticated buffer.
    fn aead(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 4, span)?;
        let mut operands = Vec::with_capacity(4);
        for argument in arguments {
            let Value::CryptoBytes(handle) = argument else {
                return Err(type_mismatch(
                    "BNCrypto.Bytes",
                    "non-BNCrypto.Bytes value",
                    member,
                    span,
                ));
            };
            operands.push(self.buffer(*handle, span)?.clone());
        }
        let algorithm = if method.ends_with("AesGcm") {
            bn_rt::crypto::Aead::Aes256Gcm
        } else {
            bn_rt::crypto::Aead::ChaCha20Poly1305
        };
        let sealing = method.starts_with("Seal");
        let result = if sealing {
            bn_rt::crypto::seal(
                algorithm,
                &operands[0],
                &operands[1],
                &operands[2],
                &operands[3],
            )
        } else {
            bn_rt::crypto::open(
                algorithm,
                &operands[0],
                &operands[1],
                &operands[2],
                &operands[3],
            )
        };
        match result {
            Some(bytes) => Ok(self.insert(bytes)),
            None => Ok(Value::Error {
                code: 1,
                message: if sealing {
                    "BNCrypto seal rejected the key or nonce length".into()
                } else {
                    "BNCrypto open failed: authentication tag did not verify".into()
                },
            }),
        }
    }

    fn buffer(&self, handle: u64, span: Span) -> Result<&Vec<u8>, Diagnostic> {
        self.buffers.get(&handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNCrypto.Bytes is invalid",
                span,
            )
        })
    }
}

impl Provider for CryptoProvider {
    fn call(
        &mut self,
        _core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = member.rsplit('.').next().unwrap_or_default();
        match method {
            "SHA256" | "SHA512" => {
                require_arity(member, &arguments, 1, span)?;
                let Value::String(data) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let digest = if method == "SHA256" {
                    bn_rt::crypto::sha256_hex(data.as_bytes())
                } else {
                    bn_rt::crypto::sha512_hex(data.as_bytes())
                };
                Ok(Value::String(shared_string(digest.as_str())))
            }
            "FromText" => {
                require_arity(member, &arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                Ok(self.insert(text.as_bytes().to_vec()))
            }
            "FromHex" => {
                require_arity(member, &arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_rt::crypto::decode_hex(text) {
                    Some(bytes) => Ok(self.insert(bytes)),
                    None => Ok(Value::Error {
                        code: 1,
                        message: "BNCrypto.FromHex expects an even-length hexadecimal string"
                            .into(),
                    }),
                }
            }
            "Length" => {
                require_arity(member, &arguments, 1, span)?;
                let Value::CryptoBytes(handle) = arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.Bytes",
                        "non-BNCrypto.Bytes value",
                        member,
                        span,
                    ));
                };
                let length = self.buffer(handle, span)?.len();
                Ok(Value::Integer(
                    i128::try_from(length).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "ToHex" => {
                require_arity(member, &arguments, 1, span)?;
                let Value::CryptoBytes(handle) = arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.Bytes",
                        "non-BNCrypto.Bytes value",
                        member,
                        span,
                    ));
                };
                let hex = bn_rt::crypto::encode_hex(self.buffer(handle, span)?);
                Ok(Value::String(shared_string(hex.as_str())))
            }
            "SealAesGcm" | "SealChaCha20" | "OpenAesGcm" | "OpenChaCha20" => {
                self.aead(method, member, &arguments, span)
            }
            "CONSTRUCTOR" => Ok(Value::Null),
            _ => Err(name_not_found(method, "BNCrypto function", span)),
        }
    }

    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        (class.rsplit('.').next() == Some("Bytes")).then(|| Ok(self.insert(Vec::new())))
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        let Value::CryptoBytes(handle) = value else {
            return None;
        };
        Some(if self.buffers.remove(handle).is_some() {
            Ok(())
        } else {
            Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNCrypto.Bytes was already released",
                span,
            ))
        })
    }
}
