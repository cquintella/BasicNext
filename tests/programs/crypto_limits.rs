// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNCrypto` bounds that return `Error` instead of stopping the program,
//! identically on both backends (bncrypto.md; bucket 0.6.11, S6.8): a
//! `RandomBytes` length outside 0 through 1 MiB, and a `Key` erased with
//! `Zeroize()`.

use super::support::{TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

const PROGRAM: &str = "IMPORT BNCrypto AS Crypto

FUNCTION Start() AS VOID
    LET edge AS Crypto.Bytes OR Error = Crypto.RandomBytes(1048576)
    LET big AS Crypto.Bytes OR Error = Crypto.RandomBytes(1048577)
    LET negative AS Crypto.Bytes OR Error = Crypto.RandomBytes(-1)
    IF edge IS Crypto.Bytes THEN
        PRINT edge.Length()
    END IF
    IF big IS Error THEN
        PRINT big.Code = Crypto.INVALID_ARGUMENT, big.Message
    END IF
    IF negative IS Error THEN
        PRINT negative.Code = Crypto.INVALID_ARGUMENT, negative.Message
    END IF

    LET key AS Crypto.Key OR Error = Crypto.GenerateKey(Crypto.ALG_AES_GCM_256)
    LET nonce AS Crypto.Bytes OR Error = Crypto.GenerateNonce(Crypto.ALG_AES_GCM_256)
    IF key IS Error OR nonce IS Error THEN
        PRINT \"setup failed\"
        RETURN
    END IF
    key.Zeroize()
    LET sealed AS Crypto.Bytes OR Error = Crypto.SealAesGcm(key, nonce, Crypto.FromText(\"abc\"), Crypto.FromText(\"hdr\"))
    IF sealed IS Error THEN
        PRINT sealed.Code = Crypto.INVALID_ARGUMENT, sealed.Message
    END IF
    PRINT \"still running\"
END FUNCTION
";

const EXPECTED: &str = "1048576
TRUE cannot generate 1048577 random bytes
TRUE cannot generate -1 random bytes
TRUE key material has already been zeroized
still running
";

#[test]
fn crypto_limits_return_errors_on_both_backends() {
    let directory = TestDir::new("crypto-limits").expect("create directory");
    let source = directory.join("program.bn");
    fs::write(&source, PROGRAM).expect("write source");
    let interpreted =
        run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run interpreter");
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(&source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "bnc: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let compiled =
        run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run compiled artifact");
    assert_eq!(
        (
            String::from_utf8_lossy(&interpreted.stdout).into_owned(),
            String::from_utf8_lossy(&compiled.stdout).into_owned()
        ),
        (EXPECTED.to_string(), EXPECTED.to_string())
    );
}
