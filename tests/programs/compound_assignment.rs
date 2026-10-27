// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Writes to a place evaluate it once and first, then the value (0.6.md,
//! "Evaluation order"); `a op= b` is `a = a op b` ("Compound assignment").
//! The same on both backends.

use super::support::{TestDir, bnc, bni, run};
use std::fs;
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

/// `Idx` and `Val` print when they run, so the output shows the order and
/// the count of evaluations.
const TRACED_PLACE_AND_VALUE: &str = "FUNCTION Idx() AS INTEGER\n    PRINT \"idx\"\n    RETURN 1\nEND FUNCTION\n\nFUNCTION Val() AS INTEGER\n    PRINT \"val\"\n    RETURN 10\nEND FUNCTION\n\n";

/// Each case: a label, the body of `Start` (after `TRACED_PLACE_AND_VALUE`
/// and `LET v AS INTEGER[3] = [1, 2, 3]`), and the output derived from the
/// spec by hand.
type Case = (&'static str, &'static str, &'static str);

const COMPOUND_CASES: [Case; 2] = [
    (
        "narrowed alternative: 5 + 1 stored into `a`",
        "    LET a AS INTEGER OR NULL = 5\n    IF a IS INTEGER THEN\n        a += 1\n    END IF\n    PRINT a\n",
        "6\n",
    ),
    (
        "indexed place: the index once and first, then the value; v[1] = 2 + 10",
        "    v[Idx()] += Val()\n    PRINT v[1]\n",
        "idx\nval\n12\n",
    ),
];

const PLACE_ORDER_CASES: [Case; 2] = [
    (
        "`++` expression: the index once; prints the old 2, stores 3",
        "    PRINT v[Idx()]++\n    PRINT v[1]\n",
        "idx\n2\n3\n",
    ),
    (
        "plain `=`: the place first, then the value; v[1] = 10",
        "    v[Idx()] = Val()\n    PRINT v[1]\n",
        "idx\nval\n10\n",
    ),
];

fn program(body: &str) -> String {
    format!(
        "{TRACED_PLACE_AND_VALUE}FUNCTION Start() AS VOID\n    LET v AS INTEGER[3] = [1, 2, 3]\n{body}END FUNCTION\n"
    )
}

fn output_text(success: bool, stdout: &[u8], stderr: &[u8], backend: &str) -> String {
    if success {
        String::from_utf8_lossy(stdout).into_owned()
    } else {
        format!("{backend} failed: {}", String::from_utf8_lossy(stderr))
    }
}

/// The cases whose output differs from the expected text, on either backend.
fn mismatches(cases: &[Case]) -> Vec<String> {
    let mut mismatches = Vec::new();
    for (case, body, expected) in cases {
        let directory = TestDir::new("place-order").expect("create directory");
        let source = directory.join("program.bn");
        fs::write(&source, program(body)).expect("write source");
        let interpreted =
            run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run interpreter");
        let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
        let built = bnc()
            .arg(&source)
            .arg("-o")
            .arg(&artifact)
            .output()
            .expect("run bnc");
        let native = if built.status.success() {
            let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT)
                .expect("run compiled artifact");
            output_text(
                compiled.status.success(),
                &compiled.stdout,
                &compiled.stderr,
                "bnc",
            )
        } else {
            output_text(false, &[], &built.stderr, "bnc build")
        };
        let interpreted = output_text(
            interpreted.status.success(),
            &interpreted.stdout,
            &interpreted.stderr,
            "bni",
        );
        for (backend, actual) in [("bni", interpreted), ("bnc", native)] {
            if actual != *expected {
                mismatches.push(format!("{case} [{backend}]: {actual:?} != {expected:?}"));
            }
        }
    }
    mismatches
}

#[test]
fn compound_assignment_matches_the_spec_on_both_backends() {
    let mismatches = mismatches(&COMPOUND_CASES);
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}

#[test]
fn every_write_evaluates_the_place_once_and_first_on_both_backends() {
    let mismatches = mismatches(&PLACE_ORDER_CASES);
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
