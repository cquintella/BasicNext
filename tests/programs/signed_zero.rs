// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A signed zero is a value (math.md, "Numeric model"): constants, unary
//! minus, copies and products keep its sign, the same on both backends.

use super::support::{TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const SIGNED_ZERO_PROGRAM: &str = "FUNCTION Start() AS VOID\n    LET z AS FLOAT = 0.0\n    LET m AS FLOAT = -z\n    LET copy AS FLOAT = m\n    LET single AS FLOAT32 = -0.0\n    PRINT -0.0, m, copy, 0.0 * -1.0, -m, single\nEND FUNCTION\n";

/// IEEE 754: `-(+0)` is `-0`, a copy keeps the bits, `+0 * -1` is `-0`,
/// `-(-0)` is `+0`; `PRINT` writes `-0.0` (0.6.md, "PRINT").
const SIGNED_ZERO_OUTPUT: &str = "-0.0 -0.0 -0.0 -0.0 0.0 -0.0\n";

#[test]
fn signed_zero_survives_constants_negation_and_copies_on_both_backends() {
    let directory = TestDir::new("signed-zero").expect("create directory");
    let source = directory.join("program.bn");
    fs::write(&source, SIGNED_ZERO_PROGRAM).expect("write source");
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
        (
            SIGNED_ZERO_OUTPUT.to_string(),
            SIGNED_ZERO_OUTPUT.to_string()
        )
    );
}
