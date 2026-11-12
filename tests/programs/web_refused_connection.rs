// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A refused connection is `HTTP_FAILED` on both backends. The code was once
//! chosen by searching the system's message for words, and "Connection
//! refused" fell through to `INVALID_ARGUMENT`.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

const PROGRAM: &str = "tests/host/web_refused_connection.bn";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const EXPECTED: &str = "TRUE BNWeb.Client.Get TRUE\nTRUE BNWeb.Client.Request TRUE\n";

/// A loopback URL on a port that was free a moment ago and has no listener.
fn refused_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("local address").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/")
}

#[test]
fn a_refused_connection_is_http_failed_on_both_backends() {
    let source = workspace_root().join(PROGRAM);
    let interpreted = run(
        bni().arg("run").arg(&source).arg("--").arg(refused_url()),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run bni");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        EXPECTED,
        "bni"
    );
    let directory = TestDir::new("web-refused-connection").expect("directory");
    let artifact = compile_native(&source, &directory);
    let compiled = run(
        Command::new(&artifact).arg(refused_url()),
        None,
        PROCESS_TIMEOUT,
    )
    .expect("run artifact");
    assert_eq!(String::from_utf8_lossy(&compiled.stdout), EXPECTED, "bnc");
}
