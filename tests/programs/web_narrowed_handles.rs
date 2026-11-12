// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `BNWeb` handle returned as `T OR Error` keeps its value after the
//! narrowing on both backends. `bnc` once read the narrowed `Headers` and
//! `ServerOptions` from the pointer slot of the result, so the handle was 0.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const PROGRAM: &str = "tests/host/web_narrowed_handles.bn";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const EXPECTED: &str = "headers add FALSE 1\nstarted TRUE\nstopped TRUE\n";

#[test]
fn a_narrowed_web_handle_is_the_same_handle_on_both_backends() {
    let source = workspace_root().join(PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        EXPECTED,
        "bni"
    );
    let directory = TestDir::new("web-narrowed-handles").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), EXPECTED, "bnc");
}
