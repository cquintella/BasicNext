// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The HTTPS client trust roots (bnweb.md, "HTTPS client trust roots") on
//! both backends, through `examples/bnweb/https_private_ca.bn`. The roots
//! are settled before any name is resolved: a bad extra file fails with
//! `HTTP_FAILED` before sending a packet; accepted roots complete an HTTPS
//! exchange with a local server whose certificate the extra CA signed.

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, spawn};
use std::time::Duration;

const EXAMPLE: &str = "examples/bnweb/https_private_ca.bn";
const TEST_CA: &str = "tests/fixtures/tls/test-ca.pem";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);
const CA_VARIABLE: &str = "BN_CA_FILE";
const IGNORED_VARIABLES: [&str; 2] = ["SSL_CERT_FILE", "SSL_CERT_DIR"];
const PRIMARY: &str = "bn-cacert.pem";
const SECONDARY: &str = "bn-ca.pem";
const TRUST_FAILED: &str = "request failed: TRUE\nTLS trust roots unavailable:";
const NO_CERTIFICATE: &str = "holds no valid certificate";
const UNREADABLE: &str = "cannot be read";
const NOT_A_CERTIFICATE: &str = "not a certificate\n";
const HTTP_FAILURE: &str = "request failed: TRUE\n";

#[derive(Clone, Copy, Debug)]
enum Backend {
    Interpreter,
    Native,
}

struct TestServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start() -> Self {
        let cert_path = workspace_root().join("tests/fixtures/tls/server.pem");
        let key_path = workspace_root().join("tests/fixtures/tls/server-key.pem");
        let chain = CertificateDer::pem_file_iter(&cert_path)
            .expect("server certs")
            .collect::<Result<Vec<_>, _>>()
            .expect("parse certs");
        let key = PrivateKeyDer::from_pem_file(&key_path).expect("key");

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server_config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("versions")
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .expect("server config");
        let server_config = Arc::new(server_config);

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("port").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);

        let handle = spawn(move || {
            while !stop_clone.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let config = Arc::clone(&server_config);
                        spawn(move || {
                            if let Ok(connection) = rustls::ServerConnection::new(config) {
                                let mut tls = rustls::StreamOwned::new(connection, stream);
                                let mut buf = [0u8; 1024];
                                if tls.read(&mut buf).is_ok_and(|n| n > 0) {
                                    let _ = tls.write_all(
                                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                                    );
                                    let _ = tls.flush();
                                }
                            }
                        });
                    }
                    Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
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
/// must change nothing. Returns (success, output).
fn outcome(
    backend: Backend,
    port: u16,
    beside: &[(&str, bool)],
    variable: Option<&Path>,
) -> (bool, String) {
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
    let url = format!("https://127.0.0.1:{port}/");
    command.arg(url).env_remove(CA_VARIABLE);
    for name in IGNORED_VARIABLES {
        command.env(name, &bad);
    }
    if let Some(path) = variable {
        command.env(CA_VARIABLE, path);
    }
    let output = run(command, None, PROCESS_TIMEOUT).expect("run the example");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

/// Refused roots: `HTTP_FAILED`, naming the file and the reason.
fn refused(output: &str, file: &str, reason: &str) -> bool {
    output.starts_with(HTTP_FAILURE)
        && output.starts_with(TRUST_FAILED)
        && output.contains(file)
        && output.contains(reason)
}

fn check(backend: Backend) {
    let server = TestServer::start();
    let port = server.port;
    let ca = workspace_root().join(TEST_CA);
    let missing = Path::new("/no/such/ca.pem");

    // The platform store alone; the SSL_* variables name a bad file in vain.
    // Certificate is untrusted by the system store, so TLS handshake fails.
    let (success, output) = outcome(backend, port, &[], None);
    assert!(
        !success && output.starts_with(HTTP_FAILURE) && output.contains("TLS handshake failed"),
        "{backend:?}, no extra file: success={success}, output={output}"
    );

    // bn-cacert.pem beside the program is read, and a bad one fails.
    let (success, output) = outcome(backend, port, &[(PRIMARY, false)], None);
    assert!(
        !success && refused(&output, PRIMARY, NO_CERTIFICATE),
        "{backend:?}, bad {PRIMARY}: {output}"
    );

    // bn-ca.pem is read when bn-cacert.pem is absent.
    let (success, output) = outcome(backend, port, &[(SECONDARY, false)], None);
    assert!(
        !success && refused(&output, SECONDARY, NO_CERTIFICATE),
        "{backend:?}, bad {SECONDARY}: {output}"
    );

    // bn-cacert.pem comes first: a good one hides a bad bn-ca.pem and completes exchange.
    let (success, output) = outcome(backend, port, &[(PRIMARY, true), (SECONDARY, false)], None);
    assert!(
        success && output.contains("status 200"),
        "{backend:?}, {PRIMARY} first: success={success}, output={output}"
    );

    // BN_CA_FILE comes before both, and completes exchange.
    let (success, output) = outcome(backend, port, &[(PRIMARY, false)], Some(&ca));
    assert!(
        success && output.contains("status 200"),
        "{backend:?}, BN_CA_FILE first: success={success}, output={output}"
    );

    let (success, output) = outcome(backend, port, &[(PRIMARY, true)], Some(missing));
    assert!(
        !success && refused(&output, "/no/such/ca.pem", UNREADABLE),
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
