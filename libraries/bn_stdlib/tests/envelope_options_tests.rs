// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `EncryptWith` / `DecryptWith`: the `BNC1` flags byte, the KEYID section and
// the caller's AAD (bncrypto.md, "Envelope Format").

use bn_stdlib::crypto::{bn_decrypt_envelope, bn_encrypt_envelope};
use bn_stdlib::crypto_envelope::{
    ENVELOPE_FLAG_KEYID, ENVELOPE_FLAG_PASSWORD, ENVELOPE_FLAG_STREAM, EnvelopeOptions,
    bn_decrypt_envelope_with, bn_encrypt_envelope_with,
};
use bn_stdlib::crypto_error::CryptoFailure;
use bn_stdlib::crypto_secret_abi::secret_key_id_bytes;

const KEY: [u8; 32] = [7; 32];
const OTHER_KEY: [u8; 32] = [8; 32];
/// Offset of the flags byte: magic (4), version (1), algorithm (1).
const FLAGS_AT: usize = 6;
const KEY_ID_AT: usize = FLAGS_AT + 1;

#[test]
fn default_options_seal_what_encrypt_seals() {
    let sealed =
        bn_encrypt_envelope_with(&KEY, b"note", &EnvelopeOptions::default()).expect("seal");
    assert_eq!(sealed[FLAGS_AT], 0);
    assert_eq!(bn_decrypt_envelope(&KEY, &sealed).expect("open"), b"note");
    let plain = bn_encrypt_envelope(&KEY, b"note").expect("seal");
    assert_eq!(plain.len(), sealed.len());
}

#[test]
fn the_aad_is_bound_to_the_envelope() {
    let options = EnvelopeOptions {
        aad: b"order-17",
        include_key_id: false,
    };
    let sealed = bn_encrypt_envelope_with(&KEY, b"note", &options).expect("seal");
    assert_eq!(
        bn_decrypt_envelope_with(&KEY, &sealed, b"order-17").expect("open"),
        b"note"
    );
    assert_eq!(
        bn_decrypt_envelope_with(&KEY, &sealed, b"order-18"),
        Err(CryptoFailure::EnvelopeAuth)
    );
    assert_eq!(
        bn_decrypt_envelope(&KEY, &sealed),
        Err(CryptoFailure::EnvelopeAuth)
    );
}

#[test]
fn the_key_id_section_follows_the_flags_and_names_the_key() {
    let options = EnvelopeOptions {
        aad: b"",
        include_key_id: true,
    };
    let sealed = bn_encrypt_envelope_with(&KEY, b"note", &options).expect("seal");
    assert_eq!(sealed[FLAGS_AT], ENVELOPE_FLAG_KEYID);
    assert_eq!(
        sealed[KEY_ID_AT..KEY_ID_AT + 32],
        secret_key_id_bytes(&KEY)[..]
    );
    assert_eq!(bn_decrypt_envelope(&KEY, &sealed).expect("open"), b"note");
    assert_eq!(
        bn_decrypt_envelope(&OTHER_KEY, &sealed),
        Err(CryptoFailure::EnvelopeAuth)
    );
    let short = &sealed[..KEY_ID_AT + 16];
    assert_eq!(
        bn_decrypt_envelope(&KEY, short),
        Err(CryptoFailure::EnvelopeFormat)
    );
}

#[test]
fn a_flag_a_single_envelope_cannot_carry_is_a_format_error() {
    let sealed =
        bn_encrypt_envelope_with(&KEY, b"note", &EnvelopeOptions::default()).expect("seal");
    for flag in [
        ENVELOPE_FLAG_STREAM,
        ENVELOPE_FLAG_PASSWORD,
        0b1000,
        0b1000_0000,
    ] {
        let mut changed = sealed.clone();
        changed[FLAGS_AT] = flag;
        assert_eq!(
            bn_decrypt_envelope(&KEY, &changed),
            Err(CryptoFailure::EnvelopeFormat),
            "flags {flag:#010b}"
        );
    }
}
