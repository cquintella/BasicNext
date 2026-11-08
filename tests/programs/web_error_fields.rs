// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A `BNWeb.Client` failure carries `Code`, `Operation`, `Message` and
//! `Cause` on both backends: a native `Error` used to arrive with the three
//! texts empty.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const PROGRAM: &str = "tests/host/web_client_error_fields.bn";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const EXPECTED: &str = "code TRUE\n\
operation BNWeb.Client.Request\n\
message private or local destination requires explicit opt-in\n\
cause private or local destination requires explicit opt-in\n";

#[test]
fn a_web_client_error_has_its_four_fields_on_both_backends() {
    let source = workspace_root().join(PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        EXPECTED,
        "bni"
    );
    let directory = TestDir::new("web-error-fields").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), EXPECTED, "bnc");
}
