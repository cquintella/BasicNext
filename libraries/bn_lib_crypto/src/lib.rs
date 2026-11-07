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
use bn_value::{Handle, Value, shared_string};

use bn_interp::provider::{CoreContext, Provider};
use bn_interp::{
    name_not_found, require_arity_pub as require_arity, runtime_error_pub as runtime_error,
    type_mismatch,
};

pub const NAME: &str = "BNCrypto";

use bn_stdlib::crypto::KeyEntry;
use bn_stdlib::crypto_abi::{PrivateKeyEntry, PublicKeyEntry, SecretKeyEntry};
use bn_stdlib::crypto_policy::CryptoPolicy;
use bn_value::RecordValue;

/// Interpreter-side owner of `BNCrypto.Bytes` and `BNCrypto.Key` handles. Compiled programs use
/// the parallel tables in `bn_stdlib::crypto_abi`; neither observes the other, and both
/// hand out opaque `u64` handles.
#[derive(Debug, Default)]
pub struct CryptoProvider {
    buffers: HashMap<u64, zeroize::Zeroizing<Vec<u8>>>,
    holders: HashMap<u64, usize>,
    keys: HashMap<u64, KeyEntry>,
    key_holders: HashMap<u64, usize>,
    secrets: HashMap<u64, zeroize::Zeroizing<Vec<u8>>>,
    secret_holders: HashMap<u64, usize>,
    secret_keys: HashMap<u64, SecretKeyEntry>,
    secret_key_holders: HashMap<u64, usize>,
    public_keys: HashMap<u64, PublicKeyEntry>,
    public_key_holders: HashMap<u64, usize>,
    private_keys: HashMap<u64, PrivateKeyEntry>,
    private_key_holders: HashMap<u64, usize>,
    hashers: HashMap<Handle, i64>,
    mac_streams: HashMap<Handle, i64>,
    next: u64,
}

impl CryptoProvider {
    fn insert(&mut self, bytes: Vec<u8>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.buffers.insert(handle, zeroize::Zeroizing::new(bytes));
        self.holders.insert(handle, 1);
        Value::CryptoBytes(handle)
    }

    fn insert_secret(&mut self, bytes: Vec<u8>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.secrets.insert(handle, zeroize::Zeroizing::new(bytes));
        self.secret_holders.insert(handle, 1);
        Value::CryptoSecret(handle)
    }

    fn insert_secret_key(&mut self, entry: SecretKeyEntry) -> Value {
        self.next += 1;
        let handle = self.next;
        self.secret_keys.insert(handle, entry);
        self.secret_key_holders.insert(handle, 1);
        Value::CryptoSecretKey(handle)
    }

    fn insert_public_key(&mut self, entry: PublicKeyEntry) -> Value {
        self.next += 1;
        let handle = self.next;
        self.public_keys.insert(handle, entry);
        self.public_key_holders.insert(handle, 1);
        Value::CryptoKey(handle)
    }

    fn insert_private_key(&mut self, entry: PrivateKeyEntry) -> Value {
        self.next += 1;
        let handle = self.next;
        self.private_keys.insert(handle, entry);
        self.private_key_holders.insert(handle, 1);
        Value::CryptoKey(handle)
    }

    fn insert_key(&mut self, algorithm: i32, material: zeroize::Zeroizing<Vec<u8>>) -> Value {
        self.next += 1;
        let handle = self.next;
        self.keys.insert(handle, KeyEntry::new(algorithm, material));
        self.key_holders.insert(handle, 1);
        Value::CryptoKey(handle)
    }

    /// Reads the `Key` operand at `index`. A released key or a non-`Key`
    /// operand is a diagnostic; a key erased with `Zeroize()` is the
    /// `INVALID_ARGUMENT` failure of `bncrypto.md`, returned as an `Error`.
    fn key_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Result<zeroize::Zeroizing<Vec<u8>>, CryptoFailure>, Diagnostic> {
        match arguments.get(index) {
            Some(Value::CryptoKey(handle)) => {
                let Some(entry) = self.keys.get(handle) else {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                        span,
                    ));
                };
                if entry.zeroized {
                    return Ok(Err(CryptoFailure::ZeroizedKey));
                }
                Ok(Ok(entry.material.clone()))
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
        Ok(self.buffer(*handle, span)?.to_vec())
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
                let seed = match self.key_at(arguments, 0, member, span)? {
                    Ok(material) => material,
                    Err(failed) => return failure(&failed),
                };
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
        let key = match self.key_at(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(failed) => return failure(&failed),
        };
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
        let key = match self.key_at(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(failed) => return failure(&failed),
        };
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
                let key = match self.key_at(arguments, 0, member, span)? {
                    Ok(material) => material,
                    Err(failed) => return failure(&failed),
                };
                let data = self.bytes_at(arguments, 1, member, span)?;
                Ok(self.insert(bn_stdlib::crypto::hmac_sha256(&key, &data)))
            }
            "VerifyHmacSha256" => {
                require_arity(member, arguments, 3, span)?;
                let key = match self.key_at(arguments, 0, member, span)? {
                    Ok(material) => material,
                    Err(failed) => return failure(&failed),
                };
                let data = self.bytes_at(arguments, 1, member, span)?;
                let tag = self.bytes_at(arguments, 2, member, span)?;
                Ok(Value::Boolean(bn_stdlib::crypto::hmac_verify(
                    &key, &data, &tag,
                )))
            }
            _ => {
                require_arity(member, arguments, 5, span)?;
                let password = zeroize::Zeroizing::new(self.bytes_at(arguments, 0, member, span)?);
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
            "Length" => self.length_of(arguments, member, span),
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
            "ToBytes" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.secret_at(arguments, 0, member, span)?;
                Ok(self.insert(bytes))
            }
            "ConstantTimeEquals" => self.constant_time_equals(arguments, member, span),
            "NewSecret" | "SecretFromText" => self.new_secret_from_text(arguments, member, span),
            _ => self.slice(member, arguments, span),
        }
    }

    fn length_of(
        &self,
        arguments: &[Value],
        member: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        match &arguments[0] {
            Value::CryptoBytes(handle) => {
                let length = self.buffer(*handle, span)?.len();
                Ok(Value::Integer(
                    i128::try_from(length).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            Value::CryptoSecret(handle) => {
                let length = self.secret(*handle, span)?.len();
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
                let length = if entry.zeroized {
                    0
                } else {
                    entry.material.len()
                };
                Ok(Value::Integer(
                    i128::try_from(length).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            Value::CryptoSecretKey(handle) => {
                let entry = self.secret_keys.get(handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                let length = if entry.zeroized {
                    0
                } else {
                    entry.material.len()
                };
                Ok(Value::Integer(
                    i128::try_from(length).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            _ => Err(type_mismatch(
                "BNCrypto.Bytes, BNCrypto.Secret, BNCrypto.Key or BNCrypto.SecretKey",
                "non-crypto value",
                member,
                span,
            )),
        }
    }

    fn constant_time_equals(
        &self,
        arguments: &[Value],
        member: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let a = match &arguments[0] {
            Value::CryptoBytes(h) => self.buffer(*h, span)?.to_vec(),
            Value::CryptoSecret(h) => self.secret(*h, span)?.to_vec(),
            _ => {
                return Err(type_mismatch(
                    "BNCrypto.Bytes or BNCrypto.Secret",
                    "non-crypto value",
                    member,
                    span,
                ));
            }
        };
        let b = match &arguments[1] {
            Value::CryptoBytes(h) => self.buffer(*h, span)?.to_vec(),
            Value::CryptoSecret(h) => self.secret(*h, span)?.to_vec(),
            _ => {
                return Err(type_mismatch(
                    "BNCrypto.Bytes or BNCrypto.Secret",
                    "non-crypto value",
                    member,
                    span,
                ));
            }
        };
        Ok(Value::Boolean(bn_stdlib::crypto::constant_time_equals(
            &a, &b,
        )))
    }

    fn new_secret_from_text(
        &mut self,
        arguments: &[Value],
        member: &str,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let Value::String(text) = &arguments[0] else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        Ok(self.insert_secret(text.as_bytes().to_vec()))
    }

    /// `Slice(start, length)` of a `Bytes`; out of range is an `Error`.
    fn slice(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 3, span)?;
        let bytes = self.bytes_at(arguments, 0, member, span)?;
        let mut bounds = [0_i64; 2];
        for (slot, argument) in bounds.iter_mut().zip(&arguments[1..3]) {
            let Value::Integer(value, _) = argument else {
                return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
            };
            *slot = i64::try_from(*value).unwrap_or(if *value < 0 { i64::MIN } else { i64::MAX });
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
        let key = match self.key_at(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(failed) => return failure(&failed),
        };
        let nonce = zeroize::Zeroizing::new(self.bytes_at(arguments, 1, member, span)?);
        let data = zeroize::Zeroizing::new(self.bytes_at(arguments, 2, member, span)?);
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

    /// Entropy and key creation: `RandomBytes`, `GenerateNonce`, `GenerateKey`, `ImportKey`.
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
                match bn_stdlib::crypto::random_bytes(*length) {
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
            _ => self.key_inspection(method, member, arguments, span),
        }
    }

    /// `Key` inspection and erasure: `Algorithm` and `Zeroize`.
    fn key_inspection(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "Algorithm" => {
                require_arity(member, arguments, 1, span)?;
                match &arguments[0] {
                    Value::CryptoKey(handle) => {
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
                    Value::CryptoSecretKey(handle) => {
                        let entry = self.secret_keys.get(handle).ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::USE_AFTER_RELEASE,
                                "BNCrypto.SecretKey is invalid",
                                span,
                            )
                        })?;
                        Ok(Value::String(shared_string(entry.alg.as_str())))
                    }
                    _ => Err(type_mismatch(
                        "BNCrypto.Key or BNCrypto.SecretKey",
                        "non-key value",
                        member,
                        span,
                    )),
                }
            }
            "Zeroize" => {
                require_arity(member, arguments, 1, span)?;
                match &arguments[0] {
                    Value::CryptoKey(handle) => {
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
                    Value::CryptoBytes(handle) => {
                        let buf = self.buffers.get_mut(handle).ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::USE_AFTER_RELEASE,
                                "BNCrypto.Bytes is invalid",
                                span,
                            )
                        })?;
                        buf.as_mut_slice().zeroize();
                        Ok(Value::Null)
                    }
                    Value::CryptoSecret(handle) => {
                        let buf = self.secrets.get_mut(handle).ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::USE_AFTER_RELEASE,
                                "BNCrypto.Secret is invalid",
                                span,
                            )
                        })?;
                        buf.as_mut_slice().zeroize();
                        Ok(Value::Null)
                    }
                    Value::CryptoSecretKey(handle) => {
                        let entry = self.secret_keys.get_mut(handle).ok_or_else(|| {
                            runtime_error(
                                bn_diag::DiagId::USE_AFTER_RELEASE,
                                "BNCrypto.SecretKey is invalid",
                                span,
                            )
                        })?;
                        entry.material.as_mut_slice().zeroize();
                        entry.zeroized = true;
                        Ok(Value::Null)
                    }
                    _ => Err(type_mismatch(
                        "BNCrypto.Bytes, BNCrypto.Key, BNCrypto.Secret or BNCrypto.SecretKey",
                        "non-crypto value",
                        member,
                        span,
                    )),
                }
            }
            _ => Err(name_not_found(method, "BNCrypto function", span)),
        }
    }

    /// `HashPasswordWith`, `NeedsRehash`, `DeriveKey` and `DeriveSubkey`: the
    /// password and key-derivation calls that take options or derive a key.
    fn kdf_options_op(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let text_at = |index: usize| match arguments.get(index) {
            Some(Value::String(text)) => Ok(text.to_string()),
            _ => Err(type_mismatch(
                bn_catalog::text("crypto", "type.string.expected"),
                bn_catalog::text("crypto", "type.string.found"),
                member,
                span,
            )),
        };
        match method {
            "HashPasswordWith" | "NeedsRehash" => {
                let record = options_record(
                    arguments,
                    1,
                    bn_catalog::text("crypto", "type.password_options.expected"),
                    member,
                    span,
                )?;
                let field = |index: usize| match record.get(index) {
                    Some(Value::Integer(value, _)) => Ok(i64::try_from(*value).unwrap_or(i64::MIN)),
                    _ => Err(type_mismatch(
                        bn_catalog::text("crypto", "type.integer.expected"),
                        bn_catalog::text("crypto", "type.integer.found"),
                        member,
                        span,
                    )),
                };
                let (memory, iterations, parallelism) = (
                    field(PASSWORD_OPTS_MEMORY_FIELD)?,
                    field(PASSWORD_OPTS_ITERATIONS_FIELD)?,
                    field(PASSWORD_OPTS_PARALLELISM_FIELD)?,
                );
                let first = text_at(0)?;
                if method == "NeedsRehash" {
                    let target = bn_stdlib::crypto_password::PasswordCost::target(
                        memory,
                        iterations,
                        parallelism,
                    );
                    return Ok(Value::Boolean(bn_stdlib::crypto_password::bn_needs_rehash(
                        &first, target,
                    )));
                }
                let operation = bn_catalog::text("crypto", "op.hash_password_with");
                let Some(cost) = bn_stdlib::crypto_password::PasswordCost::from_options(
                    memory,
                    iterations,
                    parallelism,
                ) else {
                    return failure(&CryptoFailure::PasswordCost { operation });
                };
                match bn_stdlib::crypto_password::bn_hash_password_with(&first, cost) {
                    Ok(phc) => Ok(Value::String(shared_string(phc.as_str()))),
                    Err(err) => failure(&err),
                }
            }
            "DeriveKey" => {
                let password = text_at(0)?;
                let salt = self.bytes_at(arguments, 1, member, span)?;
                match bn_stdlib::crypto_password::bn_derive_key(&password, &salt) {
                    Ok(material) => Ok(self.insert_secret_key(
                        bn_stdlib::crypto_password::derived_key_entry(material, "key.alg.argon2id"),
                    )),
                    Err(err) => failure(&err),
                }
            }
            _ => {
                let Value::CryptoSecretKey(handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        bn_catalog::text("crypto", "type.secret_key.expected"),
                        bn_catalog::text("crypto", "type.secret_key.found"),
                        member,
                        span,
                    ));
                };
                let context = text_at(1)?;
                let entry = self.secret_keys.get(handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                        span,
                    )
                })?;
                // A zeroized master is the `INVALID_ARGUMENT` failure, as an `Error`.
                if entry.zeroized {
                    return failure(&CryptoFailure::ZeroizedKey);
                }
                match bn_stdlib::crypto_password::bn_derive_subkey(&entry.material, &context) {
                    Ok(material) => Ok(self.insert_secret_key(
                        bn_stdlib::crypto_password::derived_key_entry(material, "key.alg.hkdf"),
                    )),
                    Err(err) => failure(&err),
                }
            }
        }
    }

    /// `EncryptWith(key, data, opts)` and `DecryptWith(key, sealed, opts)`:
    /// `opts` is an `AeadOptions` record (`alg`, `aad`, `includeKeyId`).
    fn aead_with_op(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 3, span)?;
        let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
            return Err(type_mismatch(
                bn_catalog::text("crypto", "type.secret_key.expected"),
                bn_catalog::text("crypto", "type.secret_key.found"),
                member,
                span,
            ));
        };
        let data = self.bytes_at(arguments, 1, member, span)?;
        let record = options_record(
            arguments,
            2,
            bn_catalog::text("crypto", "type.aead_options.expected"),
            member,
            span,
        )?;
        let alg = options_alg(record, member, span)?;
        let Some(Value::CryptoBytes(aad_handle)) = record.get(AEAD_OPTS_AAD_FIELD) else {
            return Err(type_mismatch(
                bn_catalog::text("crypto", "type.bytes.expected"),
                bn_catalog::text("crypto", "type.bytes.found"),
                member,
                span,
            ));
        };
        let aad = self.buffer(*aad_handle, span)?.to_vec();
        let include_key_id = matches!(
            record.get(AEAD_OPTS_INCLUDE_KEY_ID_FIELD),
            Some(Value::Boolean(true))
        );
        let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                span,
            )
        })?;
        if entry.zeroized {
            return failure(&CryptoFailure::ZeroizedKey);
        }
        let operation = if method == "EncryptWith" {
            bn_catalog::text("crypto", "op.encrypt_with")
        } else {
            bn_catalog::text("crypto", "op.decrypt_with")
        };
        if alg != bn_stdlib::crypto_envelope::AEAD_ALG_XCHACHA20_POLY1305 {
            return failure(&CryptoFailure::UnsupportedAlgorithm { operation });
        }
        let result = if method == "EncryptWith" {
            let options = bn_stdlib::crypto_envelope::EnvelopeOptions {
                aad: &aad,
                include_key_id,
            };
            bn_stdlib::crypto_envelope::bn_encrypt_envelope_with(&entry.material, &data, &options)
        } else {
            bn_stdlib::crypto_envelope::bn_decrypt_envelope_with(&entry.material, &data, &aad)
        };
        match result {
            Ok(bytes) => Ok(self.insert(bytes)),
            Err(err) => failure(&err),
        }
    }

    fn buffer(&self, handle: u64, span: Span) -> Result<&[u8], Diagnostic> {
        self.buffers
            .get(&handle)
            .map(|b| b.as_slice())
            .ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNCrypto.Bytes is invalid",
                    span,
                )
            })
    }

    fn secret(&self, handle: u64, span: Span) -> Result<&[u8], Diagnostic> {
        self.secrets
            .get(&handle)
            .map(|b| b.as_slice())
            .ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNCrypto.Secret is invalid",
                    span,
                )
            })
    }

    fn secret_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Vec<u8>, Diagnostic> {
        let Some(Value::CryptoSecret(handle)) = arguments.get(index) else {
            return Err(type_mismatch(
                "BNCrypto.Secret",
                "non-BNCrypto.Secret value",
                member,
                span,
            ));
        };
        Ok(self.secret(*handle, span)?.to_vec())
    }

    fn secret_key_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<&SecretKeyEntry, Diagnostic> {
        let Some(Value::CryptoSecretKey(handle)) = arguments.get(index) else {
            return Err(type_mismatch(
                "BNCrypto.SecretKey",
                "non-SecretKey value",
                member,
                span,
            ));
        };
        self.secret_keys.get(handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNCrypto.SecretKey is invalid",
                span,
            )
        })
    }

    #[allow(clippy::too_many_lines)]
    fn secret_key_op(
        &mut self,
        fs: &bn_stdlib::FsPolicy,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "NewSecretKey" => {
                let material = match bn_stdlib::crypto::random_bytes(32) {
                    Ok(bytes) => bytes,
                    Err(f) => return failure(&f),
                };
                Ok(self.insert_secret_key(SecretKeyEntry {
                    material: zeroize::Zeroizing::new(material),
                    bits: 256,
                    purpose: "AEAD".into(),
                    alg: "AES256".into(),
                    zeroized: false,
                }))
            }
            "NewSecretKeyWith" => {
                require_arity(member, arguments, 1, span)?;
                let Value::Record { record } = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.KeyOptions",
                        "non-record value",
                        member,
                        span,
                    ));
                };
                let bits = match record.get(0) {
                    Some(Value::Integer(b, _)) => usize::try_from(*b).unwrap_or(256),
                    _ => 256,
                };
                let purpose = match record.get(1) {
                    Some(Value::String(s)) => s.to_string(),
                    _ => "AEAD".into(),
                };
                let alg = match record.get(2) {
                    Some(Value::String(s)) => s.to_string(),
                    _ => "AES256".into(),
                };
                let material = match bn_stdlib::crypto::random_bytes((bits / 8) as i128) {
                    Ok(bytes) => bytes,
                    Err(f) => return failure(&f),
                };
                Ok(self.insert_secret_key(SecretKeyEntry {
                    material: zeroize::Zeroizing::new(material),
                    bits,
                    purpose,
                    alg,
                    zeroized: false,
                }))
            }
            "SecretKeyFromBytes" => {
                require_arity(member, arguments, 2, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let Value::String(purpose) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let bits = bytes.len() * 8;
                Ok(self.insert_secret_key(SecretKeyEntry {
                    material: zeroize::Zeroizing::new(bytes.clone()),
                    bits,
                    purpose: purpose.to_string(),
                    alg: "AES256".into(),
                    zeroized: false,
                }))
            }
            "WrapKey" => {
                require_arity(member, arguments, 2, span)?;
                let kek = self
                    .secret_key_at(arguments, 0, member, span)?
                    .material
                    .to_vec();
                let key_data = self
                    .secret_key_at(arguments, 1, member, span)?
                    .material
                    .to_vec();
                match bn_stdlib::crypto::rfc3394_key_wrap(&kek, &key_data) {
                    Ok(wrapped) => Ok(self.insert(wrapped)),
                    Err(f) => failure(&f),
                }
            }
            "UnwrapKey" => {
                require_arity(member, arguments, 2, span)?;
                let kek = self
                    .secret_key_at(arguments, 0, member, span)?
                    .material
                    .to_vec();
                let wrapped = self.bytes_at(arguments, 1, member, span)?;
                match bn_stdlib::crypto::rfc3394_key_unwrap(&kek, &wrapped) {
                    Ok(unwrapped) => {
                        let bits = unwrapped.len() * 8;
                        Ok(self.insert_secret_key(SecretKeyEntry {
                            material: zeroize::Zeroizing::new(unwrapped),
                            bits,
                            purpose: "AEAD".into(),
                            alg: "AES256".into(),
                            zeroized: false,
                        }))
                    }
                    Err(f) => failure(&f),
                }
            }
            "UnwrapKeyWith" => {
                require_arity(member, arguments, 3, span)?;
                let kek = self
                    .secret_key_at(arguments, 0, member, span)?
                    .material
                    .to_vec();
                let wrapped = self.bytes_at(arguments, 1, member, span)?;
                let Value::Record { record } = &arguments[2] else {
                    return Err(type_mismatch(
                        "Crypto.KeyOptions",
                        "non-record value",
                        member,
                        span,
                    ));
                };
                let bits = match record.get(0) {
                    Some(Value::Integer(b, _)) => usize::try_from(*b).unwrap_or(256),
                    _ => 256,
                };
                let purpose = match record.get(1) {
                    Some(Value::String(s)) => s.to_string(),
                    _ => "AEAD".into(),
                };
                let alg = match record.get(2) {
                    Some(Value::String(s)) => s.to_string(),
                    _ => "AES256".into(),
                };
                match bn_stdlib::crypto::rfc3394_key_unwrap(&kek, &wrapped) {
                    Ok(unwrapped) => Ok(self.insert_secret_key(SecretKeyEntry {
                        material: zeroize::Zeroizing::new(unwrapped),
                        bits,
                        purpose,
                        alg,
                        zeroized: false,
                    })),
                    Err(f) => failure(&f),
                }
            }
            "Id" => {
                require_arity(member, arguments, 1, span)?;
                let entry = self.secret_key_at(arguments, 0, member, span)?;
                let id = bn_stdlib::crypto::compute_secret_key_id(&entry.material);
                Ok(Value::String(shared_string(id.as_str())))
            }
            "Bits" => {
                require_arity(member, arguments, 1, span)?;
                let entry = self.secret_key_at(arguments, 0, member, span)?;
                let bits = if entry.zeroized { 0 } else { entry.bits };
                Ok(Value::Integer(
                    i128::try_from(bits).unwrap_or(i128::MAX),
                    IntegerType::Int32,
                ))
            }
            "Purpose" => {
                require_arity(member, arguments, 1, span)?;
                let entry = self.secret_key_at(arguments, 0, member, span)?;
                Ok(Value::String(shared_string(entry.purpose.as_str())))
            }
            "ExportSecret" => {
                let entry = self.secret_key_at(arguments, 0, member, span)?;
                let fmt = if arguments.len() > 1 {
                    match &arguments[1] {
                        Value::String(s) => s.as_ref(),
                        _ => "RAW",
                    }
                } else {
                    "RAW"
                };
                if fmt == "HEX" {
                    let hex = bn_stdlib::crypto::encode_hex(&entry.material);
                    Ok(self.insert(hex.into_bytes()))
                } else {
                    Ok(self.insert(entry.material.to_vec()))
                }
            }
            "Encrypt" => {
                require_arity(member, arguments, 2, span)?;
                let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.SecretKey",
                        "non-SecretKey value",
                        member,
                        span,
                    ));
                };
                let Value::CryptoBytes(plain_handle) = &arguments[1] else {
                    return Err(type_mismatch(
                        "Crypto.Bytes",
                        "non-Bytes value",
                        member,
                        span,
                    ));
                };
                let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::ZeroizedKey);
                }
                let plain_bytes = self.buffers.get(plain_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Bytes is invalid",
                        span,
                    )
                })?;
                match bn_stdlib::crypto::bn_encrypt_envelope(&entry.material, plain_bytes) {
                    Ok(sealed) => Ok(self.insert(sealed)),
                    Err(f) => failure(&f),
                }
            }
            "Decrypt" => {
                require_arity(member, arguments, 2, span)?;
                let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.SecretKey",
                        "non-SecretKey value",
                        member,
                        span,
                    ));
                };
                let Value::CryptoBytes(sealed_handle) = &arguments[1] else {
                    return Err(type_mismatch(
                        "Crypto.Bytes",
                        "non-Bytes value",
                        member,
                        span,
                    ));
                };
                let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::ZeroizedKey);
                }
                let sealed_bytes = self.buffers.get(sealed_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Bytes is invalid",
                        span,
                    )
                })?;
                match bn_stdlib::crypto::bn_decrypt_envelope(&entry.material, sealed_bytes) {
                    Ok(plain) => Ok(self.insert(plain)),
                    Err(f) => failure(&f),
                }
            }
            "EncryptText" => {
                require_arity(member, arguments, 2, span)?;
                let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.SecretKey",
                        "non-SecretKey value",
                        member,
                        span,
                    ));
                };
                let Value::String(text) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::ZeroizedKey);
                }
                match bn_stdlib::crypto::bn_encrypt_envelope(&entry.material, text.as_bytes()) {
                    Ok(sealed) => {
                        let token = bn_stdlib::crypto::encode_base64url_unpadded(&sealed);
                        Ok(Value::String(shared_string(token.as_str())))
                    }
                    Err(f) => failure(&f),
                }
            }
            "DecryptText" => {
                require_arity(member, arguments, 2, span)?;
                let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.SecretKey",
                        "non-SecretKey value",
                        member,
                        span,
                    ));
                };
                let Value::String(token) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::ZeroizedKey);
                }
                let Some(sealed_bytes) = bn_stdlib::crypto::decode_base64url(token) else {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::EnvelopeFormat);
                };
                match bn_stdlib::crypto::bn_decrypt_envelope(&entry.material, &sealed_bytes) {
                    Ok(plain) => match String::from_utf8(plain) {
                        Ok(recovered) => Ok(Value::String(shared_string(recovered.as_str()))),
                        Err(_) => failure(&bn_stdlib::crypto_error::CryptoFailure::EnvelopeFormat),
                    },
                    Err(f) => failure(&f),
                }
            }
            "EncryptFile" => {
                require_arity(member, arguments, 3, span)?;
                let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.SecretKey",
                        "non-SecretKey value",
                        member,
                        span,
                    ));
                };
                let Value::String(in_path) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let Value::String(out_path) = &arguments[2] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::ZeroizedKey);
                }
                match bn_stdlib::crypto::bn_encrypt_file(
                    fs,
                    &entry.material,
                    std::path::Path::new(in_path.as_ref()),
                    std::path::Path::new(out_path.as_ref()),
                ) {
                    Ok(()) => Ok(Value::Null),
                    Err(f) => failure(&f),
                }
            }
            "DecryptFile" => {
                require_arity(member, arguments, 3, span)?;
                let Value::CryptoSecretKey(key_handle) = &arguments[0] else {
                    return Err(type_mismatch(
                        "Crypto.SecretKey",
                        "non-SecretKey value",
                        member,
                        span,
                    ));
                };
                let Value::String(in_path) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let Value::String(out_path) = &arguments[2] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let entry = self.secret_keys.get(key_handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return failure(&bn_stdlib::crypto_error::CryptoFailure::ZeroizedKey);
                }
                match bn_stdlib::crypto::bn_decrypt_file(
                    fs,
                    &entry.material,
                    std::path::Path::new(in_path.as_ref()),
                    std::path::Path::new(out_path.as_ref()),
                ) {
                    Ok(()) => Ok(Value::Null),
                    Err(f) => failure(&f),
                }
            }
            _ => Err(name_not_found(method, "BNCrypto SecretKey method", span)),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn hash_op(
        &mut self,
        core: &mut dyn CoreContext,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "Hash" => {
                require_arity(member, arguments, 1, span)?;
                let data = self.bytes_at(arguments, 0, member, span)?;
                match bn_stdlib::crypto::bn_hash(&data, bn_stdlib::crypto::HASH_SHA256) {
                    Ok(digest) => Ok(self.insert(digest)),
                    Err(err) => failure(&err),
                }
            }
            "HashText" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let hex = bn_stdlib::crypto::sha256_hex(text.as_bytes());
                Ok(Value::String(shared_string(hex.as_str())))
            }
            "HashFile" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_stdlib::crypto::bn_hash_file(
                    core.host().filesystem(),
                    std::path::Path::new(path.as_ref()),
                    bn_stdlib::crypto::HASH_SHA256,
                ) {
                    Ok(digest) => Ok(self.insert(digest)),
                    Err(err) => failure(&err),
                }
            }
            "HashWith" => {
                require_arity(member, arguments, 2, span)?;
                let data = self.bytes_at(arguments, 0, member, span)?;
                let record = options_record(arguments, 1, "BNCrypto.HashOptions", member, span)?;
                let alg = options_alg(record, member, span)?;
                if let Err(err) = options_policy(record, HASH_OPTS_POLICY_FIELD).admits_hash(alg) {
                    return failure(&err);
                }
                match bn_stdlib::crypto::bn_hash(&data, alg) {
                    Ok(digest) => Ok(self.insert(digest)),
                    Err(err) => failure(&err),
                }
            }
            "New" | "NewWith" if member.contains("Hasher") => {
                require_arity(member, arguments, 1, span)?;
                let alg = if method == "NewWith" {
                    // `Hasher.NewWith(opts)`: algorithm and policy from `HashOptions`.
                    let record =
                        options_record(arguments, 0, "BNCrypto.HashOptions", member, span)?;
                    let alg = options_alg(record, member, span)?;
                    if let Err(err) =
                        options_policy(record, HASH_OPTS_POLICY_FIELD).admits_hash(alg)
                    {
                        return failure(&err);
                    }
                    alg
                } else {
                    let Value::Integer(alg_val, _) = &arguments[0] else {
                        return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                    };
                    i32::try_from(*alg_val).map_err(|_| {
                        type_mismatch(
                            "an INTEGER algorithm code",
                            "out-of-range value",
                            member,
                            span,
                        )
                    })?
                };
                match bn_stdlib::crypto::Hasher::new(alg) {
                    Ok(hasher) => {
                        let hasher_id = bn_stdlib::crypto::insert_hasher(hasher);
                        let object = core.allocate_object("BNCrypto.Hasher", span)?;
                        let Value::Object { handle, .. } = object else {
                            unreachable!("allocate_object returns object");
                        };
                        self.hashers.insert(handle, hasher_id);
                        Ok(object)
                    }
                    Err(err) => failure(&err),
                }
            }
            "Update" => {
                require_arity(member, arguments, 2, span)?;
                let Value::Object { handle, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.Hasher or BNCrypto.MacStream",
                        "non-object receiver",
                        member,
                        span,
                    ));
                };
                if let Some(&stream_id) = self.mac_streams.get(handle) {
                    let data = self.bytes_at(arguments, 1, member, span)?;
                    return match bn_stdlib::crypto::update_mac_stream(stream_id, &data) {
                        Ok(()) => Ok(Value::Null),
                        Err(err) => failure(&err),
                    };
                }
                let Some(&hasher_id) = self.hashers.get(handle) else {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "streaming receiver is invalid",
                        span,
                    ));
                };
                let data = self.bytes_at(arguments, 1, member, span)?;
                match bn_stdlib::crypto::update_hasher(hasher_id, &data) {
                    Ok(()) => Ok(Value::Null),
                    Err(err) => failure(&err),
                }
            }
            "Finish" => {
                require_arity(member, arguments, 1, span)?;
                let Value::Object { handle, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.Hasher or BNCrypto.MacStream",
                        "non-object receiver",
                        member,
                        span,
                    ));
                };
                if let Some(stream_id) = self.mac_streams.remove(handle) {
                    return match bn_stdlib::crypto::finish_mac_stream(stream_id) {
                        Ok(tag) => Ok(self.insert(tag)),
                        Err(err) => failure(&err),
                    };
                }
                let Some(hasher_id) = self.hashers.remove(handle) else {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "streaming receiver is invalid",
                        span,
                    ));
                };
                match bn_stdlib::crypto::finish_hasher(hasher_id) {
                    Ok(digest) => Ok(self.insert(digest)),
                    Err(err) => failure(&err),
                }
            }
            "Verify" => {
                require_arity(member, arguments, 2, span)?;
                let Value::Object { handle, .. } = &arguments[0] else {
                    return Err(type_mismatch(
                        "BNCrypto.MacStream",
                        "non-object receiver",
                        member,
                        span,
                    ));
                };
                let Some(stream_id) = self.mac_streams.remove(handle) else {
                    return Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.MacStream is invalid",
                        span,
                    ));
                };
                let tag = self.bytes_at(arguments, 1, member, span)?;
                let matches = bn_stdlib::crypto::verify_mac_stream(stream_id, &tag);
                Ok(Value::Boolean(matches))
            }
            _ => Err(name_not_found(method, "BNCrypto hash method", span)),
        }
    }

    /// The material of the `SecretKey` or `Key` operand at `index`. A released
    /// key or another operand is a diagnostic; a zeroized key is the
    /// `INVALID_ARGUMENT` failure of `bncrypto.md`, which each caller turns
    /// into its result (an `Error`, or FALSE for `VerifyMac`).
    fn key_bytes_at(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Result<zeroize::Zeroizing<Vec<u8>>, CryptoFailure>, Diagnostic> {
        match arguments.get(index) {
            Some(Value::CryptoSecretKey(handle)) => {
                let entry = self.secret_keys.get(handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return Ok(Err(CryptoFailure::ZeroizedKey));
                }
                Ok(Ok(entry.material.clone()))
            }
            Some(Value::CryptoKey(handle)) => {
                let entry = self.keys.get(handle).ok_or_else(|| {
                    runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Key is invalid",
                        span,
                    )
                })?;
                if entry.zeroized {
                    return Ok(Err(CryptoFailure::ZeroizedKey));
                }
                Ok(Ok(entry.material.clone()))
            }
            _ => Err(type_mismatch(
                "BNCrypto.SecretKey",
                "non-SecretKey value",
                member,
                span,
            )),
        }
    }

    fn mac_op(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "Mac" => {
                require_arity(member, arguments, 2, span)?;
                let key = match self.key_bytes_at(arguments, 0, member, span)? {
                    Ok(key) => key,
                    Err(err) => return failure(&err),
                };
                let data = self.bytes_at(arguments, 1, member, span)?;
                match bn_stdlib::crypto::bn_mac(&key, &data, bn_stdlib::crypto::MAC_HMAC_SHA256) {
                    Ok(tag) => Ok(self.insert(tag)),
                    Err(err) => failure(&err),
                }
            }
            "VerifyMac" => {
                require_arity(member, arguments, 3, span)?;
                let Ok(key) = self.key_bytes_at(arguments, 0, member, span)? else {
                    // A zeroized key authenticates nothing (fail-closed).
                    return Ok(Value::Boolean(false));
                };
                let data = self.bytes_at(arguments, 1, member, span)?;
                let tag = self.bytes_at(arguments, 2, member, span)?;
                let matches = bn_stdlib::crypto::bn_mac_verify(
                    &key,
                    &data,
                    &tag,
                    bn_stdlib::crypto::MAC_HMAC_SHA256,
                );
                Ok(Value::Boolean(matches))
            }
            "MacWith" => {
                require_arity(member, arguments, 3, span)?;
                let key = match self.key_bytes_at(arguments, 0, member, span)? {
                    Ok(key) => key,
                    Err(err) => return failure(&err),
                };
                let data = self.bytes_at(arguments, 1, member, span)?;
                let (alg, policy) = mac_alg_and_policy(arguments.get(2), member, span)?;
                if let Err(err) = policy.admits_mac(alg) {
                    return failure(&err);
                }
                match bn_stdlib::crypto::bn_mac(&key, &data, alg) {
                    Ok(tag) => Ok(self.insert(tag)),
                    Err(err) => failure(&err),
                }
            }
            _ => Err(name_not_found(method, "BNCrypto MAC method", span)),
        }
    }

    fn password_op(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "HashPassword" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(pw) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_stdlib::crypto::bn_hash_password(pw.as_ref()) {
                    Ok(phc) => Ok(Value::String(shared_string(phc.as_str()))),
                    Err(err) => failure(&err),
                }
            }
            "VerifyPassword" => {
                require_arity(member, arguments, 2, span)?;
                let Value::String(pw) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let Value::String(phc) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let ok = bn_stdlib::crypto::bn_verify_password(pw.as_ref(), phc.as_ref());
                Ok(Value::Boolean(ok))
            }
            "Hkdf" => {
                require_arity(member, arguments, 4, span)?;
                let ikm = match self.key_bytes_at(arguments, 0, member, span)? {
                    Ok(key) => key,
                    Err(err) => return failure(&err),
                };
                let salt = self.bytes_at(arguments, 1, member, span)?;
                let info = self.bytes_at(arguments, 2, member, span)?;
                let Value::Integer(len_val, _) = &arguments[3] else {
                    return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
                };
                let Ok(len) = usize::try_from(*len_val) else {
                    return failure(&CryptoFailure::Key("invalid HKDF length"));
                };
                match bn_stdlib::crypto::bn_hkdf_sha256(&ikm, &salt, &info, len) {
                    Ok(derived) => Ok(self.insert_secret_key(SecretKeyEntry {
                        material: zeroize::Zeroizing::new(derived),
                        bits: len * 8,
                        purpose: "DERIVED".into(),
                        alg: "HKDF".into(),
                        zeroized: false,
                    })),
                    Err(err) => failure(&err),
                }
            }
            _ => Err(name_not_found(method, "BNCrypto password/KDF method", span)),
        }
    }

    fn mac_stream_new(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let key = match self.key_bytes_at(arguments, 0, member, span)? {
            Ok(key) => key,
            Err(err) => return failure(&err),
        };
        let (alg, policy) = mac_alg_and_policy(arguments.get(1), member, span)?;
        if let Err(err) = policy.admits_mac(alg) {
            return failure(&err);
        }
        match bn_stdlib::crypto::MacStream::new(&key, alg) {
            Ok(stream) => {
                let stream_id = bn_stdlib::crypto::insert_mac_stream(stream);
                let object = core.allocate_object("BNCrypto.MacStream", span)?;
                let Value::Object { handle, .. } = object else {
                    unreachable!("allocate_object returns object");
                };
                self.mac_streams.insert(handle, stream_id);
                Ok(object)
            }
            Err(err) => failure(&err),
        }
    }

    fn encoding_op(
        &mut self,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "ToText" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                match bn_stdlib::crypto::bn_bytes_to_text(&bytes) {
                    Ok(s) => Ok(Value::String(shared_string(s.as_str()))),
                    Err(err) => failure(&err),
                }
            }
            "ToBase64" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let s = bn_stdlib::crypto::bn_base64_encode(&bytes);
                Ok(Value::String(shared_string(s.as_str())))
            }
            "FromBase64" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_stdlib::crypto::bn_base64_decode(text.as_ref()) {
                    Ok(bytes) => Ok(self.insert(bytes)),
                    Err(err) => failure(&err),
                }
            }
            "ToBase64Url" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let s = bn_stdlib::crypto::bn_base64url_encode(&bytes);
                Ok(Value::String(shared_string(s.as_str())))
            }
            "FromBase64Url" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(text) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_stdlib::crypto::bn_base64url_decode(text.as_ref()) {
                    Ok(bytes) => Ok(self.insert(bytes)),
                    Err(err) => failure(&err),
                }
            }
            "Concat" => {
                require_arity(member, arguments, 2, span)?;
                let a = self.bytes_at(arguments, 0, member, span)?;
                let b = self.bytes_at(arguments, 1, member, span)?;
                let combined = bn_stdlib::crypto::bn_bytes_concat(&a, &b);
                Ok(self.insert(combined))
            }
            "ToPem" => {
                require_arity(member, arguments, 2, span)?;
                let Value::String(label) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let der = self.bytes_at(arguments, 1, member, span)?;
                let s = bn_stdlib::crypto::bn_pem_encode(label.as_ref(), &der);
                Ok(Value::String(shared_string(s.as_str())))
            }
            "FromPem" => {
                require_arity(member, arguments, 1, span)?;
                let Value::String(pem) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                match bn_stdlib::crypto::bn_pem_decode(pem.as_ref()) {
                    Ok(bytes) => Ok(self.insert(bytes)),
                    Err(err) => failure(&err),
                }
            }
            _ => Err(name_not_found(method, "BNCrypto function", span)),
        }
    }

    fn checksum_op(
        &self,
        fs: &bn_stdlib::FsPolicy,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "Crc32" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let val = bn_stdlib::crypto::bn_crc32(&bytes);
                Ok(Value::Integer(i128::from(val), IntegerType::Int64))
            }
            "Crc32c" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let val = bn_stdlib::crypto::bn_crc32c(&bytes);
                Ok(Value::Integer(i128::from(val), IntegerType::Int64))
            }
            "Adler32" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let val = bn_stdlib::crypto::bn_adler32(&bytes);
                Ok(Value::Integer(i128::from(val), IntegerType::Int64))
            }
            "XxHash64" => {
                require_arity(member, arguments, 1, span)?;
                let bytes = self.bytes_at(arguments, 0, member, span)?;
                let val = bn_stdlib::crypto::bn_xxhash64(&bytes);
                Ok(Value::Integer(
                    i128::from(val.cast_signed()),
                    IntegerType::Int64,
                ))
            }
            "VerifyFileChecksum" => {
                require_arity(member, arguments, 2, span)?;
                let Value::String(path) = &arguments[0] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let Value::String(expected) = &arguments[1] else {
                    return Err(type_mismatch("STRING", "non-STRING value", member, span));
                };
                let ok = bn_stdlib::crypto::bn_verify_file_checksum(
                    fs,
                    path.as_ref(),
                    expected.as_ref(),
                );
                Ok(Value::Boolean(ok))
            }
            _ => Err(name_not_found(method, "BNCrypto function", span)),
        }
    }

    fn op_new_keypair_internal(&mut self, alg: i32) -> Result<Value, Diagnostic> {
        match bn_stdlib::crypto::bn_keypair_generate_internal(alg) {
            Ok((pub_h, priv_h)) => {
                let pub_entry =
                    bn_stdlib::crypto::with_public_keys(|keys| keys.get(&pub_h).cloned());
                let priv_entry = bn_stdlib::crypto::with_private_keys(|keys| {
                    keys.get(&priv_h).map(|e| PrivateKeyEntry {
                        seed: e.seed.clone(),
                        spki: e.spki.clone(),
                        raw_public: e.raw_public.clone(),
                        algorithm: e.algorithm,
                        key_id: e.key_id.clone(),
                        zeroized: e.zeroized,
                    })
                });
                if let (Some(pub_e), Some(priv_e)) = (pub_entry, priv_entry) {
                    let pub_val = self.insert_public_key(pub_e);
                    let priv_val = self.insert_private_key(priv_e);
                    bn_stdlib::crypto_abi::bn_rt_crypto_public_key_release(pub_h);
                    bn_stdlib::crypto_abi::bn_rt_crypto_private_key_release(priv_h);
                    Ok(Value::Record {
                        record: RecordValue::new("KeyPair", vec![pub_val, priv_val]),
                    })
                } else {
                    failure(&CryptoFailure::Random("NewKeyPair"))
                }
            }
            Err(f) => failure(&f),
        }
    }

    fn op_new_keypair(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 0, span)?;
        self.op_new_keypair_internal(30)
    }

    fn op_new_keypair_for(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let alg = match arguments.first() {
            Some(Value::Integer(val, _)) => i32::try_from(*val).unwrap_or(0),
            _ => return Err(type_mismatch("INTEGER", "non-integer value", member, span)),
        };
        self.op_new_keypair_internal(alg)
    }

    fn op_public_key(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch(
                "BNCrypto.PrivateKey",
                "non-PrivateKey value",
                member,
                span,
            ));
        };
        let entry = self.private_keys.get(handle).ok_or_else(|| {
            runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "BNCrypto.PrivateKey was already released",
                span,
            )
        })?;
        // Zeroize wipes the seed only; the public key holds no secret and stays
        // derivable (bncrypto.md, `Crypto.PrivateKey`).
        let pub_entry = entry.to_public_entry();
        Ok(self.insert_public_key(pub_entry))
    }

    fn op_key_id(
        &self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch("BNCrypto.Key", "non-Key value", member, span));
        };
        if let Some(entry) = self.public_keys.get(handle) {
            return Ok(Value::String(shared_string(entry.key_id.as_str())));
        }
        if let Some(entry) = self.private_keys.get(handle) {
            return Ok(Value::String(shared_string(entry.key_id.as_str())));
        }
        Err(runtime_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "Key was already released",
            span,
        ))
    }

    fn op_key_algorithm(
        &self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch("BNCrypto.Key", "non-Key value", member, span));
        };
        if let Some(entry) = self.public_keys.get(handle) {
            return Ok(Value::Integer(
                i128::from(entry.algorithm),
                IntegerType::Int32,
            ));
        }
        if let Some(entry) = self.private_keys.get(handle) {
            return Ok(Value::Integer(
                i128::from(entry.algorithm),
                IntegerType::Int32,
            ));
        }
        Err(runtime_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "Key was already released",
            span,
        ))
    }

    fn op_key_zeroize(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch("BNCrypto.Key", "non-Key value", member, span));
        };
        if let Some(entry) = self.private_keys.get_mut(handle) {
            entry.seed.as_mut_slice().zeroize();
            entry.zeroized = true;
            return Ok(Value::Null);
        }
        Err(runtime_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "Key was already released",
            span,
        ))
    }

    /// A private key's seed (wiped when dropped) and its algorithm.
    fn private_material(
        &self,
        arguments: &[Value],
        index: usize,
        member: &str,
        span: Span,
    ) -> Result<Result<PrivateMaterial, CryptoFailure>, Diagnostic> {
        let Some(Value::CryptoKey(handle)) = arguments.get(index) else {
            return Err(type_mismatch(
                "BNCrypto.PrivateKey",
                "non-PrivateKey value",
                member,
                span,
            ));
        };
        let Some(entry) = self.private_keys.get(handle) else {
            return Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                span,
            ));
        };
        if entry.zeroized {
            return Ok(Err(CryptoFailure::ZeroizedKey));
        }
        Ok(Ok((
            zeroize::Zeroizing::new(entry.seed.to_vec()),
            entry.algorithm,
        )))
    }

    fn op_new_keypair_with(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        // `alg` and `policy`; the default `alg` (`Alg.ED25519`) is the record's.
        let record = options_record(arguments, 0, "BNCrypto.KeyPairOptions", member, span)?;
        let alg = options_alg(record, member, span)?;
        if let Err(err) = options_policy(record, KEY_PAIR_OPTS_POLICY_FIELD).admits_key_pair(alg) {
            return failure(&err);
        }
        self.op_new_keypair_internal(alg)
    }

    fn op_sign(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let (seed, algorithm) = match self.private_material(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(err) => return failure(&err),
        };
        let message = self.bytes_at(arguments, 1, member, span)?;
        match bn_stdlib::crypto::bn_sign(&seed, algorithm, &message) {
            Ok(signature) => Ok(self.insert(signature)),
            Err(err) => failure(&err),
        }
    }

    fn op_verify(
        &self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 3, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch(
                "BNCrypto.PublicKey",
                "non-PublicKey value",
                member,
                span,
            ));
        };
        let Some(entry) = self.public_keys.get(handle) else {
            return Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                span,
            ));
        };
        let message = self.bytes_at(arguments, 1, member, span)?;
        let signature = self.bytes_at(arguments, 2, member, span)?;
        Ok(Value::Boolean(bn_stdlib::crypto::bn_verify(
            &entry.raw,
            entry.algorithm,
            &message,
            &signature,
        )))
    }

    fn op_sign_file(
        &mut self,
        fs: &bn_stdlib::FsPolicy,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let (seed, algorithm) = match self.private_material(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(err) => return failure(&err),
        };
        let Value::String(path) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        match bn_stdlib::crypto::bn_sign_file(fs, &seed, algorithm, path.as_ref()) {
            Ok(signature) => Ok(self.insert(signature)),
            Err(err) => failure(&err),
        }
    }

    fn op_verify_file(
        &self,
        fs: &bn_stdlib::FsPolicy,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 3, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch(
                "BNCrypto.PublicKey",
                "non-PublicKey value",
                member,
                span,
            ));
        };
        let Some(entry) = self.public_keys.get(handle) else {
            return Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                span,
            ));
        };
        let Value::String(path) = &arguments[1] else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        let signature = self.bytes_at(arguments, 2, member, span)?;
        Ok(Value::Boolean(bn_stdlib::crypto::bn_verify_file(
            fs,
            &entry.raw,
            entry.algorithm,
            path.as_ref(),
            &signature,
        )))
    }

    fn op_export_pem(
        &self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        if arguments.len() == 1 {
            let Some(Value::CryptoKey(handle)) = arguments.first() else {
                return Err(type_mismatch(
                    "BNCrypto.PublicKey",
                    "non-PublicKey value",
                    member,
                    span,
                ));
            };
            let Some(entry) = self.public_keys.get(handle) else {
                return Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                    span,
                ));
            };
            return Ok(Value::String(shared_string(
                bn_stdlib::crypto::bn_export_public_pem(&entry.spki).as_str(),
            )));
        }
        require_arity(member, arguments, 3, span)?;
        let (seed, algorithm) = match self.private_material(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(err) => return failure(&err),
        };
        let Value::Integer(format, _) = &arguments[1] else {
            return Err(type_mismatch("INT32", "non-INT32 value", member, span));
        };
        let Ok(format) = i32::try_from(*format) else {
            return failure(&bn_stdlib::crypto_error::CryptoFailure::Format {
                operation: bn_stdlib::crypto_sign::EXPORT,
            });
        };
        let Value::String(password) = &arguments[2] else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        match bn_stdlib::crypto::bn_export_private_pem(&seed, algorithm, format, password.as_ref())
        {
            Ok(pem) => Ok(Value::String(shared_string(pem.as_str()))),
            Err(err) => failure(&err),
        }
    }

    fn op_fingerprint(
        &self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let Some(Value::CryptoKey(handle)) = arguments.first() else {
            return Err(type_mismatch(
                bn_catalog::text("crypto", "type.public_key.expected"),
                bn_catalog::text("crypto", "type.public_key.found"),
                member,
                span,
            ));
        };
        let Some(entry) = self.public_keys.get(handle) else {
            return Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                span,
            ));
        };
        let Value::Integer(alg, _) = &arguments[1] else {
            return Err(type_mismatch(
                bn_catalog::text("crypto", "type.integer.expected"),
                bn_catalog::text("crypto", "type.integer.found"),
                member,
                span,
            ));
        };
        // The type checker bounds the argument to INT32; an id that still does not
        // fit names no algorithm and takes the same failure as an unknown id.
        let alg = i32::try_from(*alg).unwrap_or(i32::MIN);
        match bn_stdlib::crypto_asymmetric::bn_spki_fingerprint(&entry.spki, alg) {
            Ok(text) => Ok(Value::String(shared_string(text.as_str()))),
            Err(err) => failure(&err),
        }
    }

    fn op_export_pem_plain(
        &self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let (seed, algorithm) = match self.private_material(arguments, 0, member, span)? {
            Ok(material) => material,
            Err(err) => return failure(&err),
        };
        let Value::Integer(format, _) = &arguments[1] else {
            return Err(type_mismatch("INT32", "non-INT32 value", member, span));
        };
        let Ok(format) = i32::try_from(*format) else {
            return failure(&bn_stdlib::crypto_error::CryptoFailure::Format {
                operation: bn_stdlib::crypto_sign::EXPORT_PLAIN,
            });
        };
        match bn_stdlib::crypto::bn_export_private_pem_unencrypted(&seed, algorithm, format) {
            Ok(pem) => Ok(Value::String(shared_string(pem.as_str()))),
            Err(err) => failure(&err),
        }
    }

    fn op_load_private(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 2, span)?;
        let Value::String(pem) = &arguments[0] else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        let password = match &arguments[1] {
            Value::Null => None,
            Value::String(text) => Some(text.as_ref()),
            _ => {
                return Err(type_mismatch(
                    "STRING OR NULL",
                    "non-STRING value",
                    member,
                    span,
                ));
            }
        };
        match bn_stdlib::crypto::bn_load_private_pem(pem.as_ref(), password) {
            Ok(entry) => Ok(self.insert_private_key(entry)),
            Err(err) => failure(&err),
        }
    }

    fn op_load_public(
        &mut self,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        require_arity(member, arguments, 1, span)?;
        let Value::String(pem) = &arguments[0] else {
            return Err(type_mismatch("STRING", "non-STRING value", member, span));
        };
        match bn_stdlib::crypto::bn_load_public_pem(pem.as_ref()) {
            Ok(entry) => Ok(self.insert_public_key(entry)),
            Err(err) => failure(&err),
        }
    }

    fn asymmetric_op(
        &mut self,
        fs: &bn_stdlib::FsPolicy,
        method: &str,
        member: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match method {
            "NewKeyPair" => self.op_new_keypair(member, arguments, span),
            "NewKeyPairFor" => self.op_new_keypair_for(member, arguments, span),
            "NewKeyPairWith" => self.op_new_keypair_with(member, arguments, span),
            "PublicKey" => self.op_public_key(member, arguments, span),
            "Sign" => self.op_sign(member, arguments, span),
            "SignFile" => self.op_sign_file(fs, member, arguments, span),
            "Verify" => self.op_verify(member, arguments, span),
            "VerifyFile" => self.op_verify_file(fs, member, arguments, span),
            "ExportPem" => self.op_export_pem(member, arguments, span),
            "ExportPemUnencrypted" => self.op_export_pem_plain(member, arguments, span),
            "Fingerprint" => self.op_fingerprint(member, arguments, span),
            "LoadPrivateKey" => self.op_load_private(member, arguments, span),
            "LoadPublicKey" => self.op_load_public(member, arguments, span),
            "Id" => self.op_key_id(member, arguments, span),
            "Algorithm" => self.op_key_algorithm(member, arguments, span),
            "Zeroize" => self.op_key_zeroize(member, arguments, span),
            _ => Err(name_not_found(method, "BNCrypto function", span)),
        }
    }

    fn release_crypto_key(&mut self, handle: u64, span: Span) -> Result<(), Diagnostic> {
        if let Some(holders) = self.key_holders.get_mut(&handle) {
            *holders -= 1;
            if *holders == 0 {
                self.key_holders.remove(&handle);
                if let Some(mut entry) = self.keys.remove(&handle) {
                    entry.material.zeroize();
                }
            }
            Ok(())
        } else if let Some(holders) = self.public_key_holders.get_mut(&handle) {
            *holders -= 1;
            if *holders == 0 {
                self.public_key_holders.remove(&handle);
                self.public_keys.remove(&handle);
            }
            Ok(())
        } else if let Some(holders) = self.private_key_holders.get_mut(&handle) {
            *holders -= 1;
            if *holders == 0 {
                self.private_key_holders.remove(&handle);
                if let Some(mut entry) = self.private_keys.remove(&handle) {
                    entry.seed.as_mut_slice().zeroize();
                }
            }
            Ok(())
        } else {
            Err(runtime_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                span,
            ))
        }
    }
}

impl Provider for CryptoProvider {
    fn call(
        &mut self,
        core: &mut dyn CoreContext,
        member: &str,
        arguments: Vec<Value>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let method = member.rsplit('.').next().unwrap_or_default();
        match method {
            "SHA256" | "SHA512" | "FromText" | "FromHex" | "Length" | "ToHex" | "Slice"
            | "ToBytes" | "ConstantTimeEquals" | "NewSecret" | "SecretFromText" => {
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
            "NewKeyPair"
            | "NewKeyPairFor"
            | "NewKeyPairWith"
            | "PublicKey"
            | "Sign"
            | "SignFile"
            | "VerifyFile"
            | "ExportPem"
            | "ExportPemUnencrypted"
            | "Fingerprint"
            | "LoadPrivateKey"
            | "LoadPublicKey" => {
                self.asymmetric_op(core.host().filesystem(), method, member, &arguments, span)
            }
            "Verify" if arguments.len() == 3 => {
                self.asymmetric_op(core.host().filesystem(), method, member, &arguments, span)
            }
            "Id" => {
                if matches!(arguments.first(), Some(Value::CryptoKey(h)) if self.public_keys.contains_key(h) || self.private_keys.contains_key(h))
                {
                    self.asymmetric_op(core.host().filesystem(), method, member, &arguments, span)
                } else {
                    self.secret_key_op(core.host().filesystem(), method, member, &arguments, span)
                }
            }
            "Algorithm" => {
                if matches!(arguments.first(), Some(Value::CryptoKey(h)) if self.public_keys.contains_key(h) || self.private_keys.contains_key(h))
                {
                    self.asymmetric_op(core.host().filesystem(), method, member, &arguments, span)
                } else {
                    self.key_or_entropy(method, member, &arguments, span)
                }
            }
            "Zeroize" => {
                if matches!(arguments.first(), Some(Value::CryptoKey(h)) if self.private_keys.contains_key(h))
                {
                    self.asymmetric_op(core.host().filesystem(), method, member, &arguments, span)
                } else {
                    self.key_or_entropy(method, member, &arguments, span)
                }
            }
            "RandomBytes" | "GenerateNonce" | "GenerateKey" | "ImportKey" => {
                self.key_or_entropy(method, member, &arguments, span)
            }
            "RandomInt" | "RandomToken" | "RandomUuidV4" | "UuidV7" | "RandomPassword" => {
                csprng_dispatch(method, member, &arguments, span)
            }
            "NewSecretKey" | "NewSecretKeyWith" | "SecretKeyFromBytes" | "WrapKey"
            | "UnwrapKey" | "UnwrapKeyWith" | "Bits" | "Purpose" | "ExportSecret" | "Encrypt"
            | "Decrypt" | "EncryptText" | "DecryptText" | "EncryptFile" | "DecryptFile" => {
                self.secret_key_op(core.host().filesystem(), method, member, &arguments, span)
            }
            "EncryptWith" | "DecryptWith" => self.aead_with_op(method, member, &arguments, span),
            "EncryptFileWithPassword" | "DecryptFileWithPassword" => {
                password_file_op(core.host().filesystem(), method, member, &arguments, span)
            }
            "HashPasswordWith" | "NeedsRehash" | "DeriveKey" | "DeriveSubkey" => {
                self.kdf_options_op(method, member, &arguments, span)
            }
            "Hash" | "HashText" | "HashFile" | "HashWith" | "Update" | "Finish" | "Verify" => {
                self.hash_op(core, method, member, &arguments, span)
            }
            "Mac" | "VerifyMac" | "MacWith" => self.mac_op(method, member, &arguments, span),
            "HashPassword" | "VerifyPassword" | "Hkdf" => {
                self.password_op(method, member, &arguments, span)
            }
            "ToText" | "ToBase64" | "FromBase64" | "ToBase64Url" | "FromBase64Url" | "Concat"
            | "ToPem" | "FromPem" => self.encoding_op(method, member, &arguments, span),
            "Crc32" | "Crc32c" | "Adler32" | "XxHash64" | "VerifyFileChecksum" => {
                self.checksum_op(core.host().filesystem(), method, member, &arguments, span)
            }
            "New" | "NewWith" if member.contains("Hasher") => {
                self.hash_op(core, method, member, &arguments, span)
            }
            "New" | "NewWith" if member.contains("MacStream") => {
                self.mac_stream_new(core, member, &arguments, span)
            }
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
        } else if base == Some("Secret") {
            Some(Ok(self.insert_secret(Vec::new())))
        } else if base == Some("SecretKey") {
            Some(Ok(self.insert_secret_key(SecretKeyEntry {
                material: zeroize::Zeroizing::new(Vec::new()),
                bits: 0,
                purpose: String::new(),
                alg: String::new(),
                zeroized: false,
            })))
        } else if base == Some("PublicKey") {
            Some(Ok(self.insert_public_key(PublicKeyEntry {
                spki: Vec::new(),
                raw: Vec::new(),
                algorithm: 0,
                key_id: String::new(),
            })))
        } else if base == Some("PrivateKey") {
            Some(Ok(self.insert_private_key(PrivateKeyEntry {
                seed: zeroize::Zeroizing::new(Vec::new()),
                spki: Vec::new(),
                raw_public: Vec::new(),
                algorithm: 0,
                key_id: String::new(),
                zeroized: false,
            })))
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
            Value::CryptoKey(handle) => {
                Some(if let Some(holders) = self.key_holders.get_mut(handle) {
                    *holders += 1;
                    Ok(())
                } else if let Some(holders) = self.public_key_holders.get_mut(handle) {
                    *holders += 1;
                    Ok(())
                } else if let Some(holders) = self.private_key_holders.get_mut(handle) {
                    *holders += 1;
                    Ok(())
                } else {
                    Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        bn_catalog::text("crypto", "msg.bncrypto_key_was_already_released"),
                        span,
                    ))
                })
            }
            Value::CryptoSecret(handle) => Some(match self.secret_holders.get_mut(handle) {
                Some(holders) => {
                    *holders += 1;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNCrypto.Secret was already released",
                    span,
                )),
            }),
            Value::CryptoSecretKey(handle) => Some(match self.secret_key_holders.get_mut(handle) {
                Some(holders) => {
                    *holders += 1;
                    Ok(())
                }
                None => Err(runtime_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "BNCrypto.SecretKey was already released",
                    span,
                )),
            }),
            _ => None,
        }
    }

    fn release(&mut self, value: &Value, span: Span) -> Option<Result<(), Diagnostic>> {
        match value {
            Value::CryptoBytes(handle) => {
                Some(if let Some(holders) = self.holders.get_mut(handle) {
                    *holders -= 1;
                    if *holders == 0 {
                        self.holders.remove(handle);
                        if let Some(mut buf) = self.buffers.remove(handle) {
                            buf.zeroize();
                        }
                    }
                    Ok(())
                } else {
                    Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        bn_catalog::text("crypto", "msg.bncrypto_bytes_was_already_released"),
                        span,
                    ))
                })
            }
            Value::CryptoKey(handle) => Some(self.release_crypto_key(*handle, span)),
            Value::CryptoSecret(handle) => {
                Some(if let Some(holders) = self.secret_holders.get_mut(handle) {
                    *holders -= 1;
                    if *holders == 0 {
                        self.secret_holders.remove(handle);
                        if let Some(mut buf) = self.secrets.remove(handle) {
                            buf.zeroize();
                        }
                    }
                    Ok(())
                } else {
                    Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.Secret was already released",
                        span,
                    ))
                })
            }
            Value::CryptoSecretKey(handle) => Some(
                if let Some(holders) = self.secret_key_holders.get_mut(handle) {
                    *holders -= 1;
                    if *holders == 0 {
                        self.secret_key_holders.remove(handle);
                        if let Some(mut entry) = self.secret_keys.remove(handle) {
                            entry.material.as_mut_slice().zeroize();
                        }
                    }
                    Ok(())
                } else {
                    Err(runtime_error(
                        bn_diag::DiagId::USE_AFTER_RELEASE,
                        "BNCrypto.SecretKey was already released",
                        span,
                    ))
                },
            ),
            Value::Object { handle, .. } => {
                if let Some(stream_id) = self.hashers.remove(handle) {
                    let _ = bn_stdlib::crypto::release_hasher(stream_id);
                    return Some(Ok(()));
                }
                if let Some(stream_id) = self.mac_streams.remove(handle) {
                    let _ = bn_stdlib::crypto::release_mac_stream(stream_id);
                    return Some(Ok(()));
                }
                None
            }
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

fn csprng_dispatch(
    method: &str,
    member: &str,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    match method {
        "RandomInt" => {
            require_arity(member, arguments, 2, span)?;
            let Value::Integer(min, _) = &arguments[0] else {
                return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
            };
            let Value::Integer(max, _) = &arguments[1] else {
                return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
            };
            let Ok(min_i64) = i64::try_from(*min) else {
                return Err(type_mismatch("INTEGER", "out-of-range value", member, span));
            };
            let Ok(max_i64) = i64::try_from(*max) else {
                return Err(type_mismatch("INTEGER", "out-of-range value", member, span));
            };
            match bn_stdlib::crypto::random_int(min_i64, max_i64) {
                Ok(val) => Ok(Value::Integer(i128::from(val), IntegerType::Int64)),
                Err(err) => failure(&err),
            }
        }
        "RandomToken" => {
            require_arity(member, arguments, 1, span)?;
            let Value::Integer(n_bytes, _) = &arguments[0] else {
                return Err(type_mismatch("INTEGER", "non-INTEGER value", member, span));
            };
            let Ok(n_bytes_i64) = i64::try_from(*n_bytes) else {
                return Err(type_mismatch("INTEGER", "out-of-range value", member, span));
            };
            match bn_stdlib::crypto::random_token(n_bytes_i64) {
                Ok(token) => Ok(Value::String(shared_string(token))),
                Err(err) => failure(&err),
            }
        }
        "RandomUuidV4" => {
            require_arity(member, arguments, 0, span)?;
            Ok(Value::String(shared_string(
                bn_stdlib::crypto::random_uuid_v4(),
            )))
        }
        "UuidV7" => {
            require_arity(member, arguments, 0, span)?;
            Ok(Value::String(shared_string(bn_stdlib::crypto::uuid_v7())))
        }
        "RandomPassword" => {
            require_arity(member, arguments, 1, span)?;
            let Value::Record { record } = &arguments[0] else {
                return Err(type_mismatch(
                    "Crypto.PasswordGenOptions",
                    "non-record value",
                    member,
                    span,
                ));
            };
            let length = match record.get(0) {
                Some(Value::Integer(len, _)) => usize::try_from(*len).unwrap_or(0),
                _ => 20,
            };
            let upper = match record.get(1) {
                Some(Value::Boolean(b)) => *b,
                _ => true,
            };
            let lower = match record.get(2) {
                Some(Value::Boolean(b)) => *b,
                _ => true,
            };
            let digits = match record.get(3) {
                Some(Value::Boolean(b)) => *b,
                _ => true,
            };
            let symbols = match record.get(4) {
                Some(Value::Boolean(b)) => *b,
                _ => true,
            };
            let opts = bn_stdlib::crypto::PasswordGenOptions {
                length,
                include_uppercase: upper,
                include_lowercase: lower,
                include_digits: digits,
                include_symbols: symbols,
            };
            match bn_stdlib::crypto::random_password(opts) {
                Ok(pwd) => Ok(Value::String(shared_string(pwd))),
                Err(err) => failure(&err),
            }
        }
        _ => Err(name_not_found(method, "BNCrypto CSPRNG function", span)),
    }
}

/// Field index of `policy` in each options record (`modules/bn/BNCrypto.bn`).
const HASH_OPTS_POLICY_FIELD: usize = 1;
const MAC_OPTS_POLICY_FIELD: usize = 1;
const KEY_PAIR_OPTS_POLICY_FIELD: usize = 1;
/// `AeadOptions` fields after `alg` (index 0), in declaration order.
const AEAD_OPTS_AAD_FIELD: usize = 1;
const AEAD_OPTS_INCLUDE_KEY_ID_FIELD: usize = 2;
/// `PasswordOptions` fields, in declaration order.
const PASSWORD_OPTS_MEMORY_FIELD: usize = 0;
const PASSWORD_OPTS_ITERATIONS_FIELD: usize = 1;
const PASSWORD_OPTS_PARALLELISM_FIELD: usize = 2;

/// The options record at `index`, or a type mismatch naming `expected`.
fn options_record<'a>(
    arguments: &'a [Value],
    index: usize,
    expected: &str,
    member: &str,
    span: Span,
) -> Result<&'a RecordValue, Diagnostic> {
    match arguments.get(index) {
        Some(Value::Record { record }) => Ok(record),
        _ => Err(type_mismatch(expected, "non-record value", member, span)),
    }
}

/// The `alg` field (index 0) of an options record. Its default is the
/// record's; a value out of `INT32` range is a type mismatch, never replaced.
fn options_alg(record: &RecordValue, member: &str, span: Span) -> Result<i32, Diagnostic> {
    match record.get(0) {
        Some(Value::Integer(value, _)) => i32::try_from(*value)
            .map_err(|_| type_mismatch("INT32", "out-of-range alg value", member, span)),
        _ => Err(type_mismatch(
            "INTEGER",
            "non-integer alg field",
            member,
            span,
        )),
    }
}

/// The `policy` field of an options record (`Policy { allowLegacy, fipsOnly }`).
fn options_policy(record: &RecordValue, field: usize) -> CryptoPolicy {
    let Some(Value::Record { record: policy }) = record.get(field) else {
        return CryptoPolicy::default();
    };
    CryptoPolicy {
        allow_legacy: matches!(policy.get(0), Some(Value::Boolean(true))),
        fips_only: matches!(policy.get(1), Some(Value::Boolean(true))),
    }
}

/// `MacWith` and `MacStream.New`/`NewWith`: a `MacOptions` record gives the
/// algorithm and the policy, an INTEGER gives the algorithm, no argument is
/// HMAC-SHA256 with the default policy.
fn mac_alg_and_policy(
    argument: Option<&Value>,
    member: &str,
    span: Span,
) -> Result<(i32, CryptoPolicy), Diagnostic> {
    match argument {
        Some(Value::Record { record }) => Ok((
            options_alg(record, member, span)?,
            options_policy(record, MAC_OPTS_POLICY_FIELD),
        )),
        Some(Value::Integer(value, _)) => Ok((
            i32::try_from(*value)
                .map_err(|_| type_mismatch("INT32", "out-of-range alg value", member, span))?,
            CryptoPolicy::default(),
        )),
        None => Ok((bn_stdlib::crypto::MAC_HMAC_SHA256, CryptoPolicy::default())),
        Some(_) => Err(type_mismatch(
            "BNCrypto.MacOptions",
            "non-record value",
            member,
            span,
        )),
    }
}

/// A private key's seed (wiped when dropped) and its `Alg` selector.
type PrivateMaterial = (zeroize::Zeroizing<Vec<u8>>, i32);

/// `EncryptFileWithPassword(inPath, outPath, password)` and
/// `DecryptFileWithPassword(...)`: `VOID OR Error`, under the file policy.
fn password_file_op(
    fs: &bn_stdlib::FsPolicy,
    method: &str,
    member: &str,
    arguments: &[Value],
    span: Span,
) -> Result<Value, Diagnostic> {
    require_arity(member, arguments, 3, span)?;
    let text_at = |index: usize| match arguments.get(index) {
        Some(Value::String(text)) => Ok(text.to_string()),
        _ => Err(type_mismatch(
            bn_catalog::text("crypto", "type.string.expected"),
            bn_catalog::text("crypto", "type.string.found"),
            member,
            span,
        )),
    };
    let (input, output, password) = (text_at(0)?, text_at(1)?, text_at(2)?);
    let (input, output) = (std::path::Path::new(&input), std::path::Path::new(&output));
    let result = if method == "EncryptFileWithPassword" {
        bn_stdlib::crypto_file::bn_encrypt_file_with_password(fs, &password, input, output)
    } else {
        bn_stdlib::crypto_file::bn_decrypt_file_with_password(fs, &password, input, output)
    };
    match result {
        Ok(()) => Ok(Value::Null),
        Err(err) => failure(&err),
    }
}

#[cfg(test)]
mod tests {
    /// `Crypto.*` codes in `modules/bn/BNCrypto.bn` are the ones both
    /// backends put in `Error.Code`.
    #[test]
    fn module_constants_match_the_runtime_codes() {
        use bn_types::error_codes::crypto;
        let module = include_str!("../../../modules/bn/BNCrypto.bn");
        let codes = [
            ("INVALID_ARGUMENT", crypto::INVALID_ARGUMENT),
            ("AUTHENTICATION_FAILED", crypto::AUTHENTICATION_FAILED),
            ("UNAVAILABLE", crypto::UNAVAILABLE),
            ("UNSUPPORTED_ALGORITHM", crypto::UNSUPPORTED_ALGORITHM),
            ("LEGACY_DISABLED", crypto::LEGACY_DISABLED),
            ("PARSE_ERROR", crypto::PARSE_ERROR),
            ("WRONG_KEY_TYPE", crypto::WRONG_KEY_TYPE),
            ("IO_ERROR", crypto::IO_ERROR),
            ("WRONG_PASSWORD", crypto::WRONG_PASSWORD),
        ];
        for (name, value) in codes {
            let line = format!("EXPORT CONST {name} AS INTEGER = {value}");
            assert!(module.contains(&line), "BNCrypto.bn lacks `{line}`");
        }
    }
}
