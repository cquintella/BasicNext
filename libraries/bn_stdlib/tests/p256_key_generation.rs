// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `GenerateKey(ALG_ECDSA_P256)` keeps only scalars in 1 through n - 1
//! (`ongoing/issues.md`, A14): the acceptance rule at the curve-order
//! boundary, and generated keys that always sign.

use bn_stdlib::crypto::{ALG_ECDSA_P256, generate_key_material, is_p256_scalar, p256_sign};

/// The P-256 group order n, big-endian (SEC 2, section 2.4.2).
const P256_ORDER: [u8; 32] = [
    0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xbc, 0xe6, 0xfa, 0xad, 0xa7, 0x17, 0x9e, 0x84, 0xf3, 0xb9, 0xca, 0xc2, 0xfc, 0x63, 0x25, 0x51,
];

/// Keys generated in one run of the property test.
const GENERATED_KEYS: usize = 256;

fn order_minus(amount: u8) -> [u8; 32] {
    let mut scalar = P256_ORDER;
    scalar[31] -= amount;
    scalar
}

#[test]
fn scalar_acceptance_follows_the_curve_order() {
    let mut one = [0u8; 32];
    one[31] = 1;
    let mut order_plus_one = P256_ORDER;
    order_plus_one[31] += 1;
    assert!(is_p256_scalar(&one));
    assert!(is_p256_scalar(&order_minus(1)));
    assert!(!is_p256_scalar(&[0u8; 32]));
    assert!(!is_p256_scalar(&P256_ORDER));
    assert!(!is_p256_scalar(&order_plus_one));
    assert!(!is_p256_scalar(&[0xff; 32]));
    // Only the exact scalar length counts, never a padded shorter slice.
    assert!(!is_p256_scalar(&order_minus(1)[1..]));
}

#[test]
fn generated_p256_keys_are_valid_scalars_that_sign() {
    for _ in 0..GENERATED_KEYS {
        let key = generate_key_material(ALG_ECDSA_P256).expect("system entropy");
        assert!(is_p256_scalar(&key));
        assert!(p256_sign(&key, b"abc").is_some());
    }
}
