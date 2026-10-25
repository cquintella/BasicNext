// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dependency gate (bucket 0.6.5 E.S1.a, AC9):
//! Asserts that `bnc` (the native compiler executable) and the default
//! runtime archive (`bn_rt`) have zero dependencies on async web crates
//! (`tokio`, `hyper`, `rustls`).

use std::process::Command;

const FORBIDDEN_PACKAGES: &[&str] = &[
    "tokio",
    "hyper",
    "rustls",
    "rusqlite",
    "libsqlite3-sys",
    "libffi",
    "libffi-sys",
    "libloading",
    "aes-gcm",
    "argon2",
    "chacha20poly1305",
    "ed25519-dalek",
    "p256",
    "ml-kem",
    "ml-dsa",
    "bn_core_dispatch",
    "bn_core_json",
    "bn_core_log",
    "bn_core_math",
    "bn_core_sqlite",
    "bn_stdlib",
    "bn_web",
];

const FORBIDDEN_CRATES_IN_CORE_DIR: &[&str] = &[
    "bn_core_dispatch",
    "bn_core_json",
    "bn_core_log",
    "bn_core_math",
    "bn_core_sqlite",
    "bn_stdlib",
    "bn_web",
];

#[test]
fn bnc_executable_does_not_depend_on_web_runtimes() {
    for entry in std::fs::read_dir("crates").expect("read crates directory") {
        let path = entry.expect("crate entry").path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            assert!(
                !name.starts_with("bn_lib_") && !FORBIDDEN_CRATES_IN_CORE_DIR.contains(&name),
                "BN* library crate `{name}` must reside in `libraries/`, not `crates/`"
            );
        }
    }

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(&cargo)
        .args(["tree", "-p", "bnc", "--prefix", "none"])
        .output()
        .expect("run cargo tree for bnc");

    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for package in stdout.lines() {
        let name = package.split_whitespace().next().unwrap_or_default();
        for &forbidden in FORBIDDEN_PACKAGES {
            assert_ne!(
                name, forbidden,
                "AC9 violation: bnc executable must not depend on `{forbidden}`, but found `{package}`"
            );
        }
    }
}

#[test]
fn default_runtime_archive_does_not_depend_on_web_runtimes() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(&cargo)
        .args(["tree", "-p", "bn_rt", "--prefix", "none"])
        .output()
        .expect("run cargo tree for bn_rt");

    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for package in stdout.lines() {
        let name = package.split_whitespace().next().unwrap_or_default();
        assert_ne!(
            name, "serde_json",
            "AC9 violation: bn_rt runtime archive must not depend on `serde_json`, but found `{package}`"
        );
        for &forbidden in FORBIDDEN_PACKAGES {
            assert_ne!(
                name, forbidden,
                "AC9 violation: bn_rt runtime archive must not depend on `{forbidden}`, but found `{package}`"
            );
        }
    }
}
