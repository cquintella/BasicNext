// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Dependency gate (bucket 0.6.5 E.S1.a, AC9):
//! Asserts that `bnc` (the native compiler executable) and the default
//! runtime archive (`bn_rt`) have zero dependencies on async web crates
//! (`tokio`, `hyper`, `rustls`).

use std::process::Command;

const FORBIDDEN_PACKAGES: &[&str] = &["tokio", "hyper", "rustls"];

#[test]
fn bnc_executable_does_not_depend_on_web_runtimes() {
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
        for &forbidden in FORBIDDEN_PACKAGES {
            assert_ne!(
                name, forbidden,
                "AC9 violation: bn_rt runtime archive must not depend on `{forbidden}`, but found `{package}`"
            );
        }
    }
}
