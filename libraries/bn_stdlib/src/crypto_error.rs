// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto` `Error`s (`language/0.6/bncrypto.md` "Errors"), one producer for
//! both backends: the interpreter turns a [`CryptoFailure`] into an `Error`
//! value, the C ABI records it for the emitted code.

use bn_types::error_codes::crypto;

use super::crypto::Aead;

/// Characters of a rejected hex input that `FromHex` shows in its message.
const HEX_SHOWN_CHARS: usize = 64;

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
    /// File I/O error while streaming a file (`IO_ERROR`): `HashFile`,
    /// `EncryptFile`, `DecryptFile`. Carries the operating-system detail.
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
    /// An Argon2id cost below the floor of `HashPassword`, or outside Argon2id's
    /// bounds (`INVALID_ARGUMENT`).
    PasswordCost { operation: &'static str },
    /// A `DeriveKey` salt shorter than 16 bytes (`INVALID_ARGUMENT`).
    SaltLength { operation: &'static str },
    /// A `DeriveSubkey` with an empty context (`INVALID_ARGUMENT`).
    EmptyContext { operation: &'static str },
}

/// Catalog domain of every `BNCrypto` failure text.
const CATALOG: &str = "crypto";

fn text(key: &str) -> &'static str {
    bn_catalog::text(CATALOG, key)
}

fn owned(key: &str) -> String {
    text(key).into()
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
            Self::Io { .. } | Self::FileIo(_) => crypto::IO_ERROR,
            Self::WrongPassword { .. } => crypto::WRONG_PASSWORD,
            _ => crypto::INVALID_ARGUMENT,
        }
    }

    /// `Error.Operation`.
    #[must_use]
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Key(operation)
            | Self::Seed(operation)
            | Self::Random(operation)
            | Self::UnsupportedAlgorithm { operation }
            | Self::LegacyDisabled { operation }
            | Self::Parse { operation }
            | Self::WrongKeyType { operation }
            | Self::Io { operation }
            | Self::WrongPassword { operation }
            | Self::Format { operation }
            | Self::PasswordCost { operation }
            | Self::SaltLength { operation }
            | Self::EmptyContext { operation } => operation,
            Self::Hex(_) => text("op.from_hex"),
            Self::Seal(Aead::Aes256Gcm) => text("op.seal_aes_gcm"),
            Self::Seal(Aead::ChaCha20Poly1305) => text("op.seal_chacha20"),
            Self::Open(Aead::Aes256Gcm) => text("op.open_aes_gcm"),
            Self::Open(Aead::ChaCha20Poly1305) => text("op.open_chacha20"),
            Self::Argon2id => text("op.argon2id"),
            Self::Slice { .. } => text("op.slice"),
            Self::EncapsulationKey => text("op.mlkem_encapsulate"),
            Self::Decapsulate => text("op.mlkem_decapsulate"),
            Self::SigningKey => text("op.mldsa_sign"),
            Self::Algorithm(_) | Self::KeyLength { .. } => text("op.import_key"),
            Self::ZeroizedKey => text("op.key"),
            Self::RandomLength(_) => text("op.random_bytes"),
            Self::RandomIntRange { .. } => text("op.random_int"),
            Self::RandomTokenLength(_) => text("op.random_token"),
            Self::PasswordLength(_) | Self::PasswordNoClasses => text("op.random_password"),
            Self::KeyWrapLength => text("op.wrap_key"),
            Self::KeyUnwrapAuth => text("op.unwrap_key"),
            Self::EnvelopeFormat | Self::EnvelopeAuth => text("op.decrypt"),
            Self::EnvelopeKeyLength => text("op.encrypt"),
            Self::FileIo(_) => text("op.file"),
            Self::Base64 => text("op.from_base64"),
            Self::Base64Url => text("op.from_base64url"),
            Self::Pem => text("op.from_pem"),
            Self::Utf8 => text("op.bytes_to_text"),
        }
    }

    /// `Error.Message`: what failed, naming the input where it identifies it.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Hex(input) => {
                let shown: String = input.chars().take(HEX_SHOWN_CHARS).collect();
                let more = if input.chars().count() > HEX_SHOWN_CHARS {
                    text("hex.decode.more")
                } else {
                    ""
                };
                bn_catalog::fill(
                    "crypto",
                    "hex.decode.message",
                    &[("shown", &shown), ("more", &more)],
                )
            }
            Self::Seal(_) => owned("seal.message"),
            Self::Open(_) => owned("open.message"),
            Self::Argon2id => owned("argon2id.message"),
            Self::Key(_) => owned("key.message"),
            Self::Slice { .. } => owned("slice.message"),
            Self::Seed(_) => owned("seed.message"),
            Self::EncapsulationKey => owned("encapsulation.message"),
            Self::Decapsulate => owned("decapsulate.message"),
            Self::SigningKey => owned("signing_key.message"),
            Self::Random(_) => owned("random.message"),
            Self::Algorithm(code) => {
                bn_catalog::fill("crypto", "algorithm.code.message", &[("code", code)])
            }
            Self::KeyLength { expected, actual } => bn_catalog::fill(
                "crypto",
                "key.length.message",
                &[("actual", actual), ("expected", expected)],
            ),
            Self::ZeroizedKey => owned("key.zeroized.message"),
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
            Self::PasswordNoClasses => owned("random.password.classes.message"),
            Self::KeyWrapLength => owned("key.wrap.length.message"),
            Self::KeyUnwrapAuth => owned("key.unwrap.message"),
            Self::EnvelopeFormat => owned("envelope.format.message"),
            Self::EnvelopeAuth => owned("envelope.auth.message"),
            Self::EnvelopeKeyLength => owned("envelope.key.message"),
            Self::FileIo(detail) => {
                bn_catalog::fill("crypto", "file.io.message", &[("detail", detail)])
            }
            Self::Base64 => owned("decode.base64.message"),
            Self::Base64Url => owned("decode.base64url.message"),
            Self::Pem => owned("decode.pem.message"),
            Self::Utf8 => owned("decode.utf8.message"),
            Self::UnsupportedAlgorithm { .. } => owned("algorithm.unsupported.message"),
            Self::LegacyDisabled { .. } => owned("algorithm.legacy.message"),
            Self::Parse { .. } => owned("parse.pem.message"),
            Self::WrongKeyType { .. } => owned("key.wrong_type.message"),
            Self::Io { .. } => owned("io.file.message"),
            Self::WrongPassword { .. } => owned("password.wrong.message"),
            Self::Format { .. } => owned("format.rejected.message"),
            Self::PasswordCost { .. } => owned("password.cost.message"),
            Self::SaltLength { .. } => owned("derive.salt.message"),
            Self::EmptyContext { .. } => owned("derive.context.message"),
        }
    }

    /// `Error.Cause`: the violated rule.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::Hex(_) => owned("hex.decode.cause"),
            Self::Seal(_) => owned("seal.cause"),
            Self::Open(_) | Self::EnvelopeAuth => owned("open.tag.cause"),
            Self::Argon2id => owned("argon2id.cause"),
            Self::Key(_) => owned("key.cause"),
            Self::Slice {
                start,
                length,
                size,
            } => {
                if *start < 0 || *length < 0 {
                    bn_catalog::fill(
                        "crypto",
                        "slice.negative.cause",
                        &[("start", start), ("length", length)],
                    )
                } else {
                    let end = start.saturating_add(*length);
                    bn_catalog::fill(
                        "crypto",
                        "slice.range.cause",
                        &[("start", start), ("end", &end), ("size", size)],
                    )
                }
            }
            Self::Seed(_) => owned("seed.cause"),
            Self::EncapsulationKey => owned("encapsulation.cause"),
            Self::Decapsulate => owned("decapsulate.cause"),
            Self::SigningKey => owned("signing_key.cause"),
            Self::Random(_) => owned("random.cause"),
            Self::Algorithm(_) => owned("algorithm.code.cause"),
            Self::KeyLength { .. } => owned("key.length.cause"),
            Self::ZeroizedKey => owned("key.zeroized.cause"),
            Self::RandomLength(_) => bn_catalog::fill(
                "crypto",
                "random.length.cause",
                &[("limit", &crate::crypto::MAX_RANDOM_BYTES)],
            ),
            Self::RandomIntRange { .. } => owned("random.int.range.cause"),
            Self::RandomTokenLength(_) => owned("random.token.length.cause"),
            Self::PasswordLength(_) => owned("random.password.length.cause"),
            Self::PasswordNoClasses => owned("random.password.classes.cause"),
            Self::KeyWrapLength => owned("key.wrap.length.message"),
            Self::KeyUnwrapAuth => owned("key.unwrap.cause"),
            Self::EnvelopeFormat => owned("envelope.format.cause"),
            Self::EnvelopeKeyLength => owned("envelope.key.cause"),
            Self::FileIo(_) => owned("file.io.cause"),
            Self::Base64 => owned("decode.base64.cause"),
            Self::Base64Url => owned("decode.base64url.cause"),
            Self::Pem => owned("decode.pem.cause"),
            Self::Utf8 => owned("decode.utf8.cause"),
            Self::UnsupportedAlgorithm { .. } => owned("algorithm.unsupported.cause"),
            Self::LegacyDisabled { .. } => owned("algorithm.legacy.cause"),
            Self::Parse { .. } => owned("parse.pem.cause"),
            Self::WrongKeyType { .. } => owned("key.wrong_type.cause"),
            Self::Io { .. } => owned("io.file.cause"),
            Self::WrongPassword { .. } => owned("password.wrong.cause"),
            Self::Format { .. } => owned("format.rejected.cause"),
            Self::PasswordCost { .. } => owned("password.cost.cause"),
            Self::SaltLength { .. } => owned("derive.salt.cause"),
            Self::EmptyContext { .. } => owned("derive.context.cause"),
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
