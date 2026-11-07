// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// `EncryptFileWithPassword` / `DecryptFileWithPassword`: the PASSWORD section
// of a `BNC1` stream (bncrypto.md, "Envelope Format").

#![allow(
    clippy::disallowed_methods,
    reason = "tests read back the fixture files they write"
)]

use bn_stdlib::FsPolicy;
use bn_stdlib::crypto::{bn_decrypt_file, bn_encrypt_file};
use bn_stdlib::crypto_error::CryptoFailure;
use bn_stdlib::crypto_file::{bn_decrypt_file_with_password, bn_encrypt_file_with_password};
use std::fs;
use std::path::PathBuf;

/// Offset of the flags byte, then of the KDF section that follows it.
const FLAGS_AT: usize = 6;
const KDF_AT: usize = FLAGS_AT + 1;
const STREAM_AND_PASSWORD: u8 = 0b101;

fn temp_path(name: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    std::env::temp_dir().join(format!(
        "bn_pwfile_{name}_{nonce}_{}.tmp",
        std::process::id()
    ))
}

struct Paths {
    plain: PathBuf,
    sealed: PathBuf,
    opened: PathBuf,
}

impl Paths {
    fn new(name: &str, content: &[u8]) -> Self {
        let paths = Self {
            plain: temp_path(&format!("{name}_plain")),
            sealed: temp_path(&format!("{name}_sealed")),
            opened: temp_path(&format!("{name}_opened")),
        };
        fs::write(&paths.plain, content).expect("write the plain file");
        paths
    }
}

impl Drop for Paths {
    fn drop(&mut self) {
        for path in [&self.plain, &self.sealed, &self.opened] {
            let _ = fs::remove_file(path);
        }
    }
}

#[test]
fn a_password_file_round_trips_across_segments() {
    let content: Vec<u8> = (0_usize..150 * 1024)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect();
    let paths = Paths::new("roundtrip", &content);
    let policy = FsPolicy::unrestricted();
    bn_encrypt_file_with_password(&policy, "correct horse", &paths.plain, &paths.sealed)
        .expect("encrypt");
    let sealed = fs::read(&paths.sealed).expect("read the sealed file");
    assert_eq!(sealed[FLAGS_AT], STREAM_AND_PASSWORD);
    // The floor cost: m=19456, t=2, p=1, little-endian.
    assert_eq!(sealed[KDF_AT..KDF_AT + 4], 19456_u32.to_le_bytes());
    assert_eq!(sealed[KDF_AT + 4..KDF_AT + 8], 2_u32.to_le_bytes());
    assert_eq!(sealed[KDF_AT + 8..KDF_AT + 12], 1_u32.to_le_bytes());
    bn_decrypt_file_with_password(&policy, "correct horse", &paths.sealed, &paths.opened)
        .expect("decrypt");
    assert_eq!(
        fs::read(&paths.opened).expect("read the opened file"),
        content
    );
}

#[test]
fn a_wrong_password_is_wrong_password_and_leaves_no_output() {
    let paths = Paths::new("wrong", b"secret notes");
    let policy = FsPolicy::unrestricted();
    bn_encrypt_file_with_password(&policy, "right", &paths.plain, &paths.sealed).expect("encrypt");
    assert!(matches!(
        bn_decrypt_file_with_password(&policy, "wrong", &paths.sealed, &paths.opened),
        Err(CryptoFailure::WrongPassword { .. })
    ));
    assert!(!paths.opened.exists());
}

#[test]
fn a_recorded_cost_other_than_the_floor_is_refused() {
    let paths = Paths::new("cost", b"secret notes");
    let policy = FsPolicy::unrestricted();
    bn_encrypt_file_with_password(&policy, "pw", &paths.plain, &paths.sealed).expect("encrypt");
    let mut sealed = fs::read(&paths.sealed).expect("read the sealed file");
    // A hostile header asks for 4 GiB of memory.
    sealed[KDF_AT..KDF_AT + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&paths.sealed, &sealed).expect("write the changed file");
    assert!(matches!(
        bn_decrypt_file_with_password(&policy, "pw", &paths.sealed, &paths.opened),
        Err(CryptoFailure::PasswordCost { .. })
    ));
    assert!(!paths.opened.exists());
}

#[test]
fn key_and_password_files_do_not_open_each_other() {
    let policy = FsPolicy::unrestricted();
    let by_password = Paths::new("by_password", b"a");
    bn_encrypt_file_with_password(&policy, "pw", &by_password.plain, &by_password.sealed)
        .expect("encrypt");
    assert_eq!(
        bn_decrypt_file(&policy, &[1; 32], &by_password.sealed, &by_password.opened),
        Err(CryptoFailure::EnvelopeFormat)
    );
    let by_key = Paths::new("by_key", b"b");
    bn_encrypt_file(&policy, &[1; 32], &by_key.plain, &by_key.sealed).expect("encrypt");
    assert_eq!(
        bn_decrypt_file_with_password(&policy, "pw", &by_key.sealed, &by_key.opened),
        Err(CryptoFailure::EnvelopeFormat)
    );
}
