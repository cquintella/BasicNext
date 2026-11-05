// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `examples/bncrypto_tour.bn` is both the usage guide of `BNCrypto` and a
//! test of it: every line is `PASS` or `FAIL`, the published test vectors
//! included. Both backends must print the same lines and no `FAIL`.

use super::support::{TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
const TOUR: &str = "examples/bncrypto_tour.bn";
const LAST_LINE: &str = "BNCrypto tour done";
/// Checks the tour makes; a removed check must be removed here on purpose.
const CHECKS: usize = 134;

fn assert_all_pass(backend: &str, stdout: &str) {
    let failures: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.starts_with("PASS"))
        .collect();
    assert_eq!(
        failures,
        vec![LAST_LINE],
        "{backend}: lines that are not PASS"
    );
    let passes = stdout
        .lines()
        .filter(|line| line.starts_with("PASS"))
        .count();
    assert_eq!(passes, CHECKS, "{backend}: number of checks");
}

#[test]
fn bncrypto_tour_passes_on_both_backends() {
    let source = fs::read_to_string(TOUR).expect("read tour");
    // The tour writes and deletes files in its working directory: one per backend.
    let interpreted_dir = TestDir::new("crypto-tour-bni").expect("create directory");
    let program = interpreted_dir.join("tour.bn");
    fs::write(&program, &source).expect("write tour");
    let interpreted = run(
        bni()
            .arg("run")
            .arg(&program)
            .current_dir(interpreted_dir.path()),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run interpreter");
    let interpreted = String::from_utf8_lossy(&interpreted.stdout).into_owned();
    assert_all_pass("bni", &interpreted);

    let compiled_dir = TestDir::new("crypto-tour-bnc").expect("create directory");
    let program = compiled_dir.join("tour.bn");
    fs::write(&program, &source).expect("write tour");
    let artifact = compiled_dir.join(format!("tour{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(&program)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "bnc: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let compiled = run(
        Command::new(&artifact).current_dir(compiled_dir.path()),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run compiled artifact");
    let compiled = String::from_utf8_lossy(&compiled.stdout).into_owned();
    assert_all_pass("bnc", &compiled);
    assert_eq!(interpreted, compiled, "bni and bnc print different lines");
}
