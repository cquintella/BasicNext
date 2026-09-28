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
}

impl CryptoFailure {
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::Open(_) | Self::Decapsulate => crypto::AUTHENTICATION_FAILED,
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
            Self::Key(operation) | Self::Seed(operation) => operation,
            Self::Slice { .. } => "BNCrypto.Slice",
            Self::EncapsulationKey => "BNCrypto.MlKemEncapsulate",
            Self::Decapsulate => "BNCrypto.MlKemDecapsulate",
            Self::SigningKey => "BNCrypto.MlDsaSign",
        }
    }

    /// `Error.Message`: what failed, naming the input where it identifies it.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::Hex(text) => {
                let shown: String = text.chars().take(64).collect();
                let more = if text.chars().count() > 64 { "…" } else { "" };
                format!("cannot decode \"{shown}{more}\" as hexadecimal")
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
            Self::Open(_) => "the authentication tag does not verify: the ciphertext, AAD, \
                              key, or nonce differs from the ones that sealed it"
                .into(),
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
