// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Crypto.Bytes` values made by `BNWeb` and by `BNCrypto` live in one store:
//! on `bni` the two libraries once kept separate tables whose handle numbers
//! collided, so a web body read the bytes of a crypto value.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const PROGRAM: &str = "tests/host/web_crypto_bytes_shared.bn";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const EXPECTED: &str = "empty body length 0\ntext length 5 68656c6c6f\n";

#[test]
fn web_and_crypto_bytes_do_not_share_a_handle_on_either_backend() {
    let source = workspace_root().join(PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        EXPECTED,
        "bni"
    );
    let directory = TestDir::new("web-crypto-bytes").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), EXPECTED, "bnc");
}
