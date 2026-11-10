// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Socket quota parity fixtures: exhausting process socket quota causes
//! subsequent `BNWeb.Client` connect and `BNWeb.Server.Start` to return `Web.LIMIT`.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::process::Command;
use std::time::Duration;

const CLIENT_PROGRAM: &str = "tests/host/net_quota_client.bn";
const SERVER_PROGRAM: &str = "tests/host/net_quota_server.bn";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const CLIENT_EXPECTED: &str = "client limit: TRUE 4\n";
const SERVER_EXPECTED: &str = "server limit: TRUE 4\n";

#[test]
fn exhausting_socket_quota_fails_client_connect_with_web_limit_on_both_backends() {
    let source = workspace_root().join(CLIENT_PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        CLIENT_EXPECTED,
        "bni"
    );
    let directory = TestDir::new("net-quota-client").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(&mut Command::new(&artifact), None, PROCESS_TIMEOUT).expect("run artifact");
    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        CLIENT_EXPECTED,
        "bnc"
    );
}

#[test]
fn exhausting_socket_quota_fails_server_start_with_web_limit() {
    let source = workspace_root().join(SERVER_PROGRAM);
    let interpreted = run(bni().arg("run").arg(&source), None, PROCESS_TIMEOUT).expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        SERVER_EXPECTED,
        "bni"
    );
}
