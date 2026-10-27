// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `examples/math_expressions.bn` checks operator precedence and well-known
//! formulas against hand-derived and IEEE 754 values (0.6.md, "Operators").
//! Every check passes, and both backends print the same text.

use super::support::{TestDir, bnc, bni, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

const EXAMPLE: &str = "examples/math_expressions.bn";

/// The number of `Check*` calls in the example; a dropped line fails.
const CHECK_COUNT: usize = 52;

const PASS_PREFIX: &str = "PASS";

fn checks_report(stdout: &str) -> Result<(), String> {
    let results = stdout
        .lines()
        .filter(|line| !line.starts_with("---"))
        .collect::<Vec<_>>();
    let failures = results
        .iter()
        .filter(|line| !line.starts_with(PASS_PREFIX))
        .collect::<Vec<_>>();
    if !failures.is_empty() {
        return Err(format!("failed checks: {failures:#?}"));
    }
    if results.len() != CHECK_COUNT {
        return Err(format!(
            "{} checks ran, {CHECK_COUNT} expected",
            results.len()
        ));
    }
    Ok(())
}

#[test]
fn math_expressions_pass_on_both_backends() {
    let source = workspace_root().join(EXAMPLE);
    let interpreted =
        run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run interpreter");
    let interpreted_stdout = String::from_utf8_lossy(&interpreted.stdout).into_owned();

    let directory = TestDir::new("math-expressions").expect("create directory");
    let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
    let built = bnc()
        .arg(&source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .expect("run bnc");
    let native_stdout = if built.status.success() {
        let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT)
            .expect("run compiled artifact");
        String::from_utf8_lossy(&compiled.stdout).into_owned()
    } else {
        format!(
            "bnc build failed: {}",
            String::from_utf8_lossy(&built.stderr)
        )
    };

    let mut problems = Vec::new();
    if let Err(report) = checks_report(&interpreted_stdout) {
        problems.push(format!("bni: {report}"));
    }
    if let Err(report) = checks_report(&native_stdout) {
        problems.push(format!("bnc: {report}"));
    }
    if interpreted_stdout != native_stdout {
        problems.push("bni and bnc print different text".to_string());
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
