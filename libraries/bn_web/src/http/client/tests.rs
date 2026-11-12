// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Failures of the HTTP client are typed where they happen.

use crate::web_error::{SizeLimit, WebFailure};

#[test]
fn client_rejects_oversized_request_before_transport() {
    let body = "x".repeat(bn_limits::web_limits().max_body_bytes + 1);
    let error = super::client_request("POST", "http://127.0.0.1/", &body).unwrap_err();
    let limit = bn_limits::web_limits().max_body_bytes;
    assert_eq!(
        error,
        WebFailure::TooLarge {
            what: SizeLimit::RequestBody,
            limit
        }
    );
}

#[test]
fn client_connect_failure_due_to_socket_quota_returns_web_limit() {
    let quota = std::io::Error::other(bn_core_net::QuotaExceeded);
    assert_eq!(
        super::connect_failure(&quota).code(),
        bn_types::error_codes::web::LIMIT,
        "socket quota failure must map to Web.LIMIT"
    );
    let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
    assert_eq!(
        super::connect_failure(&refused).code(),
        bn_types::error_codes::web::HTTP_FAILED,
        "a refused connection must map to Web.HTTP_FAILED"
    );
}

#[test]
fn client_dns_deadline_expiry_returns_web_timeout() {
    struct TimeoutResolver;
    impl super::AddressResolver for TimeoutResolver {
        fn resolve(
            &self,
            _host: &str,
            _port: u16,
            _maximum: usize,
            _timeout: std::time::Duration,
        ) -> Result<Vec<bn_core_net::Address>, WebFailure> {
            Err(WebFailure::Timeout("DNS resolution timed out".into()))
        }
    }
    let resolver = TimeoutResolver;
    let policy = crate::web::EgressPolicy::default();
    let error = super::client_request_with_resolver_and_policy(
        "GET",
        "http://example.com/",
        "",
        &resolver,
        &policy,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        bn_types::error_codes::web::TIMEOUT,
        "DNS timeout must map to Web.TIMEOUT; got {error:?}"
    );
}

#[test]
fn client_dns_pool_saturation_returns_web_limit() {
    struct SaturatedResolver;
    impl super::AddressResolver for SaturatedResolver {
        fn resolve(
            &self,
            _host: &str,
            _port: u16,
            _maximum: usize,
            _timeout: std::time::Duration,
        ) -> Result<Vec<bn_core_net::Address>, WebFailure> {
            Err(WebFailure::LimitExceeded(
                "DNS resolution pool limit exceeded".into(),
            ))
        }
    }
    let resolver = SaturatedResolver;
    let policy = crate::web::EgressPolicy::default();
    let error = super::client_request_with_resolver_and_policy(
        "GET",
        "http://example.com/",
        "",
        &resolver,
        &policy,
    )
    .unwrap_err();
    assert_eq!(
        error.code(),
        bn_types::error_codes::web::LIMIT,
        "DNS pool saturation must map to Web.LIMIT; got {error:?}"
    );
}

/// A loopback server for one connection: it reads the request, then plays
/// `script` (wait, then write), and holds the socket open until the test ends.
/// Each wait is a timed receive on a channel nobody sends on.
struct StallingServer {
    port: u16,
    release: Option<std::sync::mpsc::Sender<()>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl StallingServer {
    fn start(script: Vec<(std::time::Duration, Vec<u8>)>, read_request: bool) -> Self {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local address").port();
        let (release, held) = std::sync::mpsc::channel::<()>();
        let handle = std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            if read_request {
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer);
            }
            for (wait, bytes) in script {
                let _ = held.recv_timeout(wait);
                if stream.write_all(&bytes).is_err() {
                    return;
                }
                let _ = stream.flush();
            }
            // Hold the socket open until the test drops the server.
            let _ = held.recv();
        });
        Self {
            port,
            release: Some(release),
            handle: Some(handle),
        }
    }
}

impl Drop for StallingServer {
    fn drop(&mut self) {
        drop(self.release.take());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// The total deadline of the stalling tests, and the most a timed-out call
/// may take beyond it.
const CLIENT_DEADLINE_MS: u64 = 600;
const DEADLINE_SLACK: std::time::Duration = std::time::Duration::from_millis(1500);

/// Runs one GET under a policy that allows the loopback port with a
/// `CLIENT_DEADLINE_MS` deadline, on its own thread: a call that does not
/// return within the deadline and its slack is reported, never waited for.
fn timed_get(scheme: &'static str, port: u16) -> Result<std::time::Duration, String> {
    let policy = crate::web::EgressPolicy::from_csv(
        scheme,
        "127.0.0.0/8",
        &port.to_string(),
        0,
        CLIENT_DEADLINE_MS,
    )
    .map_err(|reason| format!("policy: {reason:?}"))?;
    let (done, finished) = std::sync::mpsc::channel();
    let started = std::time::Instant::now();
    std::thread::spawn(move || {
        let outcome = super::client_transfer_with_policy(
            "GET",
            &format!("{scheme}://127.0.0.1:{port}/"),
            &[],
            &[],
            &policy,
        );
        let _ = done.send(outcome.map(|_| ()));
    });
    let limit = std::time::Duration::from_millis(CLIENT_DEADLINE_MS) + DEADLINE_SLACK;
    match finished.recv_timeout(limit) {
        Ok(Err(failure)) if failure.code() == bn_types::error_codes::web::TIMEOUT => {
            Ok(started.elapsed())
        }
        Ok(other) => Err(format!("expected TIMEOUT, got {other:?}")),
        Err(_) => Err(format!("the call did not return within {limit:?}")),
    }
}

fn assert_timed_out(outcome: Result<std::time::Duration, String>) {
    if let Err(problem) = outcome {
        panic!("{problem}");
    }
}

#[test]
fn client_deadline_bounds_a_stalled_tls_handshake() {
    let server = StallingServer::start(Vec::new(), false);
    assert_timed_out(timed_get("https", server.port));
}

#[test]
fn client_deadline_bounds_a_stalled_response_head() {
    let server = StallingServer::start(Vec::new(), true);
    assert_timed_out(timed_get("http", server.port));
}

#[test]
fn client_deadline_bounds_a_stalled_body() {
    let head = b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\npartial".to_vec();
    let server = StallingServer::start(vec![(std::time::Duration::ZERO, head)], true);
    assert_timed_out(timed_get("http", server.port));
}

/// Two waits, each well inside the deadline, together past it: one deadline
/// for the whole exchange, not one per phase.
#[test]
fn client_deadline_is_one_for_all_phases() {
    let wait = std::time::Duration::from_millis(CLIENT_DEADLINE_MS * 2 / 3);
    let head = b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\n".to_vec();
    let server = StallingServer::start(vec![(wait, head), (wait, b"done".to_vec())], true);
    assert_timed_out(timed_get("http", server.port));
}
