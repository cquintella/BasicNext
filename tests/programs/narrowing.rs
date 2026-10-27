// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An assignment reads its value before it writes the target: inside an
//! `IS` branch, `a = a + 1` uses the narrowed `a` and stores the sum, the same
//! on both backends (0.6.md, "`IS` narrowing").

use super::support::{TestDir, bni, compile_native, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const SELF_ASSIGNMENT_PROGRAM: &str = "FUNCTION Start() AS VOID\n    LET a AS INTEGER OR NULL = 5\n    IF a IS INTEGER THEN\n        a = a + 1\n    END IF\n    PRINT a\nEND FUNCTION\n";

/// 5 + 1, stored into `a`.
const SELF_ASSIGNMENT_OUTPUT: &str = "6\n";

#[test]
fn self_assignment_in_an_is_branch_stores_the_sum_on_both_backends() {
    let directory = TestDir::new("narrowing").expect("create directory");
    let source = directory.join("program.bn");
    fs::write(&source, SELF_ASSIGNMENT_PROGRAM).expect("write source");

    let interpreted =
        run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run interpreter");
    assert!(
        interpreted.status.success(),
        "bni: {}",
        String::from_utf8_lossy(&interpreted.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        SELF_ASSIGNMENT_OUTPUT
    );

    let artifact = compile_native(&source, &directory);
    let compiled =
        run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run compiled artifact");
    assert!(
        compiled.status.success(),
        "native: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        SELF_ASSIGNMENT_OUTPUT
    );
}
