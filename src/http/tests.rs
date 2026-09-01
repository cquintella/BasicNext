use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};

use super::{Handler, serve_connection, serve_connection_with_handler};
use crate::{
    net::TcpStream,
    web::{RouteOutcome, ServerState},
};
use http_body_util::Full;
use hyper::{Request, StatusCode, body::Bytes};
use hyper_util::rt::{TokioExecutor, TokioIo};

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
    let body = "x".repeat(super::MAX_HTTP_BODY + 1);
    let error = super::client_request("POST", "http://127.0.0.1/", &body).unwrap_err();
    assert_eq!(error, "request body exceeds 8 MiB");
}
