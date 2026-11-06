// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The HTTPS client trust roots (bnweb.md, "HTTPS client trust roots") on
//! both backends, through `examples/bnweb/https_private_ca.bn`. The roots
//! are settled before any name is resolved, so a request to a loopback URL
//! shows which extra file was chosen without sending a packet: a bad extra
//! file fails with `HTTP_FAILED` and names the file; accepted roots go on to
//! the egress refusal of a loopback destination. The handshake itself, with a
//! server the test CA signed, is `bn_web`'s `tls_roots` test.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const EXAMPLE: &str = "examples/bnweb/https_private_ca.bn";
const TEST_CA: &str = "tests/fixtures/tls/test-ca.pem";
const URL: &str = "https://127.0.0.1:9/";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const CA_VARIABLE: &str = "BN_CA_FILE";
const IGNORED_VARIABLES: [&str; 2] = ["SSL_CERT_FILE", "SSL_CERT_DIR"];
const PRIMARY: &str = "bn-cacert.pem";
const SECONDARY: &str = "bn-ca.pem";
const TRUST_FAILED: &str = "request failed: TRUE\nTLS trust roots unavailable:";
const NO_CERTIFICATE: &str = "holds no valid certificate";
const UNREADABLE: &str = "cannot be read";
/// What the roots being accepted looks like here: the next check refuses
/// the loopback destination.
const ROOTS_ACCEPTED: &str = "private or local destination requires explicit opt-in";
const NOT_A_CERTIFICATE: &str = "not a certificate\n";
const HTTP_FAILURE: &str = "request failed: TRUE\n";
const OTHER_FAILURE: &str = "request failed: FALSE\n";

#[derive(Clone, Copy, Debug)]
enum Backend {
    Interpreter,
    Native,
}

/// A program and the directory "beside the program" means for it: the
/// `.bn` file's directory under `bni`, the executable's under `bnc` (built
/// away from its source, so the source directory cannot be the one read).
struct Program {
    command: Command,
    beside: PathBuf,
    _directories: Vec<TestDir>,
}

fn program(backend: Backend) -> Program {
    let source_directory = TestDir::new("https-roots-source").expect("directory");
    let source = source_directory.join("https_private_ca.bn");
    fs::copy(workspace_root().join(EXAMPLE), &source).expect("copy the example");
    match backend {
        Backend::Interpreter => {
            let mut command = bni();
            command.arg("run").arg(&source).arg("--");
            Program {
                command,
                beside: source_directory.path().to_path_buf(),
                _directories: vec![source_directory],
            }
        }
        Backend::Native => {
            let build = TestDir::new("https-roots-build").expect("directory");
            let artifact = compile_native(&source, &build);
            Program {
                command: Command::new(artifact),
                beside: build.path().to_path_buf(),
                _directories: vec![source_directory, build],
            }
        }
    }
}

/// Runs a fresh program with the given extra files beside it and the given
/// `BN_CA_FILE`; `SSL_CERT_FILE`/`SSL_CERT_DIR` always name a bad file and
/// must change nothing. Returns the program's output.
fn outcome(backend: Backend, beside: &[(&str, bool)], variable: Option<&Path>) -> String {
    let mut program = program(backend);
    for (name, valid) in beside {
        let path = program.beside.join(name);
        if *valid {
            fs::copy(workspace_root().join(TEST_CA), &path).expect("copy the CA");
        } else {
            fs::write(&path, NOT_A_CERTIFICATE).expect("write a bad CA file");
        }
    }
    let bad = program.beside.join("not-a-ca.pem");
    fs::write(&bad, NOT_A_CERTIFICATE).expect("write");
    let command = &mut program.command;
    command.arg(URL).env_remove(CA_VARIABLE);
    for name in IGNORED_VARIABLES {
        command.env(name, &bad);
    }
    if let Some(path) = variable {
        command.env(CA_VARIABLE, path);
    }
    let output = run(command, None, PROCESS_TIMEOUT).expect("run the example");
    assert!(
        !output.status.success(),
        "a loopback request cannot succeed"
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A `bnc` artifact reports only the error code of a `BNWeb.Client` failure
/// today (its `Cause` is empty, an open parity defect), so there the code alone
/// tells the outcome: `HTTP_FAILED` for refused roots, the egress code for
/// accepted ones. The interpreter also shows which file and why.
fn accepted(backend: Backend, output: &str) -> bool {
    match backend {
        Backend::Interpreter => !output.contains(TRUST_FAILED) && output.contains(ROOTS_ACCEPTED),
        Backend::Native => output.starts_with(OTHER_FAILURE),
    }
}

fn refused(backend: Backend, output: &str, file: &str, reason: &str) -> bool {
    match backend {
        Backend::Interpreter => {
            output.starts_with(TRUST_FAILED) && output.contains(file) && output.contains(reason)
        }
        Backend::Native => output.starts_with(HTTP_FAILURE),
    }
}

fn check(backend: Backend) {
    let ca = workspace_root().join(TEST_CA);
    let missing = Path::new("/no/such/ca.pem");
    // The platform store alone; the SSL_* variables name a bad file in vain.
    let output = outcome(backend, &[], None);
    assert!(
        accepted(backend, &output),
        "{backend:?}, no extra file: {output}"
    );
    // bn-cacert.pem beside the program is read, and a bad one fails.
    let output = outcome(backend, &[(PRIMARY, false)], None);
    assert!(
        refused(backend, &output, PRIMARY, NO_CERTIFICATE),
        "{backend:?}, bad {PRIMARY}: {output}"
    );
    // bn-ca.pem is read when bn-cacert.pem is absent.
    let output = outcome(backend, &[(SECONDARY, false)], None);
    assert!(
        refused(backend, &output, SECONDARY, NO_CERTIFICATE),
        "{backend:?}, bad {SECONDARY}: {output}"
    );
    // bn-cacert.pem comes first: a good one hides a bad bn-ca.pem.
    let output = outcome(backend, &[(PRIMARY, true), (SECONDARY, false)], None);
    assert!(
        accepted(backend, &output),
        "{backend:?}, {PRIMARY} first: {output}"
    );
    // BN_CA_FILE comes before both, and is never skipped.
    let output = outcome(backend, &[(PRIMARY, false)], Some(&ca));
    assert!(
        accepted(backend, &output),
        "{backend:?}, BN_CA_FILE first: {output}"
    );
    let output = outcome(backend, &[(PRIMARY, true)], Some(missing));
    assert!(
        refused(backend, &output, "/no/such/ca.pem", UNREADABLE),
        "{backend:?}, unreadable BN_CA_FILE: {output}"
    );
}

#[test]
fn https_trust_roots_follow_the_rule_on_the_interpreter() {
    check(Backend::Interpreter);
}

#[test]
fn https_trust_roots_follow_the_rule_on_a_compiled_program() {
    check(Backend::Native);
}
