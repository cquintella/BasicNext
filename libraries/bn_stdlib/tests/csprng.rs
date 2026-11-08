// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Unit tests for CSPRNG utilities in `BNCrypto` (`bncrypto.md`):
//! `RandomInt`, `RandomToken`, `RandomUuidV4`, `UuidV7`, and `RandomPassword`.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

use std::collections::HashSet;

use bn_stdlib::crypto::{
    PasswordGenOptions, random_int, random_password, random_token, random_uuid_v4, uuid_v7,
};
use bn_stdlib::crypto_error::CryptoFailure;

#[test]
fn random_int_validates_bounds_and_samples_uniformly() {
    // Rejects min > max
    assert!(matches!(
        random_int(10, 5),
        Err(CryptoFailure::RandomIntRange { min: 10, max: 5 })
    ));

    // When min == max, always returns min
    assert_eq!(random_int(42, 42).expect("min == max"), 42);
    assert_eq!(random_int(-7, -7).expect("min == max"), -7);

    // Bounded range [1, 5]
    let mut seen = HashSet::new();
    for _ in 0..1000 {
        let val = random_int(1, 5).expect("valid range");
        assert!((1..=5).contains(&val), "val {val} out of bounds");
        seen.insert(val);
    }
    // Over 1000 draws from 5 possibilities, every value should be seen
    assert_eq!(seen.len(), 5);

    // Negative range [-10, -5]
    for _ in 0..100 {
        let val = random_int(-10, -5).expect("valid negative range");
        assert!((-10..=-5).contains(&val));
    }

    // Extremes (i64::MIN to i64::MAX)
    let wide = random_int(i64::MIN, i64::MAX).expect("full range");
    let _ = wide;
}

#[test]
fn random_token_validates_length_and_generates_unpadded_base64url() {
    // Rejects non-positive lengths
    assert!(matches!(
        random_token(0),
        Err(CryptoFailure::RandomTokenLength(0))
    ));
    assert!(matches!(
        random_token(-5),
        Err(CryptoFailure::RandomTokenLength(-5))
    ));

    // Default 32 bytes produces 43 base64url characters without padding
    let t32 = random_token(32).expect("32 bytes");
    assert_eq!(t32.len(), 43);
    assert!(!t32.contains('='));
    assert!(!t32.contains('+'));
    assert!(!t32.contains('/'));

    // 16 bytes produces 22 characters
    let t16 = random_token(16).expect("16 bytes");
    assert_eq!(t16.len(), 22);
    assert!(!t16.contains('='));

    // Successive tokens are unique
    let t32_second = random_token(32).expect("second 32 bytes");
    assert_ne!(t32, t32_second);
}

#[test]
fn random_uuid_v4_format_and_version() {
    let mut seen = HashSet::new();
    for _ in 0..100 {
        let u = random_uuid_v4();
        assert_eq!(u.len(), 36);
        let bytes = u.as_bytes();
        assert_eq!(bytes[8], b'-');
        assert_eq!(bytes[13], b'-');
        assert_eq!(bytes[18], b'-');
        assert_eq!(bytes[23], b'-');

        // Version 4
        assert_eq!(bytes[14], b'4');
        // Variant 8, 9, a, or b
        assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));

        // All other characters must be valid lowercase hex digits
        for (i, &b) in bytes.iter().enumerate() {
            if i == 8 || i == 13 || i == 18 || i == 23 {
                continue;
            }
            assert!(
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
                "invalid hex character at {i}: {b}"
            );
        }
        seen.insert(u);
    }
    assert_eq!(seen.len(), 100);
}

#[test]
fn uuid_v7_format_version_and_ordering() {
    let mut seen = HashSet::new();
    let mut prev = String::new();
    for _ in 0..50 {
        let u = uuid_v7();
        assert_eq!(u.len(), 36);
        let bytes = u.as_bytes();
        assert_eq!(bytes[8], b'-');
        assert_eq!(bytes[13], b'-');
        assert_eq!(bytes[18], b'-');
        assert_eq!(bytes[23], b'-');

        // Version 7
        assert_eq!(bytes[14], b'7');
        // Variant 8, 9, a, or b
        assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'));

        for (i, &b) in bytes.iter().enumerate() {
            if i == 8 || i == 13 || i == 18 || i == 23 {
                continue;
            }
            assert!(
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b),
                "invalid hex character at {i}: {b}"
            );
        }

        // Time ordering: monotonically non-decreasing string representation
        if !prev.is_empty() {
            assert!(u >= prev, "uuid v7 not monotonic: {u} < {prev}");
        }
        prev = u.clone();
        seen.insert(u);
    }
    assert_eq!(seen.len(), 50);
}

#[test]
fn uuid_v7_monotonic_under_high_throughput_counter_rollover() {
    let mut prev = uuid_v7();
    for _ in 0..5000 {
        let u = uuid_v7();
        assert!(u > prev, "uuid v7 strictly monotonic failed: {u} <= {prev}");
        prev = u;
    }
}

#[test]
fn random_password_validates_options_and_includes_requested_classes() {
    // Invalid length
    let bad_opts = PasswordGenOptions {
        length: 0,
        include_uppercase: true,
        include_lowercase: true,
        include_digits: true,
        include_symbols: true,
    };
    assert!(random_password(bad_opts).is_err());

    // All classes disabled
    let no_classes = PasswordGenOptions {
        length: 20,
        include_uppercase: false,
        include_lowercase: false,
        include_digits: false,
        include_symbols: false,
    };
    assert!(random_password(no_classes).is_err());

    // Default options (length 20, all classes)
    let default_pwd = random_password(PasswordGenOptions::default()).expect("default password");
    assert_eq!(default_pwd.len(), 20);

    // Only digits
    let digits_only = PasswordGenOptions {
        length: 30,
        include_uppercase: false,
        include_lowercase: false,
        include_digits: true,
        include_symbols: false,
    };
    let digits_pwd = random_password(digits_only).expect("digits password");
    assert_eq!(digits_pwd.len(), 30);
    assert!(digits_pwd.chars().all(|c| c.is_ascii_digit()));

    // Only uppercase
    let upper_only = PasswordGenOptions {
        length: 25,
        include_uppercase: true,
        include_lowercase: false,
        include_digits: false,
        include_symbols: false,
    };
    let upper_pwd = random_password(upper_only).expect("upper password");
    assert_eq!(upper_pwd.len(), 25);
    assert!(upper_pwd.chars().all(|c| c.is_ascii_uppercase()));

    // Only lowercase
    let lower_only = PasswordGenOptions {
        length: 25,
        include_uppercase: false,
        include_lowercase: true,
        include_digits: false,
        include_symbols: false,
    };
    let lower_pwd = random_password(lower_only).expect("lower password");
    assert_eq!(lower_pwd.len(), 25);
    assert!(lower_pwd.chars().all(|c| c.is_ascii_lowercase()));

    // Only symbols
    let symbols_only = PasswordGenOptions {
        length: 25,
        include_uppercase: false,
        include_lowercase: false,
        include_digits: false,
        include_symbols: true,
    };
    let symbols_pwd = random_password(symbols_only).expect("symbols password");
    assert_eq!(symbols_pwd.len(), 25);
    assert!(
        symbols_pwd
            .chars()
            .all(|c| !c.is_ascii_alphanumeric() && c.is_ascii_graphic())
    );
}
