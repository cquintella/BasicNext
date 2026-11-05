// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto` `Error`s (`language/0.6/bncrypto.md` "Errors"), one producer for
//! both backends: the interpreter turns a [`CryptoFailure`] into an `Error`
//! value, the C ABI records it for the emitted code.

use bn_types::error_codes::crypto;

use super::crypto::Aead;

/// Why a `BNCrypto` operation returned `Error`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CryptoFailure {
    /// `FromHex` of text that is not an even number of hex digits.
    Hex(String),
    /// `Seal*` with a key or nonce of the wrong length.
    Seal(Aead),
    /// `Open*` whose tag does not verify.
    Open(Aead),
    /// `Argon2id` cost parameters out of range.
    Argon2id,
    /// A private key the scheme rejects (`operation` names the member).
    Key(&'static str),
    /// `Slice(start, length)` outside a buffer of `size` bytes.
    Slice {
        start: i64,
        length: i64,
        size: usize,
    },
    /// A seed of the wrong length (`MlKemKeypair`, `MlDsaKeypair`).
    Seed(&'static str),
    /// `MlKemEncapsulate` with a malformed encapsulation key.
    EncapsulationKey,
    /// `MlKemDecapsulate` rejects the ciphertext or key.
    Decapsulate,
    /// `MlDsaSign` with a malformed signing key.
    SigningKey,
    /// Operating system CSPRNG entropy failure (`operation` names the caller).
    Random(&'static str),
    /// Invalid or unsupported algorithm constant.
    Algorithm(i32),
    /// Key material length does not match algorithm requirement.
    KeyLength { expected: usize, actual: usize },
    /// Attempt to use an already zeroized key.
    ZeroizedKey,
    /// `RandomBytes` with a length outside 0 through [`crate::crypto::MAX_RANDOM_BYTES`].
    RandomLength(i128),
    /// `RandomInt` with `min > max`.
    RandomIntRange { min: i64, max: i64 },
    /// `RandomToken` with length <= 0 or > `MAX_RANDOM_BYTES`.
    RandomTokenLength(i64),
    /// `RandomPassword` with length <= 0.
    PasswordLength(usize),
    /// `RandomPassword` with all character classes disabled.
    PasswordNoClasses,
    /// `WrapKey` with key material not >= 16 bytes or not a multiple of 8.
    KeyWrapLength,
    /// `UnwrapKey` authentication/integrity verification failure.
    KeyUnwrapAuth,
    /// Invalid or unsupported envelope format.
    EnvelopeFormat,
    /// Envelope authentication failure.
    EnvelopeAuth,
    /// Envelope key length mismatch.
    EnvelopeKeyLength,
    /// File I/O error during file encryption or decryption.
    FileIo(String),
    /// Cannot decode input as Base64. The input is not kept: it may be a secret.
    Base64,
    /// Cannot decode input as `Base64Url`. The input is not kept.
    Base64Url,
    /// Malformed PEM input (`PARSE_ERROR`). The input is not kept.
    Pem,
    /// Strict UTF-8 validation failed.
    Utf8,
    /// Requested algorithm id is not accepted by this operation (`UNSUPPORTED_ALGORITHM`).
    UnsupportedAlgorithm { operation: &'static str },
    /// MD5 or SHA-1 requested: reserved in this version (`LEGACY_DISABLED`).
    LegacyDisabled { operation: &'static str },
    /// Malformed or ambiguous PEM (`PARSE_ERROR`). The password is never stored.
    Parse { operation: &'static str },
    /// The key algorithm cannot perform the operation (`WRONG_KEY_TYPE`).
    WrongKeyType { operation: &'static str },
    /// A file could not be read (`IO_ERROR`).
    Io { operation: &'static str },
    /// PKCS#8 password authentication failed (`WRONG_PASSWORD`). The password is never stored.
    WrongPassword { operation: &'static str },
    /// Export format other than `PKCS8` (`INVALID_ARGUMENT`).
    Format { operation: &'static str },
}

impl CryptoFailure {
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::Open(_) | Self::Decapsulate | Self::KeyUnwrapAuth | Self::EnvelopeAuth => {
                crypto::AUTHENTICATION_FAILED
            }
            Self::Random(_) => crypto::UNAVAILABLE,
            Self::UnsupportedAlgorithm { .. } => crypto::UNSUPPORTED_ALGORITHM,
            Self::LegacyDisabled { .. } => crypto::LEGACY_DISABLED,
            Self::Parse { .. } | Self::Pem => crypto::PARSE_ERROR,
            Self::WrongKeyType { .. } => crypto::WRONG_KEY_TYPE,
            Self::Io { .. } => crypto::IO_ERROR,
            Self::WrongPassword { .. } => crypto::WRONG_PASSWORD,
            _ => crypto::INVALID_ARGUMENT,
        }
    }

    /// `Error.Operation`.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        match self {
            Self::Hex(_) => "BNCrypto.FromHex",
            Self::Seal(Aead::Aes256Gcm) => "BNCrypto.SealAesGcm",
            Self::Seal(Aead::ChaCha20Poly1305) => "BNCrypto.SealChaCha20",
            Self::Open(Aead::Aes256Gcm) => "BNCrypto.OpenAesGcm",
            Self::Open(Aead::ChaCha20Poly1305) => "BNCrypto.OpenChaCha20",
            Self::Argon2id => "BNCrypto.Argon2id",
            Self::Key(operation)
            | Self::Seed(operation)
            | Self::Random(operation)
            | Self::UnsupportedAlgorithm { operation }
            | Self::LegacyDisabled { operation }
            | Self::Parse { operation }
            | Self::WrongKeyType { operation }
            | Self::Io { operation }
            | Self::WrongPassword { operation }
            | Self::Format { operation } => operation,
            Self::Slice { .. } => "BNCrypto.Slice",
            Self::EncapsulationKey => "BNCrypto.MlKemEncapsulate",
            Self::Decapsulate => "BNCrypto.MlKemDecapsulate",
            Self::SigningKey => "BNCrypto.MlDsaSign",
            Self::Algorithm(_) | Self::KeyLength { .. } => "BNCrypto.ImportKey",
            Self::ZeroizedKey => "BNCrypto.Key",
            Self::RandomLength(_) => "BNCrypto.RandomBytes",
            Self::RandomIntRange { .. } => "BNCrypto.RandomInt",
            Self::RandomTokenLength(_) => "BNCrypto.RandomToken",
            Self::PasswordLength(_) | Self::PasswordNoClasses => "BNCrypto.RandomPassword",
            Self::KeyWrapLength => "BNCrypto.WrapKey",
            Self::KeyUnwrapAuth => "BNCrypto.UnwrapKey",
            Self::EnvelopeFormat | Self::EnvelopeAuth => "BNCrypto.Decrypt",
            Self::EnvelopeKeyLength => "BNCrypto.Encrypt",
            Self::FileIo(_) => "BNCrypto.File",
            Self::Base64 => "BNCrypto.FromBase64",
            Self::Base64Url => "BNCrypto.FromBase64Url",
            Self::Pem => "BNCrypto.FromPem",
            Self::Utf8 => "BNCrypto.Bytes.ToText",
        }
    }

    /// `Error.Message`: what failed, naming the input where it identifies it.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Hex(text) => {
                let shown: String = text.chars().take(64).collect();
                let more = if text.chars().count() > 64 { "…" } else { "" };
                bn_catalog::fill(
                    "crypto",
                    "hex.decode.message",
                    &[("shown", &shown), ("more", &more)],
                )
            }
            Self::Seal(_) => "cannot seal the plaintext".into(),
            Self::Open(_) => "cannot open the ciphertext".into(),
            Self::Argon2id => "cannot derive a key with Argon2id".into(),
            Self::Key(_) => "cannot use the private key".into(),
            Self::Slice { .. } => "cannot slice the buffer".into(),
            Self::Seed(_) => "cannot derive the key pair".into(),
            Self::EncapsulationKey => "cannot encapsulate a secret".into(),
            Self::Decapsulate => "cannot decapsulate the ciphertext".into(),
            Self::SigningKey => "cannot sign the message".into(),
            Self::Random(_) => "cannot obtain random bytes from system entropy".into(),
            Self::Algorithm(code) => format!("unsupported or invalid algorithm code {code}"),
            Self::KeyLength { expected, actual } => {
                format!("key material length {actual} does not match required length {expected}")
            }
            Self::ZeroizedKey => "key material has already been zeroized".into(),
            Self::RandomLength(length) => {
                bn_catalog::fill("crypto", "random.length.message", &[("length", length)])
            }
            Self::RandomIntRange { min, max } => bn_catalog::fill(
                "crypto",
                "random.int.range.message",
                &[("min", min), ("max", max)],
            ),
            Self::RandomTokenLength(length) => bn_catalog::fill(
                "crypto",
                "random.token.length.message",
                &[("length", length)],
            ),
            Self::PasswordLength(length) => bn_catalog::fill(
                "crypto",
                "random.password.length.message",
                &[("length", length)],
            ),
            Self::PasswordNoClasses => {
                bn_catalog::text("crypto", "random.password.classes.message").into()
            }
            Self::KeyWrapLength => {
                "key data length must be at least 16 bytes and a multiple of 8".into()
            }
            Self::KeyUnwrapAuth => "RFC 3394 key unwrap integrity check failed".into(),
            Self::EnvelopeFormat => "invalid authenticated encryption envelope format".into(),
            Self::EnvelopeAuth => "cannot open authenticated encryption envelope".into(),
            Self::EnvelopeKeyLength => {
                "key material length must be 32 bytes for authenticated encryption".into()
            }
            Self::FileIo(err) => format!("file I/O error: {err}"),
            Self::Base64 => bn_catalog::text("crypto", "decode.base64.message").into(),
            Self::Base64Url => bn_catalog::text("crypto", "decode.base64url.message").into(),
            Self::Pem => bn_catalog::text("crypto", "decode.pem.message").into(),
            Self::Utf8 => bn_catalog::text("crypto", "decode.utf8.message").into(),
            Self::UnsupportedAlgorithm { .. } => {
                bn_catalog::text("crypto", "algorithm.unsupported.message").into()
            }
            Self::LegacyDisabled { .. } => {
                bn_catalog::text("crypto", "algorithm.legacy.message").into()
            }
            Self::Parse { .. } => bn_catalog::text("crypto", "parse.pem.message").into(),
            Self::WrongKeyType { .. } => {
                bn_catalog::text("crypto", "key.wrong_type.message").into()
            }
            Self::Io { .. } => bn_catalog::text("crypto", "io.file.message").into(),
            Self::WrongPassword { .. } => {
                bn_catalog::text("crypto", "password.wrong.message").into()
            }
            Self::Format { .. } => bn_catalog::text("crypto", "format.rejected.message").into(),
        }
    }

    /// `Error.Cause`: the violated rule.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::Hex(_) => {
                "the text must hold an even number of hexadecimal digits (0-9, a-f, A-F)".into()
            }
            Self::Seal(_) => "the key must be 32 bytes and the nonce 12 bytes".into(),
            Self::Open(_) | Self::EnvelopeAuth => {
                bn_catalog::text("crypto", "open.tag.cause").into()
            }
            Self::Argon2id => {
                "the memory, iteration, or parallelism cost is outside Argon2id's bounds".into()
            }
            Self::Key(_) => "the key has the wrong length or is not valid for the scheme".into(),
            Self::Slice {
                start,
                length,
                size,
            } => {
                if *start < 0 || *length < 0 {
                    format!("the start and length must not be negative; got {start} and {length}")
                } else {
                    format!(
                        "the range {start}..{} is outside the {size} bytes of the buffer",
                        start.saturating_add(*length)
                    )
                }
            }
            Self::Seed(_) => "the seed has the wrong length".into(),
            Self::EncapsulationKey => {
                "the encapsulation key has the wrong length or is malformed".into()
            }
            Self::Decapsulate => "the ciphertext or the private key is malformed".into(),
            Self::SigningKey => "the signing key has the wrong length or is malformed".into(),
            Self::Random(_) => "the system entropy generator failed or is unavailable".into(),
            Self::Algorithm(_) => "the algorithm code must be one of the ALG_* constants".into(),
            Self::KeyLength { .. } => {
                "the key material must match the exact byte length required by the algorithm".into()
            }
            Self::ZeroizedKey => "zeroized keys cannot be used for cryptographic operations".into(),
            Self::RandomLength(_) => bn_catalog::fill(
                "crypto",
                "random.length.cause",
                &[("limit", &crate::crypto::MAX_RANDOM_BYTES)],
            ),
            Self::RandomIntRange { .. } => {
                bn_catalog::text("crypto", "random.int.range.cause").into()
            }
            Self::RandomTokenLength(_) => {
                bn_catalog::text("crypto", "random.token.length.cause").into()
            }
            Self::PasswordLength(_) => {
                bn_catalog::text("crypto", "random.password.length.cause").into()
            }
            Self::PasswordNoClasses => {
                bn_catalog::text("crypto", "random.password.classes.cause").into()
            }
            Self::KeyWrapLength => {
                "key data length must be at least 16 bytes and a multiple of 8".into()
            }
            Self::KeyUnwrapAuth => {
                "unwrapped key failed integrity check; key or KEK mismatch".into()
            }
            Self::EnvelopeFormat => {
                "the envelope must start with BNC1 and have a valid header, nonce and ciphertext"
                    .into()
            }
            Self::EnvelopeKeyLength => {
                "authenticated encryption envelopes require a 256-bit (32-byte) key".into()
            }
            Self::FileIo(_) => "filesystem read or write failed".into(),
            Self::Base64 => bn_catalog::text("crypto", "decode.base64.cause").into(),
            Self::Base64Url => bn_catalog::text("crypto", "decode.base64url.cause").into(),
            Self::Pem => bn_catalog::text("crypto", "decode.pem.cause").into(),
            Self::Utf8 => bn_catalog::text("crypto", "decode.utf8.cause").into(),
            Self::UnsupportedAlgorithm { .. } => {
                bn_catalog::text("crypto", "algorithm.unsupported.cause").into()
            }
            Self::LegacyDisabled { .. } => {
                bn_catalog::text("crypto", "algorithm.legacy.cause").into()
            }
            Self::Parse { .. } => bn_catalog::text("crypto", "parse.pem.cause").into(),
            Self::WrongKeyType { .. } => bn_catalog::text("crypto", "key.wrong_type.cause").into(),
            Self::Io { .. } => bn_catalog::text("crypto", "io.file.cause").into(),
            Self::WrongPassword { .. } => bn_catalog::text("crypto", "password.wrong.cause").into(),
            Self::Format { .. } => bn_catalog::text("crypto", "format.rejected.cause").into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Aead, CryptoFailure};
    use bn_types::error_codes::crypto;

    #[test]
    fn failures_carry_code_operation_message_and_cause() {
        let open = CryptoFailure::Open(Aead::ChaCha20Poly1305);
        assert_eq!(open.code(), crypto::AUTHENTICATION_FAILED);
        assert_eq!(open.operation(), "BNCrypto.OpenChaCha20");
        let hex = CryptoFailure::Hex("abc".into());
        assert_eq!(hex.code(), crypto::INVALID_ARGUMENT);
        assert_eq!(hex.message(), "cannot decode \"abc\" as hexadecimal");
        let slice = CryptoFailure::Slice {
            start: 2,
            length: 5,
            size: 4,
        };
        assert_eq!(
            slice.cause(),
            "the range 2..7 is outside the 4 bytes of the buffer"
        );
    }
}
