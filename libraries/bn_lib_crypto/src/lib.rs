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
use zeroize::Zeroize;

use bn_diag::Diagnostic;
use bn_source::Span;
use bn_stdlib::crypto_error::CryptoFailure;
use bn_types::IntegerType;
use bn_value::{Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    name_not_found, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};

pub const NAME: &str = "BNCrypto";

#[derive(Debug)]
struct KeyEntry {
    algorithm: i32,
    material: zeroize::Zeroizing<Vec<u8>>,
    zeroized: bool,
}

/// Interpreter-side owner of `BNCrypto.Bytes` and `BNCrypto.Key` handles. Compiled programs use
/// the parallel tables in `bn_stdlib::crypto_abi`; neither observes the other, and both
/// hand out opaque `u64` handles.
#[derive(Debug, Default)]
pub struct CryptoProvider {
    buffers: HashMap<u64, Vec<u8>>,
    holders: HashMap<u64, usize>,
    keys: HashMap<u64, KeyEntry>,
    key_holders: HashMap<u64, usize>,
    next: u64,
}

impl CryptoProvider {
    fn insert(&mut self, bytes: Vec<u8>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.buffers.insert(handle, bytes);
        self.holders.insert(handle, 1);
        Value::CryptoBytes(handle)
    }

    fn insert_key(&mut self, algorithm: i32, material: zeroize::Zeroizing<Vec<u8>>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.keys.insert(
            handle,
            KeyEntry {
                algorithm,
                material,
                zeroized: false,
            },
        );
        self.key_holders.insert(handle, 1);
        Value::CryptoKey(handle)
    }

    /// Reads the `Key` operand at `index`, or reports the type mismatch.
    fn key_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<zeroize::Zeroizing<Vec<u8>>, Diagnostic> {
        match arguments.get(index) {
            Some(Value::CryptoKey(handle)) => {
                let Some(entry) = self.keys.get(handle) else {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Key was already released",
                        span,
                    ));
                };
                if entry.zeroized {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "key material has already been zeroized",
                        span,
                    ));
                }
                Ok(entry.material.clone())
            }
            Some(Value::CryptoBytes(handle)) => {
                Ok(zeroize::Zeroizing::new(self.buffer(*handle, span)?.clone()))
            }
            _ => Err(type_mismatch(
                "BNCrypto.Key",
                "non-BNCrypto.Key value",
                member,
                span,
            )),
        }
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
        match method {
            "MlKemKeypair" | "MlDsaKeypair" => {
                require_arity(member, arguments, 1, span)?;
                let seed = self.key_at(arguments, 0, member, span)?;
                let pair = if method.starts_with("MlKem") {
                    bn_stdlib::crypto::ml_kem_keypair(&seed)
                } else {
                    bn_stdlib::crypto::ml_dsa_keypair(&seed)
                };
                match pair {
                    Some((mut public_key, private_key)) => {
                        public_key.extend_from_slice(&private_key);
                        Ok(self.insert(public_key))
                    }
                    None => failure(&CryptoFailure::Seed(if method.starts_with("MlKem") {
                        "BNCrypto.MlKemKeypair"
                    } else {
                        "BNCrypto.MlDsaKeypair"
                    })),
                }
            }
            "MlKemEncapsulate" => {
                require_arity(member, arguments, 1, span)?;
                let public_key = self.bytes_at(arguments, 0, member, span)?;
                match bn_stdlib::crypto::ml_kem_encapsulate(&public_key) {
                    Some((mut ciphertext, secret)) => {
                        ciphertext.extend_from_slice(&secret);
                        Ok(self.insert(ciphertext))
                    }
                    None => failure(&CryptoFailure::EncapsulationKey),
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
            return Ok(Value::Boolean(bn_stdlib::crypto::ml_dsa_verify(
                &public_key,
                &message,
                &signature,
            )));
        }
        require_arity(member, arguments, 2, span)?;
        let key = self.key_at(arguments, 0, member, span)?;
        let payload = self.bytes_at(arguments, 1, member, span)?;
        let (produced, rejected) = if method == "MlKemDecapsulate" {
            (
                bn_stdlib::crypto::ml_kem_decapsulate(&key, &payload),
                CryptoFailure::Decapsulate,
            )
        } else {
            (
                bn_stdlib::crypto::ml_dsa_sign(&key, &payload),
                CryptoFailure::SigningKey,
            )
        };
        match produced {
            Some(bytes) => Ok(self.insert(bytes)),
            None => failure(&rejected),
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
                bn_stdlib::crypto::ed25519_verify(&public_key, &message, &signature)
            } else {
                bn_stdlib::crypto::p256_verify(&public_key, &message, &signature)
            }));
        }
        let key = self.key_at(arguments, 0, member, span)?;
        let produced = if method.ends_with("PublicKey") {
            require_arity(member, arguments, 1, span)?;
            if ed25519 {
                bn_stdlib::crypto::ed25519_public_key(&key)
            } else {
                bn_stdlib::crypto::p256_public_key(&key)
            }
        } else {
            require_arity(member, arguments, 2, span)?;
            let message = self.bytes_at(arguments, 1, member, span)?;
            if ed25519 {
                bn_stdlib::crypto::ed25519_sign(&key, &message)
            } else {
                bn_stdlib::crypto::p256_sign(&key, &message)
            }
        };
        match produced {
            Some(bytes) => Ok(self.insert(bytes)),
            None => failure(&CryptoFailure::Key(
                match (ed25519, method.ends_with("Sign")) {
                    (true, true) => "BNCrypto.Ed25519Sign",
                    (true, false) => "BNCrypto.Ed25519PublicKey",
                    (false, true) => "BNCrypto.EcdsaP256Sign",
                    (false, false) => "BNCrypto.EcdsaP256PublicKey",
                },
            )),
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
                let key = self.key_at(arguments, 0, member, span)?;
                let data = self.bytes_at(arguments, 1, member, span)?;
                Ok(self.insert(bn_stdlib::crypto::hmac_sha256(&key, &data)))
            }
            "VerifyHmacSha256" => {
                require_arity(member, arguments, 3, span)?;
                let key = self.key_at(arguments, 0, member, span)?;
                let data = self.bytes_at(arguments, 1, member, span)?;
                let tag = self.bytes_at(arguments, 2, member, span)?;
                Ok(Value::Boolean(bn_stdlib::crypto::hmac_verify(
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
                match bn_stdlib::crypto::argon2id(
                    &password,
                    &salt,
                    bn_stdlib::crypto::Argon2Params {
                        secret: &[],
                        associated_data: &[],
                        memory_kib: costs[0],
                        iterations: costs[1],
                        parallelism: costs[2],
                        tag_length: 32,
                    },
                ) {
                    Some(tag) => Ok(self.insert(tag)),
                    None => failure(&CryptoFailure::Argon2id),
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
                    bn_stdlib::crypto::sha256_hex(data.as_bytes())
                } else {
                    bn_stdlib::crypto::sha512_hex(data.as_bytes())
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
                match bn_stdlib::crypto::decode_hex(text.as_ref()) {
                    Some(bytes) => Ok(self.insert(bytes)),
                    None => failure(&CryptoFailure::Hex(text.to_string())),
                }
            }
            "Length" => {
                require_arity(member, arguments, 1, span)?;
                match &arguments[0] {
                    Value::CryptoBytes(handle) => {
                        let length = self.buffer(*handle, span)?.len();
                        Ok(Value::Integer(
                            i128::try_from(length).unwrap_or(i128::MAX),
                            IntegerType::Int32,
                        ))
                    }
                    Value::CryptoKey(handle) => {
                        let entry = self.keys.get(handle).ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::USE_AFTER_RELEASE,
                                "BNCrypto.Key is invalid",
                                span,
                            )
                        })?;
                        let length = if entry.zeroized { 0 } else { entry.material.len() };
                        Ok(Value::Integer(
                            i128::try_from(length).unwrap_or(i128::MAX),
                            IntegerType::Int32,
                        ))
                    }
                    _ => Err(type_mismatch(
                        "BNCrypto.Bytes or BNCrypto.Key",
                        "non-crypto value",
                        member,
                        span,
                    )),
                }
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
                let hex = bn_stdlib::crypto::encode_hex(self.buffer(handle, span)?);
                Ok(Value::String(shared_string(hex.as_str())))
            }
            _ => {
                require_arity(member, arguments, 3, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let mut bounds = [0_i64; 2];
                for (slot, argument) in bounds.iter_mut().zip(&arguments[1..3]) {
                    let Value::Integer(value, _) = argument else {
                        return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                    };
                    *slot = i64::try_from(*value).unwrap_or(if *value < 0 {
                        i64::MIN
                    } else {
                        i64::MAX
                    });
                }
                let [start, length] = bounds;
                let range = usize::try_from(start)
                    .ok()
                    .zip(usize::try_from(length).ok())
                    .and_then(|(from, count)| Some((from, from.checked_add(count)?)))
                    .filter(|(_, end)| *end <= bytes.len());
                match range {
                    Some((from, end)) => Ok(self.insert(bytes[from..end].to_vec())),
                    // Out of range is an Error, never a short buffer that could
                    // pass for a key.
                    None => failure(&CryptoFailure::Slice {
                        start,
                        length,
                        size: bytes.len(),
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
        let key = self.key_at(arguments, 0, member, span)?;
        let nonce = self.bytes_at(arguments, 1, member, span)?;
        let data = self.bytes_at(arguments, 2, member, span)?;
        let aad = self.bytes_at(arguments, 3, member, span)?;
        let algorithm = if method.ends_with("AesGcm") {
            bn_stdlib::crypto::Aead::Aes256Gcm
        } else {
            bn_stdlib::crypto::Aead::ChaCha20Poly1305
        };
        let sealing = method.starts_with("Seal");
        let result = if sealing {
            bn_stdlib::crypto::seal(algorithm, &key, &nonce, &data, &aad)
        } else {
            bn_stdlib::crypto::open(algorithm, &key, &nonce, &data, &aad)
        };
        match result {
            Some(bytes) => Ok(self.insert(bytes)),
            None => failure(&if sealing {
                CryptoFailure::Seal(algorithm)
            } else {
                CryptoFailure::Open(algorithm)
            }),
        }
    }

    fn key_or_entropy(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "RandomBytes" => {
                require_arity(member, arguments, 1, span)?;
                let Value::Integer(length, _) = &arguments[0] else {
                    return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                };
                let Ok(len) = usize::try_from(*length) else {
                    return Err(type_mismatch(
                        "a non-negative INTEGER",
                        "out-of-range value",
                        member,
                        span,
                    ));
                };
                match bn_stdlib::crypto::random_bytes(len) {
                    Ok(bytes) => Ok(self.insert(bytes)),
                    Err(err) => failure(&err),
                }
            }
            "GenerateNonce" => {
                require_arity(member, arguments, 1, span)?;
                let Value::Integer(algorithm, _) = &arguments[0] else {
                    return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                };
                let Ok(alg) = i32::try_from(*algorithm) else {
                    return Err(type_mismatch(
                        "an INTEGER algorithm code",
                        "out-of-range value",
                        member,
                        span,
                    ));
                };
                match bn_stdlib::crypto::generate_nonce(alg) {
                    Ok(nonce) => Ok(self.insert(nonce)),
                    Err(err) => failure(&err),
                }
            }
            "GenerateKey" => {
                require_arity(member, arguments, 1, span)?;
                let Value::Integer(algorithm, _) = &arguments[0] else {
                    return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                };
                let Ok(alg) = i32::try_from(*algorithm) else {
                    return Err(type_mismatch(
                        "an INTEGER algorithm code",
                        "out-of-range value",
                        member,
                        span,
                    ));
                };
                match bn_stdlib::crypto::generate_key_material(alg) {
                    Ok(material) => Ok(self.insert_key(alg, material)),
                    Err(err) => failure(&err),
                }
            }
            "ImportKey" => {
                require_arity(member, arguments, 2, span)?;
                let raw_bytes = self.bytes_at(arguments, 0, member, span)?;
                let Value::Integer(algorithm, _) = &arguments[1] else {
                    return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                };
                let Ok(alg) = i32::try_from(*algorithm) else {
                    return Err(type_mismatch(
                        "an INTEGER algorithm code",
                        "out-of-range value",
                        member,
                        span,
                    ));
                };
                let Some(expected_len) = bn_stdlib::crypto::required_key_length(alg) else {
                    return failure(&CryptoFailure::Algorithm(alg));
                };
                if raw_bytes.len() != expected_len {
                    return failure(&CryptoFailure::KeyLength {
                        expected: expected_len,
                        actual: raw_bytes.len(),
                    });
                }
                let material = zeroize::Zeroizing::new(raw_bytes);
                Ok(self.insert_key(alg, material))
            }
            "Algorithm" => {
                require_arity(member, arguments, 1, span)?;
                let Value::CryptoKey(handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.Key",
                        "non-BNCrypto.Key value",
                        member,
                        span,
                    ));
                };
                let entry = self.keys.get(handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Key is invalid",
                        span,
                    )
                })?;
                Ok(Value::Integer(
                    i128::from(entry.algorithm),
                    IntegerType::Int32,
                ))
            }
            "Zeroize" => {
                require_arity(member, arguments, 1, span)?;
                let Value::CryptoKey(handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.Key",
                        "non-BNCrypto.Key value",
                        member,
                        span,
                    ));
                };
                let entry = self.keys.get_mut(handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Key is invalid",
                        span,
                    )
                })?;
                entry.material.zeroize();
                entry.zeroized = true;
                Ok(Value::Null)
            }
            _ => Err(name_not_found(method, "BNCrypto function", span)),
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
            "RandomBytes" | "GenerateNonce" | "GenerateKey" | "ImportKey" | "Algorithm"
            | "Zeroize" => self.key_or_entropy(method, member, &arguments, span),
            "CONSTRUCTOR" => Ok(Value::Null),
            _ => Err(name_not_found(method, "BNCrypto function", span)),
        }
    }

    fn allocate(&mut self, class: &str, _span: Span) -> Option<Result<Value, Diagnostic>> {
        let base = class.rsplit('.').next();
        if base == Some("Bytes") {
            Some(Ok(self.insert(Vec::new())))
        } else if base == Some("Key") {
            Some(Ok(self.insert_key(0, zeroize::Zeroizing::new(Vec::new()))))
        } else {
            None
        }
    }

    fn retain(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        match value {
            Value::CryptoBytes(handle) => Some(match self.holders.get_mut(handle) {
                Some(holders) => {
                    *holders += 1;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("crypto", "msg.bncrypto_bytes_was_already_released"),
                    span,
                )),
            }),
            Value::CryptoKey(handle) => Some(match self.key_holders.get_mut(handle) {
                Some(holders) => {
                    *holders += 1;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNCrypto.Key was already released",
                    span,
                )),
            }),
            _ => None,
        }
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        match value {
            Value::CryptoBytes(handle) => Some(if let Some(holders) = self.holders.get_mut(handle) {
                *holders -= 1;
                if *holders == 0 {
                    self.holders.remove(handle);
                    self.buffers.remove(handle);
                }
                Ok(())
            } else {
                Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("crypto", "msg.bncrypto_bytes_was_already_released"),
                    span,
                ))
            }),
            Value::CryptoKey(handle) => Some(if let Some(holders) = self.key_holders.get_mut(handle) {
                *holders -= 1;
                if *holders == 0 {
                    self.key_holders.remove(handle);
                    if let Some(mut entry) = self.keys.remove(handle) {
                        entry.material.zeroize();
                    }
                }
                Ok(())
            } else {
                Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNCrypto.Key was already released",
                    span,
                ))
            }),
            _ => None,
        }
    }
}

/// A `BNCrypto` `Error` value, as the native ABI records it.
#[allow(clippy::unnecessary_wraps)] // Every call site returns it as its result.
fn failure(failure: &CryptoFailure) -> Result<Value, Diagnostic> {
    Ok(Value::error_report(
        failure.code(),
        failure.operation(),
        failure.message(),
        failure.cause(),
    ))
}

#[cfg(test)]
mod tests {
    /// `Crypto.*` codes in `modules/bn/BNCrypto.bn` are the ones both
    /// backends put in `Error.Code`.
    #[test]
    fn module_constants_match_the_runtime_codes() {
        let module = include_str!("../../../modules/bn/BNCrypto.bn");
        for (name, value) in bn_types::error_codes::crypto::ALL {
            let line = format!("EXPORT CONST {name} AS INTEGER = {value}");
            assert!(module.contains(&line), "BNCrypto.bn lacks `{line}`");
        }
    }
}
