// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `STRUCT` literal as an element of a vector literal (0.6.md, "STRUCT
//! literals"; bucket 0.6.11, S5.5) builds the same values on both backends.

use super::support::{TestDir, bnc, bni, run};
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const FIXTURE: &str = "tests/grammar/valid/struct-literal-vector-elements.bn";
const EXPECTED: &str = "3 4\n2 1\nroute 5\n";

#[test]
fn struct_literals_fill_vector_elements_on_both_backends() {
    let interpreted =
        run(bni().arg("run").arg(FIXTURE), None, PROCESS_TIMEOUT).expect("run interpreter");
    let directory = TestDir::new("struct-vector-literals").expect("create directory");
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(FIXTURE)
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
