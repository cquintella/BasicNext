// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `HashPasswordWith`, `NeedsRehash`, `DeriveKey` and `DeriveSubkey`
// (bncrypto.md, "Passwords and Key Derivation").

use bn_stdlib::crypto_error::CryptoFailure;
use bn_stdlib::crypto_password::{
    PasswordCost, bn_derive_key, bn_derive_subkey, bn_hash_password, bn_hash_password_with,
    bn_hkdf_sha256, bn_needs_rehash, bn_verify_password,
};

const FLOOR: PasswordCost = PasswordCost {
    memory_kib: 19456,
    iterations: 2,
    parallelism: 1,
};

#[test]
fn hash_password_with_records_its_cost_and_verifies() {
    let stronger = PasswordCost {
        iterations: 3,
        ..FLOOR
    };
    let phc = bn_hash_password_with("hunter2", stronger).expect("hash");
    assert!(phc.starts_with("$argon2id$v=19$m=19456,t=3,p=1$"), "{phc}");
    assert!(bn_verify_password("hunter2", &phc));
    assert!(!bn_verify_password("hunter3", &phc));
}

#[test]
fn a_cost_below_the_floor_is_refused() {
    for weaker in [
        PasswordCost {
            memory_kib: 19455,
            ..FLOOR
        },
        PasswordCost {
            iterations: 1,
            ..FLOOR
        },
        PasswordCost {
            parallelism: 0,
            ..FLOOR
        },
    ] {
        assert!(
            matches!(
                bn_hash_password_with("pw", weaker),
                Err(CryptoFailure::PasswordCost { .. })
            ),
            "{weaker:?}"
        );
    }
}

#[test]
fn needs_rehash_compares_with_the_target_and_fails_safe() {
    let phc = bn_hash_password("pw").expect("hash");
    assert!(!bn_needs_rehash(&phc, FLOOR));
    let target = PasswordCost {
        iterations: 3,
        ..FLOOR
    };
    assert!(bn_needs_rehash(&phc, target));
    assert!(bn_needs_rehash("not a phc string", FLOOR));
    let argon2i = phc.replacen("$argon2id$", "$argon2i$", 1);
    assert!(bn_needs_rehash(&argon2i, FLOOR));
    let old_version = phc.replacen("$v=19$", "$v=16$", 1);
    assert!(bn_needs_rehash(&old_version, FLOOR));
}

#[test]
fn derive_key_is_deterministic_and_needs_a_16_byte_salt() {
    let salt = [3_u8; 16];
    let first = bn_derive_key("correct horse", &salt).expect("derive");
    let second = bn_derive_key("correct horse", &salt).expect("derive");
    assert_eq!(first.len(), 32);
    assert_eq!(first, second);
    assert_ne!(
        first,
        bn_derive_key("correct horsf", &salt).expect("derive")
    );
    assert!(matches!(
        bn_derive_key("correct horse", &salt[..15]),
        Err(CryptoFailure::SaltLength { .. })
    ));
}

#[test]
fn derive_subkey_is_labelled_hkdf_and_refuses_an_empty_context() {
    let master = [9_u8; 32];
    let subkey = bn_derive_subkey(&master, "session").expect("derive");
    let mut info = b"BNC-SUBKEY\0".to_vec();
    info.extend_from_slice(b"session");
    assert_eq!(
        *subkey,
        bn_hkdf_sha256(&master, &[], &info, 32).expect("hkdf")
    );
    assert_ne!(subkey, bn_derive_subkey(&master, "other").expect("derive"));
    assert!(matches!(
        bn_derive_subkey(&master, ""),
        Err(CryptoFailure::EmptyContext { .. })
    ));
}
