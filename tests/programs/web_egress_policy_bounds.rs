// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Web.EgressPolicy.New` returns `EgressPolicy OR Error` on both backends:
//! a setting outside its bounds is an `Error` with `INVALID_ARGUMENT` that
//! the program handles (the interpreter used to end the program; a native
//! program got `EGRESS_DENIED`), and a redirect limit of 0 is valid.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const PROGRAM: &str = "tests/host/web_egress_policy_bounds.bn";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const EXPECTED: &str = "zero redirects policy\n\
ten redirects policy\n\
negative redirects TRUE BNWeb.EgressPolicy.New\n\
eleven redirects TRUE BNWeb.EgressPolicy.New\n\
zero deadline TRUE BNWeb.EgressPolicy.New\n\
negative deadline TRUE BNWeb.EgressPolicy.New\n\
long deadline TRUE BNWeb.EgressPolicy.New\n\
ftp scheme TRUE BNWeb.EgressPolicy.New\n\
port zero TRUE BNWeb.EgressPolicy.New\n\
bad CIDR TRUE BNWeb.EgressPolicy.New\n\
after\n";

#[test]
fn egress_policy_bounds_are_invalid_arguments_on_both_backends() {
    let source = workspace_root().join(PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        EXPECTED,
        "bni: {}",
        String::from_utf8_lossy(&interpreted.stderr)
    );
    let directory = TestDir::new("web-egress-bounds").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), EXPECTED, "bnc");
}
