use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{Handler, serve_connection, serve_connection_with_handler};
use crate::{
    net::TcpStream,
    web::{RouteOutcome, ServerState},
};
use http_body_util::Full;
use hyper::{Request, StatusCode, body::Bytes};
use hyper_util::rt::{TokioExecutor, TokioIo};

struct ScriptedResolver {
    answers: HashMap<String, Vec<crate::net::Address>>,
}

#[derive(Debug)]
struct NoCertificateResolver;

impl rustls::server::ResolvesServerCert for NoCertificateResolver {
    fn resolve(
        &self,
        _client_hello: rustls::server::ClientHello<'_>,
    ) -> Option<std::sync::Arc<rustls::sign::CertifiedKey>> {
        None
    }
}

impl super::AddressResolver for ScriptedResolver {
    fn resolve(
        &self,
        host: &str,
        _port: u16,
        maximum: usize,
    ) -> Result<Vec<crate::net::Address>, String> {
        self.answers
            .get(host)
            .map(|addresses| addresses.iter().copied().take(maximum).collect())
            .ok_or_else(|| format!("missing scripted answer for {host}"))
    }
}

#[test]
fn serves_local_http11_route_over_host_net_stream() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind test listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("listener address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/health".into()).unwrap();
    state.start().unwrap();
    let state = Arc::new(Mutex::new(state));
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept test connection");
        serve_connection(TcpStream::from_std(stream), server_state).expect("serve connection");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect test server");
    client
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    let mut response = String::new();
    client.read_to_string(&mut response).expect("read response");
    server.join().expect("server thread");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
}

#[test]
fn default_security_headers_are_transport_aware() {
    let mut cleartext = hyper::Response::new(Full::new(Bytes::new()));
    super::apply_default_security_headers(&mut cleartext, false);
    assert_eq!(
        cleartext.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert!(
        cleartext
            .headers()
            .get("strict-transport-security")
            .is_none()
    );

    let mut tls = hyper::Response::new(Full::new(Bytes::new()));
    tls.headers_mut().insert(
        "x-content-type-options",
        hyper::header::HeaderValue::from_static("unsafe"),
    );
    super::apply_default_security_headers(&mut tls, true);
    assert_eq!(
        tls.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(
        tls.headers().get("strict-transport-security").unwrap(),
        "max-age=31536000"
    );
}

#[test]
fn projects_callback_response_over_local_http11() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind test listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("listener address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/callback".into()).unwrap();
    state.start().unwrap();
    let state = Arc::new(Mutex::new(state));
    let callback: Handler = Arc::new(|_, response| {
        response.set_status(201)?;
        response.write("created")?;
        Ok(())
    });
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept test connection");
        serve_connection_with_handler(TcpStream::from_std(stream), server_state, Some(callback))
            .expect("serve callback connection");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect test server");
    client
        .write_all(b"GET /callback HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    let mut response = String::new();
    client.read_to_string(&mut response).expect("read response");
    server.join().expect("server thread");
    assert!(response.starts_with("HTTP/1.1 201 Created"), "{response}");
    assert!(response.ends_with("created"), "{response}");
}

#[test]
fn opt_in_concurrent_handler_runs_with_bounded_handler_slot() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind concurrent-handler listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("concurrent-handler address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/concurrent".into()).unwrap();
    state
        .start_with_options(crate::web::ServerOptions {
            concurrent_handlers: true,
            worker_count: 1,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    let state = Arc::new(Mutex::new(state));
    let handler: Handler = Arc::new(|_, response| {
        assert!(
            std::thread::current()
                .name()
                .is_some_and(|name| name.contains("tokio"))
        );
        response.write("concurrent")
    });
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept concurrent-handler peer");
        serve_connection_with_handler(TcpStream::from_std(stream), server_state, Some(handler))
            .expect("serve concurrent handler");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect concurrent handler");
    client
        .write_all(b"GET /concurrent HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write concurrent-handler request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read concurrent-handler response");
    server.join().expect("concurrent-handler server thread");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("concurrent"), "{response}");
}

#[test]
fn opt_in_concurrent_handler_rejects_when_global_slot_is_occupied() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind concurrent-overload listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("concurrent-overload address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/busy".into()).unwrap();
    state
        .start_with_options(crate::web::ServerOptions {
            concurrent_handlers: true,
            worker_count: 1,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    state.install_worker_pool().unwrap();
    let state = Arc::new(Mutex::new(state));
    let slots = state
        .lock()
        .expect("server lock")
        .handler_slots()
        .expect("handler slots");
    let slot = slots.try_acquire().expect("occupy only handler slot");
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handler_called = Arc::clone(&called);
    let handler: Handler = Arc::new(move |_, _| {
        handler_called.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept concurrent-overload peer");
        serve_connection_with_handler(TcpStream::from_std(stream), server_state, Some(handler))
            .expect("serve concurrent overload");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect concurrent overload");
    client
        .write_all(b"GET /busy HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write concurrent-overload request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read concurrent-overload response");
    server.join().expect("concurrent-overload server thread");
    drop(slot);
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert!(!called.load(std::sync::atomic::Ordering::SeqCst));
    let mut state = state.lock().expect("server lock");
    state.begin_stop(1000).expect("stop worker pool");
    while !state.workers_finished() {
        std::thread::yield_now();
    }
}

#[test]
fn opt_in_concurrent_handler_failure_maps_to_internal_error() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind concurrent-failure listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("concurrent-failure address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/failure".into()).unwrap();
    state
        .start_with_options(crate::web::ServerOptions {
            concurrent_handlers: true,
            worker_count: 1,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    let state = Arc::new(Mutex::new(state));
    let handler: Handler = Arc::new(|_, _| Err("controlled handler failure"));
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept concurrent-failure peer");
        serve_connection_with_handler(TcpStream::from_std(stream), server_state, Some(handler))
            .expect("serve concurrent failure");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect concurrent failure");
    client
        .write_all(b"GET /failure HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write concurrent-failure request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read concurrent-failure response");
    server.join().expect("concurrent-failure server thread");
    assert!(
        response.starts_with("HTTP/1.1 500 Internal Server Error"),
        "{response}"
    );
}

#[test]
fn opt_in_concurrent_handler_timeout_maps_to_request_timeout() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind concurrent-timeout listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("concurrent-timeout address");
    let options = crate::web::ServerOptions {
        concurrent_handlers: true,
        worker_count: 1,
        connection_total_ms: 10,
        ..crate::web::ServerOptions::default()
    };
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/timeout".into()).unwrap();
    state.start_with_options(options).unwrap();
    let state = Arc::new(Mutex::new(state));
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let handler: Handler = Arc::new(move |_, _| {
        let _ = release_receiver
            .lock()
            .expect("release receiver lock")
            .recv_timeout(Duration::from_secs(1));
        Ok(())
    });
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept concurrent-timeout peer");
        serve_connection_with_handler(TcpStream::from_std(stream), server_state, Some(handler))
            .expect("serve concurrent timeout");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect concurrent timeout");
    client
        .write_all(b"GET /timeout HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write concurrent-timeout request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read concurrent-timeout response");
    release_sender.send(()).expect("release timed-out handler");
    server.join().expect("concurrent-timeout server thread");
    assert!(
        response.starts_with("HTTP/1.1 408 Request Timeout"),
        "{response}"
    );
}

#[test]
fn callback_response_strips_body_for_head_requests() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind test listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("listener address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/callback".into()).unwrap();
    state.start().unwrap();
    let state = Arc::new(Mutex::new(state));
    let callback: Handler = Arc::new(|_, response| {
        response.write("hidden")?;
        Ok(())
    });
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept test connection");
        serve_connection_with_handler(TcpStream::from_std(stream), server_state, Some(callback))
            .expect("serve callback connection");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect test server");
    client
        .write_all(b"HEAD /callback HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    let mut response = String::new();
    client.read_to_string(&mut response).expect("read response");
    server.join().expect("server thread");
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(!response.ends_with("hidden"), "{response}");
}

#[test]
fn serves_local_http2_route_over_host_net_stream() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind test listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("listener address");
    let mut state = ServerState::new();
    state.add_route("GET".into(), "/health".into()).unwrap();
    state.start().unwrap();
    let state = Arc::new(Mutex::new(state));
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept test connection");
        serve_connection(TcpStream::from_std(stream), server_state).expect("serve connection");
    });
    let client = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("client runtime");
        runtime.block_on(async move {
            let stream = std::net::TcpStream::connect(endpoint).expect("connect test server");
            stream.set_nonblocking(true).expect("nonblocking stream");
            let io = TokioIo::new(tokio::net::TcpStream::from_std(stream).expect("tokio stream"));
            let (mut sender, connection) =
                hyper::client::conn::http2::handshake(TokioExecutor::new(), io)
                    .await
                    .expect("HTTP/2 handshake");
            tokio::spawn(async move {
                let _ = connection.await;
            });
            let request = Request::builder()
                .method("GET")
                .uri("http://localhost/health")
                .body(Full::new(Bytes::new()))
                .expect("request");
            let response = sender.send_request(request).await.expect("HTTP/2 response");
            assert_eq!(response.status(), StatusCode::OK);
        });
    });
    client.join().expect("client thread");
    server.join().expect("server thread");
}

#[test]
fn method_not_allowed_response_carries_allow_header() {
    let response =
        super::response_with_allow(hyper::StatusCode::METHOD_NOT_ALLOWED, "", Some("GET, HEAD"));
    assert_eq!(response.status(), hyper::StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers()["allow"], "GET, HEAD");
}

#[test]
fn options_uses_no_content_and_allow() {
    let outcome = RouteOutcome::MethodNotAllowed(vec!["GET".into(), "HEAD".into()]);
    let (status, allow) = super::route_status("OPTIONS", &outcome);
    assert_eq!(status, hyper::StatusCode::NO_CONTENT);
    assert_eq!(allow.as_deref(), Some("GET, HEAD"));
}

#[test]
fn authority_parser_handles_default_and_ipv6_ports() {
    assert_eq!(
        super::parse_authority("example.test").unwrap(),
        ("example.test".into(), 80)
    );
    assert_eq!(
        super::parse_authority("[2001:db8::1]:8080").unwrap(),
        ("2001:db8::1".into(), 8080)
    );
}

#[test]
fn redirects_resolve_relative_paths_and_reject_fragments() {
    assert_eq!(
        super::resolve_redirect("http://example.test/a", "/next").unwrap(),
        "http://example.test/next"
    );
    assert_eq!(
        super::resolve_redirect("http://example.test/a", "?page=2").unwrap(),
        "http://example.test/a?page=2"
    );
    assert_eq!(
        super::resolve_redirect("http://example.test/dir/a", "next").unwrap(),
        "http://example.test/dir/next"
    );
    assert!(super::resolve_redirect("http://example.test/a", "/next#x").is_err());
}

#[test]
fn compressed_responses_are_rejected_without_decoder_limits() {
    assert!(super::has_unsupported_encoding(&[(
        "Content-Encoding".into(),
        "gzip".into()
    )]));
    assert!(!super::has_unsupported_encoding(&[(
        "Content-Type".into(),
        "text/plain".into()
    )]));
}

#[test]
fn client_rejects_oversized_request_before_transport() {
    let body = "x".repeat(crate::config::web_limits().max_body_bytes + 1);
    let error = super::client_request("POST", "http://127.0.0.1/", &body).unwrap_err();
    assert_eq!(error, "request body exceeds 8 MiB");
}

#[test]
fn client_resolver_rechecks_mixed_dns_answers_fail_closed() {
    let resolver = ScriptedResolver {
        answers: HashMap::from([
            (
                "mixed.test".into(),
                vec![
                    crate::net::Address::parse("8.8.8.8").unwrap(),
                    crate::net::Address::parse("::ffff:127.0.0.1").unwrap(),
                ],
            ),
            (
                "public.test".into(),
                vec![crate::net::Address::parse("8.8.8.8").unwrap()],
            ),
            (
                "blocked.test".into(),
                vec![crate::net::Address::parse("100.64.0.1").unwrap()],
            ),
        ]),
    };
    assert!(super::resolve_validated_addresses("mixed.test", 80, &resolver).is_err());
    assert_eq!(
        super::resolve_validated_addresses("public.test", 80, &resolver).unwrap(),
        vec!["8.8.8.8".parse::<std::net::IpAddr>().unwrap()]
    );
    assert!(super::resolve_validated_addresses("blocked.test", 80, &resolver).is_err());
}

#[test]
fn explicit_egress_policy_is_applied_after_resolution() {
    let resolver = ScriptedResolver {
        answers: HashMap::from([(
            "allowed.test".into(),
            vec![crate::net::Address::parse("93.184.216.34").unwrap()],
        )]),
    };
    let policy = crate::web::EgressPolicy::new(
        Some(vec!["http".into()]),
        Some(vec![crate::net::Cidr::parse("93.184.216.0/24").unwrap()]),
        Some(vec![80]),
        2,
        1000,
    )
    .unwrap();
    assert!(
        super::resolve_validated_addresses_with_policy(
            "allowed.test",
            80,
            &resolver,
            &policy,
            "http"
        )
        .is_ok()
    );
    assert!(
        super::resolve_validated_addresses_with_policy(
            "allowed.test",
            8080,
            &resolver,
            &policy,
            "http"
        )
        .is_err()
    );
}

#[test]
fn https_resolution_uses_tls_scheme_and_default_port() {
    let resolver = ScriptedResolver {
        answers: HashMap::from([(
            "secure.test".into(),
            vec![crate::net::Address::parse("93.184.216.34").unwrap()],
        )]),
    };
    let policy =
        crate::web::EgressPolicy::new(Some(vec!["https".into()]), None, Some(vec![443]), 2, 1000)
            .unwrap();
    assert!(
        super::resolve_validated_addresses_with_policy(
            "secure.test",
            443,
            &resolver,
            &policy,
            "https"
        )
        .is_ok()
    );
    assert!(
        super::resolve_validated_addresses_with_policy(
            "secure.test",
            80,
            &resolver,
            &policy,
            "http"
        )
        .is_err()
    );
}

#[test]
fn rate_limited_http_request_returns_429_and_retry_after() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind rate-limit listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("rate-limit listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            rate_limit_burst: 1,
            rate_limit_refill_per_second: 1,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    state.add_route("GET".into(), "/limited".into()).unwrap();
    let state = Arc::new(Mutex::new(state));
    let handler: super::Handler = Arc::new(|_, _| Ok(()));
    let server_state = Arc::clone(&state);
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (stream, _) = listener.accept().expect("accept rate-limit peer");
            super::serve_connection_with_handler(
                TcpStream::from_std(stream),
                Arc::clone(&server_state),
                Some(Arc::clone(&handler)),
            )
            .expect("serve rate-limit request");
        }
    });
    let request = || {
        let mut client = std::net::TcpStream::connect(endpoint).expect("connect rate-limit peer");
        client
            .write_all(b"GET /limited HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .expect("write rate-limit request");
        let mut response = String::new();
        client
            .read_to_string(&mut response)
            .expect("read rate-limit response");
        response
    };
    assert!(request().starts_with("HTTP/1.1 200 OK"));
    let limited = request();
    server.join().expect("rate-limit server thread");
    assert!(limited.starts_with("HTTP/1.1 429 Too Many Requests"));
    assert!(limited.to_ascii_lowercase().contains("retry-after: 1"));
}

#[test]
fn overloaded_http_request_returns_503_without_running_handler() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind overload listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("overload listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            pending_work: 1,
            rate_limit_burst: 100,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    state.add_route("GET".into(), "/busy".into()).unwrap();
    let state = Arc::new(Mutex::new(state));
    let (started_sender, started_receiver) = std::sync::mpsc::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    let release_receiver = Arc::new(Mutex::new(release_receiver));
    let handler: super::Handler = Arc::new(move |_, _| {
        started_sender.send(()).unwrap();
        release_receiver.lock().unwrap().recv().unwrap();
        Ok(())
    });
    let server_state = Arc::clone(&state);
    let server_handler = Arc::clone(&handler);
    let server = std::thread::spawn(move || {
        let (first, _) = listener.accept().expect("accept first overload peer");
        let first_state = Arc::clone(&server_state);
        let first_handler = Arc::clone(&server_handler);
        let first_worker = std::thread::spawn(move || {
            super::serve_connection_with_handler(
                TcpStream::from_std(first),
                first_state,
                Some(first_handler),
            )
            .expect("serve first overload request");
        });
        let (second, _) = listener.accept().expect("accept second overload peer");
        super::serve_connection_with_handler(
            TcpStream::from_std(second),
            server_state,
            Some(server_handler),
        )
        .expect("serve second overload request");
        release_sender.send(()).unwrap();
        first_worker.join().expect("first overload worker");
    });
    let mut first_client =
        std::net::TcpStream::connect(endpoint).expect("connect first overload peer");
    first_client
        .write_all(b"GET /busy HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write first overload request");
    started_receiver.recv().expect("first handler started");
    let mut second_client =
        std::net::TcpStream::connect(endpoint).expect("connect second overload peer");
    second_client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("set overload read deadline");
    second_client
        .write_all(b"GET /busy HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write second overload request");
    let mut response = String::new();
    second_client
        .read_to_string(&mut response)
        .expect("read overload response");
    server.join().expect("overload server thread");
    first_client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("set first response deadline");
    let mut first_response = String::new();
    first_client
        .read_to_string(&mut first_response)
        .expect("read first response");
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable"),
        "{response}"
    );
    assert!(
        first_response.starts_with("HTTP/1.1 200 OK"),
        "{first_response}"
    );
}

#[test]
fn configured_body_timeout_returns_request_timeout() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind body-timeout listener: {error}"),
    };
    let endpoint = listener
        .local_addr()
        .expect("body-timeout listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            body_read_ms: 1,
            connection_total_ms: 1000,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    state.add_route("POST".into(), "/body".into()).unwrap();
    let state = Arc::new(Mutex::new(state));
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept body-timeout peer");
        super::serve_connection_with_handler(
            TcpStream::from_std(stream),
            state,
            Some(Arc::new(|_, _| Ok(()))),
        )
        .expect("serve body-timeout request");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect body-timeout peer");
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("set body-timeout read deadline");
    client
        .write_all(b"POST /body HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\nConnection: close\r\n\r\n")
        .expect("write body-timeout headers");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read body-timeout response");
    server.join().expect("body-timeout server thread");
    assert!(
        response.starts_with("HTTP/1.1 408 Request Timeout"),
        "{response}"
    );
}

#[test]
fn configured_header_timeout_closes_a_partial_request() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind header-timeout listener: {error}"),
    };
    let endpoint = listener
        .local_addr()
        .expect("header-timeout listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            header_read_ms: 1,
            connection_total_ms: 1000,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    state.add_route("GET".into(), "/partial".into()).unwrap();
    let state = Arc::new(Mutex::new(state));
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept header-timeout peer");
        let result = super::serve_connection_with_handler(
            TcpStream::from_std(stream),
            state,
            Some(Arc::new(|_, _| Ok(()))),
        );
        assert!(result.is_err(), "partial headers must time out");
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect header-timeout peer");
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("set header-timeout read deadline");
    client
        .write_all(b"GET /partial HTTP/1.1\r\nHost: localhost\r\n")
        .expect("write partial headers");
    let mut response = Vec::new();
    let _ = client.read_to_end(&mut response);
    server.join().expect("header-timeout server thread");
    assert!(!response.starts_with(b"HTTP/1.1 200 OK"));
}

#[test]
fn configured_tls_handshake_timeout_closes_an_idle_peer() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind TLS-timeout listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("TLS-timeout listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            tls_handshake_ms: 1,
            connection_total_ms: 1000,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    let state = Arc::new(Mutex::new(state));
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(NoCertificateResolver));
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept TLS-timeout peer");
        let result = super::serve_tls_connection_with_handler(
            TcpStream::from_std(stream),
            state,
            Arc::new(config),
            None,
        );
        assert!(result.is_err(), "idle TLS handshake must time out");
    });
    let _client = std::net::TcpStream::connect(endpoint).expect("connect TLS-timeout peer");
    server.join().expect("TLS-timeout server thread");
}

#[test]
fn configured_connection_deadline_closes_a_stalled_request() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind connection-timeout listener: {error}"),
    };
    let endpoint = listener
        .local_addr()
        .expect("connection-timeout listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            header_read_ms: 60_000,
            connection_total_ms: 1,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    let state = Arc::new(Mutex::new(state));
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept connection-timeout peer");
        let result = super::serve_connection_with_handler(
            TcpStream::from_std(stream),
            state,
            Some(Arc::new(|_, _| Ok(()))),
        );
        assert!(
            result.is_err(),
            "stalled connection must hit total deadline"
        );
    });
    let mut client =
        std::net::TcpStream::connect(endpoint).expect("connect connection-timeout peer");
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("set connection-timeout read deadline");
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n")
        .expect("write partial request");
    let mut response = Vec::new();
    let _ = client.read_to_end(&mut response);
    server.join().expect("connection-timeout server thread");
    assert!(!response.starts_with(b"HTTP/1.1 200 OK"));
}

#[test]
fn configured_http2_idle_keep_alive_closes_a_peer_that_ignores_ping() {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind HTTP/2 idle-timeout listener: {error}"),
    };
    let endpoint = listener
        .local_addr()
        .expect("HTTP/2 idle-timeout listener address");
    let mut state = ServerState::new();
    state
        .start_with_options(crate::web::ServerOptions {
            idle_keep_alive_ms: 10,
            connection_total_ms: 60_000,
            ..crate::web::ServerOptions::default()
        })
        .unwrap();
    let state = Arc::new(Mutex::new(state));
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept HTTP/2 idle-timeout peer");
        let result = super::serve_connection_with_handler(
            TcpStream::from_std(stream),
            state,
            Some(Arc::new(|_, _| Ok(()))),
        );
        assert!(
            result.is_err(),
            "an unanswered HTTP/2 keep-alive ping must close the connection"
        );
    });
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect HTTP/2 peer");
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("set HTTP/2 idle-timeout read deadline");
    client
        .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
        .expect("write HTTP/2 preface");
    client
        .write_all(&[0, 0, 0, 4, 0, 0, 0, 0, 0])
        .expect("write HTTP/2 settings");
    let mut buffer = [0_u8; 1024];
    let mut reached_eof = false;
    loop {
        match client.read(&mut buffer) {
            Ok(0) => {
                reached_eof = true;
                break;
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => panic!("read HTTP/2 idle-timeout result: {error}"),
        }
    }
    let _ = client.shutdown(std::net::Shutdown::Both);
    server.join().expect("HTTP/2 idle-timeout server thread");
    assert!(
        reached_eof,
        "server must close an idle HTTP/2 peer after unanswered ping"
    );
}

#[test]
fn redirect_destination_is_revalidated_with_the_same_resolver_policy() {
    let resolver = ScriptedResolver {
        answers: HashMap::from([
            (
                "public.test".into(),
                vec![crate::net::Address::parse("8.8.8.8").unwrap()],
            ),
            (
                "blocked.test".into(),
                vec![crate::net::Address::parse("169.254.169.254").unwrap()],
            ),
        ]),
    };
    let redirect =
        super::resolve_redirect("http://public.test/start", "http://blocked.test/metadata")
            .unwrap();
    assert_eq!(redirect, "http://blocked.test/metadata");
    assert!(super::resolve_validated_addresses("public.test", 80, &resolver).is_ok());
    assert!(super::resolve_validated_addresses("blocked.test", 80, &resolver).is_err());
}
