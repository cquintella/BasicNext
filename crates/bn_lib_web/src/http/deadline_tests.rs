// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Connection and handler deadlines: a handler that outlives the connection
// deadline answers `408 Request Timeout`, however late the request arrived.

use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

use super::{Handler, serve_connection_with_handler};
use crate::web::ServerState;
use bn_host_net::net::TcpStream;

/// Runs a handler that outlives a 200 ms connection deadline; `client_delay`
/// is how long the client waits after connecting before it writes the
/// request. The deadline stays far above scheduling noise (a CI runner took
/// more than 10 ms to read the request, so the deadline fell before the
/// handler ran and the test measured something else); the handler blocks up
/// to 1 s, so it still outlives the deadline.
fn concurrent_handler_timeout_response(client_delay: Duration) -> Option<String> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
        Err(error) => panic!("bind concurrent-timeout listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("concurrent-timeout address");
    let options = crate::web::ServerOptions {
        concurrent_handlers: true,
        worker_count: 1,
        connection_total_ms: 200,
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
    std::thread::sleep(client_delay);
    client
        .write_all(b"GET /timeout HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write concurrent-timeout request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read concurrent-timeout response");
    release_sender.send(()).expect("release timed-out handler");
    server.join().expect("concurrent-timeout server thread");
    Some(response)
}

/// The handler deadline used to start with the handler, so it fell after the
/// connection deadline: a client that wrote even 1 ms after connecting got the
/// connection closed with no response. The delay is the scenario (client
/// latency longer than the 1 ms timer tick), not synchronization.
#[test]
fn late_client_still_gets_request_timeout_from_a_slow_handler() {
    let Some(response) = concurrent_handler_timeout_response(Duration::from_millis(3)) else {
        return;
    };
    assert!(
        response.starts_with("HTTP/1.1 408 Request Timeout"),
        "{response}"
    );
}

#[test]
fn opt_in_concurrent_handler_timeout_maps_to_request_timeout() {
    let Some(response) = concurrent_handler_timeout_response(Duration::ZERO) else {
        return;
    };
    assert!(
        response.starts_with("HTTP/1.1 408 Request Timeout"),
        "{response}"
    );
}
