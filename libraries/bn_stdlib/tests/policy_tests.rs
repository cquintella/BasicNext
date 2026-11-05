// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `BNCrypto.Policy`: fipsOnly refuses exactly the non-approved algorithm of
// each family; the default policy and allowLegacy refuse nothing.

use bn_stdlib::crypto_asymmetric::{KEY_ALG_ECDSA_P256, KEY_ALG_ED25519, KEY_ALG_X25519};
use bn_stdlib::crypto_hash::{
    HASH_BLAKE2B, HASH_BLAKE3, HASH_SHA3_256, HASH_SHA3_512, HASH_SHA256, HASH_SHA384, HASH_SHA512,
    HASH_SHA512_256,
};
use bn_stdlib::crypto_mac::{MAC_BLAKE3_KEYED, MAC_HMAC_SHA256, MAC_HMAC_SHA384, MAC_HMAC_SHA512};
use bn_stdlib::crypto_policy::CryptoPolicy;
use bn_types::error_codes::crypto::UNSUPPORTED_ALGORITHM;

const FIPS: CryptoPolicy = CryptoPolicy {
    allow_legacy: false,
    fips_only: true,
};
const LEGACY: CryptoPolicy = CryptoPolicy {
    allow_legacy: true,
    fips_only: false,
};

fn code(result: Result<(), bn_stdlib::crypto_error::CryptoFailure>) -> Option<i32> {
    result.err().map(|failure| failure.code())
}

#[test]
fn fips_only_refuses_the_non_approved_algorithms() {
    for refused in [HASH_BLAKE2B, HASH_BLAKE3] {
        assert_eq!(code(FIPS.admits_hash(refused)), Some(UNSUPPORTED_ALGORITHM));
    }
    assert_eq!(
        code(FIPS.admits_mac(MAC_BLAKE3_KEYED)),
        Some(UNSUPPORTED_ALGORITHM)
    );
    assert_eq!(
        code(FIPS.admits_key_pair(KEY_ALG_X25519)),
        Some(UNSUPPORTED_ALGORITHM)
    );
}

#[test]
fn fips_only_admits_the_approved_algorithms() {
    for approved in [
        HASH_SHA256,
        HASH_SHA384,
        HASH_SHA512,
        HASH_SHA512_256,
        HASH_SHA3_256,
        HASH_SHA3_512,
    ] {
        assert_eq!(code(FIPS.admits_hash(approved)), None);
    }
    for approved in [MAC_HMAC_SHA256, MAC_HMAC_SHA384, MAC_HMAC_SHA512] {
        assert_eq!(code(FIPS.admits_mac(approved)), None);
    }
    for approved in [KEY_ALG_ED25519, KEY_ALG_ECDSA_P256] {
        assert_eq!(code(FIPS.admits_key_pair(approved)), None);
    }
}

#[test]
fn the_default_policy_and_allow_legacy_refuse_nothing() {
    for policy in [CryptoPolicy::default(), LEGACY] {
        assert_eq!(code(policy.admits_hash(HASH_BLAKE3)), None);
        assert_eq!(code(policy.admits_mac(MAC_BLAKE3_KEYED)), None);
        assert_eq!(code(policy.admits_key_pair(KEY_ALG_X25519)), None);
    }
}
