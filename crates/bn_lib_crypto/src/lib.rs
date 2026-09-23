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

    /// Reads the `Bytes` operand at `index`, or reports the type mismatch.
    fn bytes_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Vec<u8>, Diagnostic> {
        let Some(Value::CryptoBytes(handle)) = arguments.get(index) else {
            return Err(type_mismatch(
                "BNCrypto.Bytes",
                "non-BNCrypto.Bytes value",
                member,
                span,
            ));
        };
        Ok(self.buffer(*handle, span)?.clone())
    }

    /// ML-KEM-768 and ML-DSA-65 (FIPS 203 / FIPS 204). Key generation is
    /// deterministic from a seed; encapsulation deliberately is not, because
    /// reusing its randomness is a catastrophic failure.
    ///
    /// Members that produce two values concatenate them, since BN has no tuple:
    /// `public || private` for key generation and `ciphertext || sharedSecret`
    /// for encapsulation. Both parts have sizes fixed by the standard, so
    /// `Slice` splits them unambiguously.
    fn post_quantum(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let rejected = |what: &str| {
            Ok(Value::Error {
                code: 1,
                message: format!("BNCrypto rejected the {what}").into(),
            })
        };
        match method {
            "MlKemKeypair" | "MlDsaKeypair" => {
                require_arity(member, arguments, 1, span)?;
                let seed = self.bytes_at(arguments, 0, member, span)?;
                let pair = if method.starts_with("MlKem") {
                    bn_rt::crypto::ml_kem_keypair(&seed)
                } else {
                    bn_rt::crypto::ml_dsa_keypair(&seed)
                };
                match pair {
                    Some((mut public_key, private_key)) => {
                        public_key.extend_from_slice(&private_key);
                        Ok(self.insert(public_key))
                    }
                    None => rejected("seed length"),
                }
            }
            "MlKemEncapsulate" => {
                require_arity(member, arguments, 1, span)?;
                let public_key = self.bytes_at(arguments, 0, member, span)?;
                match bn_rt::crypto::ml_kem_encapsulate(&public_key) {
                    Some((mut ciphertext, secret)) => {
                        ciphertext.extend_from_slice(&secret);
                        Ok(self.insert(ciphertext))
                    }
                    None => rejected("encapsulation key"),
                }
            }
            _ => self.post_quantum_use(method, member, arguments, span),
        }
    }

    /// The post-quantum members that consume an existing key: decapsulation,
    /// signing and verification.
    fn post_quantum_use(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if method == "MlDsaVerify" {
            require_arity(member, arguments, 3, span)?;
            let public_key = self.bytes_at(arguments, 0, member, span)?;
            let message = self.bytes_at(arguments, 1, member, span)?;
            let signature = self.bytes_at(arguments, 2, member, span)?;
            return Ok(Value::Boolean(bn_rt::crypto::ml_dsa_verify(
                &public_key,
                &message,
                &signature,
            )));
        }
        require_arity(member, arguments, 2, span)?;
        let key = self.bytes_at(arguments, 0, member, span)?;
        let payload = self.bytes_at(arguments, 1, member, span)?;
        let (produced, what) = if method == "MlKemDecapsulate" {
            (
                bn_rt::crypto::ml_kem_decapsulate(&key, &payload),
                "ciphertext",
            )
        } else {
            (bn_rt::crypto::ml_dsa_sign(&key, &payload), "signing key")
        };
        match produced {
            Some(bytes) => Ok(self.insert(bytes)),
            None => Ok(Value::Error {
                code: 1,
                message: format!("BNCrypto rejected the {what}").into(),
            }),
        }
    }

    /// Ed25519 and ECDSA P-256. Both are deterministic, so a signature is a
    /// function of the key and message alone. Verification returns a plain
    /// `BOOLEAN`: an invalid signature is an answer, not a failure.
    fn signature(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let ed25519 = method.starts_with("Ed25519");
        if method.ends_with("Verify") {
            require_arity(member, arguments, 3, span)?;
            let public_key = self.bytes_at(arguments, 0, member, span)?;
            let message = self.bytes_at(arguments, 1, member, span)?;
            let signature = self.bytes_at(arguments, 2, member, span)?;
            return Ok(Value::Boolean(if ed25519 {
                bn_rt::crypto::ed25519_verify(&public_key, &message, &signature)
            } else {
                bn_rt::crypto::p256_verify(&public_key, &message, &signature)
            }));
        }
        let key = self.bytes_at(arguments, 0, member, span)?;
        let produced = if method.ends_with("PublicKey") {
            require_arity(member, arguments, 1, span)?;
            if ed25519 {
                bn_rt::crypto::ed25519_public_key(&key)
            } else {
                bn_rt::crypto::p256_public_key(&key)
            }
        } else {
            require_arity(member, arguments, 2, span)?;
            let message = self.bytes_at(arguments, 1, member, span)?;
            if ed25519 {
                bn_rt::crypto::ed25519_sign(&key, &message)
            } else {
                bn_rt::crypto::p256_sign(&key, &message)
            }
        };
        match produced {
            Some(bytes) => Ok(self.insert(bytes)),
            None => Ok(Value::Error {
                code: 1,
                message: "BNCrypto rejected the key material".into(),
            }),
        }
    }

    /// HMAC-SHA-256 and Argon2id. Verification is constant-time; Argon2id
    /// rejects out-of-range parameters rather than weakening them.
    fn mac_or_kdf(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "HmacSha256" => {
                require_arity(member, arguments, 2, span)?;
                let key = self.bytes_at(arguments, 0, member, span)?;
                let data = self.bytes_at(arguments, 1, member, span)?;
                Ok(self.insert(bn_rt::crypto::hmac_sha256(&key, &data)))
            }
            "VerifyHmacSha256" => {
                require_arity(member, arguments, 3, span)?;
                let key = self.bytes_at(arguments, 0, member, span)?;
                let data = self.bytes_at(arguments, 1, member, span)?;
                let tag = self.bytes_at(arguments, 2, member, span)?;
                Ok(Value::Boolean(bn_rt::crypto::hmac_verify(
                    &key, &data, &tag,
                )))
            }
            _ => {
                require_arity(member, arguments, 5, span)?;
                let password = self.bytes_at(arguments, 0, member, span)?;
                let salt = self.bytes_at(arguments, 1, member, span)?;
                let mut costs = [0_u32; 3];
                for (slot, argument) in costs.iter_mut().zip(&arguments[2..5]) {
                    let Value::Integer(value, _) = argument else {
                        return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                    };
                    let Ok(value) = u32::try_from(*value) else {
                        return Err(type_mismatch(
                            "a non-negative INTEGER",
                            "out-of-range value",
                            member,
                            span,
                        ));
                    };
                    *slot = value;
                }
                match bn_rt::crypto::argon2id(
                    &password,
                    &salt,
                    bn_rt::crypto::Argon2Params {
                        secret: &[],
                        associated_data: &[],
                        memory_kib: costs[0],
                        iterations: costs[1],
                        parallelism: costs[2],
                        tag_length: 32,
                    },
                ) {
                    Some(tag) => Ok(self.insert(tag)),
                    None => Ok(Value::Error {
                        code: 1,
                        message: "BNCrypto.Argon2id rejected the cost parameters".into(),
                    }),
                }
            }
        }
    }

    /// Digests, buffer construction and buffer inspection — the members that do
    /// not involve a key.
    fn digest_or_buffer(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "SHA256" | "SHA512" => {
                require_arity(member, arguments, 1, span)?;
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
                require_arity(member, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                Ok(self.insert(text.as_bytes().to_vec()))
            }
            "FromHex" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_rt::crypto::decode_hex(text.as_ref()) {
                    Some(bytes) => Ok(self.insert(bytes)),
                    None => Ok(Value::Error {
                        code: 1,
                        message: "BNCrypto.FromHex expects an even-length hexadecimal string"
                            .into(),
                    }),
                }
            }
            "Length" => {
                require_arity(member, arguments, 1, span)?;
                let &Value::CryptoBytes(handle) = &arguments[0] else {
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
                require_arity(member, arguments, 1, span)?;
                let &Value::CryptoBytes(handle) = &arguments[0] else {
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
            _ => {
                require_arity(member, arguments, 3, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let mut bounds = [0_usize; 2];
                for (slot, argument) in bounds.iter_mut().zip(&arguments[1..3]) {
                    let Value::Integer(value, _) = argument else {
                        return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                    };
                    let Ok(value) = usize::try_from(*value) else {
                        return Err(type_mismatch(
                            "a non-negative INTEGER",
                            "out-of-range value",
                            member,
                            span,
                        ));
                    };
                    *slot = value;
                }
                match bounds[0]
                    .checked_add(bounds[1])
                    .filter(|end| *end <= bytes.len())
                {
                    Some(end) => Ok(self.insert(bytes[bounds[0]..end].to_vec())),
                    // Out of range is an Error, never a short buffer that could
                    // pass for a key.
                    None => Ok(Value::Error {
                        code: 1,
                        message: "BNCrypto.Slice range is outside the buffer".into(),
                    }),
                }
            }
        }
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
            "SHA256" | "SHA512" | "FromText" | "FromHex" | "Length" | "ToHex" | "Slice" => {
                self.digest_or_buffer(method, member, &arguments, span)
            }
            "SealAesGcm" | "SealChaCha20" | "OpenAesGcm" | "OpenChaCha20" => {
                self.aead(method, member, &arguments, span)
            }
            "HmacSha256" | "VerifyHmacSha256" | "Argon2id" => {
                self.mac_or_kdf(method, member, &arguments, span)
            }
            "Ed25519PublicKey" | "Ed25519Sign" | "Ed25519Verify" | "EcdsaP256PublicKey"
            | "EcdsaP256Sign" | "EcdsaP256Verify" => {
                self.signature(method, member, &arguments, span)
            }
            "MlKemKeypair" | "MlKemEncapsulate" | "MlKemDecapsulate" | "MlDsaKeypair"
            | "MlDsaSign" | "MlDsaVerify" => self.post_quantum(method, member, &arguments, span),
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
