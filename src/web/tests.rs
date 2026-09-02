use super::server::ServerOptions;
use super::server::ServerStatus;
use super::{
    Route, RouteOutcome, ServerState, allowed_methods, bounded_body, canonical_target,
    dispatch_route, effective_client_address, header_values, query_values, route_for_request,
    select_route, validate_client_url, validate_ssrf_destinations,
};
use std::{
    io::{Read, Write},
    net::IpAddr,
};

#[test]
fn server_options_validate_before_accepting_and_bound_quotas() {
    let options = ServerOptions {
        active_connections: 1,
        pending_work: 1,
        ..ServerOptions::default()
    };
    options.validate().unwrap();
    let mut server = ServerState::new();
    server.start_with_options(options).unwrap();
    server.admit_connection().unwrap();
    assert!(server.admit_connection().is_err());
    assert!(
        ServerOptions {
            active_connections: 0,
            ..ServerOptions::default()
        }
        .validate()
        .is_err()
    );
}

#[test]
fn rate_limit_rejects_before_handler_and_bounds_keys() {
    let options = ServerOptions {
        rate_limit_burst: 1,
        rate_limit_refill_per_second: 1,
        rate_limit_key_capacity: 1,
        ..ServerOptions::default()
    };
    let mut server = ServerState::new();
    server.start_with_options(options).unwrap();
    server.add_route("GET".into(), "/".into()).unwrap();
    let mut handled = 0;
    server
        .dispatch_with_key("GET", "/", "client-a", |_| handled += 1)
        .unwrap();
    assert_eq!(handled, 1);
    assert_eq!(
        server.dispatch_with_key("GET", "/", "client-a", |_| handled += 1),
        Err("rate limit exceeded")
    );
    assert_eq!(handled, 1);
    assert_eq!(
        server.dispatch_with_key("GET", "/", "client-b", |_| handled += 1),
        Ok(())
    );
}

#[test]
fn rate_limit_refills_with_controlled_time_and_evicts_oldest_key() {
    let options = ServerOptions {
        rate_limit_burst: 1,
        rate_limit_refill_per_second: 1,
        rate_limit_key_capacity: 2,
        ..ServerOptions::default()
    };
    let mut server = ServerState::new();
    server.start_with_options(options).unwrap();
    server.add_route("GET".into(), "/".into()).unwrap();
    let origin = std::time::Instant::now();
    server
        .dispatch_with_key_at("GET", "/", "route-a|client-a", origin, |_| {})
        .unwrap();
    assert_eq!(
        server.dispatch_with_key_at("GET", "/", "route-a|client-a", origin, |_| {}),
        Err("rate limit exceeded")
    );
    server
        .dispatch_with_key_at(
            "GET",
            "/",
            "route-b|client-a",
            origin + std::time::Duration::from_millis(1),
            |_| {},
        )
        .unwrap();
    server
        .dispatch_with_key_at(
            "GET",
            "/",
            "route-c|client-a",
            origin + std::time::Duration::from_millis(2),
            |_| {},
        )
        .unwrap();
    assert_eq!(
        server.dispatch_with_key_at(
            "GET",
            "/",
            "route-a|client-a",
            origin + std::time::Duration::from_millis(3),
            |_| {},
        ),
        Ok(()),
        "oldest route-a bucket must be evicted"
    );
    assert_eq!(
        server.dispatch_with_key_at(
            "GET",
            "/",
            "route-c|client-a",
            origin + std::time::Duration::from_millis(3),
            |_| {},
        ),
        Err("rate limit exceeded"),
        "newest route-c bucket must remain"
    );
}

#[test]
fn rate_limit_refills_fractional_seconds_without_losing_remainder() {
    let options = ServerOptions {
        rate_limit_burst: 1,
        rate_limit_refill_per_second: 10,
        rate_limit_key_capacity: 1,
        ..ServerOptions::default()
    };
    let mut server = ServerState::new();
    server.start_with_options(options).unwrap();
    server.add_route("GET".into(), "/".into()).unwrap();
    let origin = std::time::Instant::now();
    server
        .dispatch_with_key_at("GET", "/", "client-a", origin, |_| {})
        .unwrap();
    assert_eq!(
        server.dispatch_with_key_at(
            "GET",
            "/",
            "client-a",
            origin + std::time::Duration::from_millis(50),
            |_| {},
        ),
        Err("rate limit exceeded")
    );
    server
        .dispatch_with_key_at(
            "GET",
            "/",
            "client-a",
            origin + std::time::Duration::from_millis(100),
            |_| {},
        )
        .unwrap();
    assert_eq!(
        server.dispatch_with_key_at(
            "GET",
            "/",
            "client-a",
            origin + std::time::Duration::from_millis(150),
            |_| {},
        ),
        Err("rate limit exceeded")
    );
    server
        .dispatch_with_key_at(
            "GET",
            "/",
            "client-a",
            origin + std::time::Duration::from_millis(200),
            |_| {},
        )
        .unwrap();
}

#[test]
fn server_status_tracks_readiness_and_drain() {
    let mut server = ServerState::new();
    assert_eq!(server.status(), ServerStatus::Starting);
    assert!(!server.is_ready());
    server.start().unwrap();
    assert_eq!(server.status(), ServerStatus::Accepting);
    assert!(server.is_ready());
    server.begin_stop(1000).unwrap();
    assert_eq!(server.status(), ServerStatus::Draining);
    assert!(!server.is_ready());
    server.finish_stop().unwrap();
    server.mark_closed();
    assert_eq!(server.status(), ServerStatus::Stopped);
    let mut failed = ServerState::new();
    failed.mark_failed();
    assert_eq!(failed.status(), ServerStatus::Failed);
    assert!(!failed.is_ready());
}

#[test]
fn server_stats_are_read_only_and_saturating() {
    let mut server = ServerState::new();
    assert_eq!(server.stats().accepted, 0);
    server.start().unwrap();
    server.dispatch("GET", "/", |_| {}).unwrap();
    let stats = server.stats();
    assert_eq!(stats.accepted, 1);
    assert_eq!(stats.active, 0);
    assert_eq!(stats.completed, 1);
    assert!(stats.duration_total_ms <= 1_000);
    assert!(stats.duration_max_ms <= stats.duration_total_ms);
    assert_eq!(stats.duration_total_ms / stats.completed.max(1), 0);
    server.record_request_failure(false, false);
    assert_eq!(server.stats().failed, 1);
    server.record_connection_error(true);
    server.record_connection_error(false);
    assert_eq!(server.stats().timed_out, 1);
    assert_eq!(server.stats().failed, 2);
}

#[test]
fn server_stats_snapshot_supports_concurrent_reads() {
    let server = std::sync::Arc::new(std::sync::Mutex::new(ServerState::new()));
    let readers = (0..8)
        .map(|_| {
            let server = std::sync::Arc::clone(&server);
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let snapshot = server.lock().expect("server lock").stats();
                    assert!(snapshot.active <= snapshot.accepted);
                    assert!(
                        snapshot.duration_max_ms <= snapshot.duration_total_ms
                            || snapshot.duration_total_ms == 0
                    );
                }
            })
        })
        .collect::<Vec<_>>();
    for reader in readers {
        reader.join().expect("stats reader");
    }
}
#[test]
fn canonicalizes_once_and_preserves_query() {
    assert_eq!(canonical_target("/a%20b//x?q=%2F").unwrap().path, "/a b//x");
    assert_eq!(canonical_target("/a%20b//x?q=%2F").unwrap().query, "q=%2F");
}
#[test]
fn rejects_ambiguous_targets() {
    for target in [
        "/a%2Fb", "/../x", "/a%ZZ", "/a\\b", "/a\n", "/a%00", "/a%7F",
    ] {
        assert!(canonical_target(target).is_err());
    }
    assert!(canonical_target(&format!("/{}", "x".repeat(16 * 1024))).is_err());
    assert!(query_values(&format!("x={}", "y".repeat(16 * 1024)), "x").is_err());
}
#[test]
fn literals_outrank_parameters_and_registration_breaks_ties() {
    let routes = vec![
        Route {
            method: "GET".into(),
            pattern: "/x/:id".into(),
            order: 0,
        },
        Route {
            method: "GET".into(),
            pattern: "/x/new".into(),
            order: 1,
        },
    ];
    let (route, params) = select_route(&routes, "GET", "/x/new").unwrap();
    assert_eq!(route.pattern, "/x/new");
    assert!(params.is_empty());
    assert!(select_route(&routes, "GET", "/x/").is_none());
}
#[test]
fn response_cannot_change_after_commit() {
    let mut response = super::Response::new();
    response.set_status(201).unwrap();
    response.set_header("Content-Type", "text/plain").unwrap();
    response.write("ok").unwrap();
    response.commit().unwrap();
    assert!(response.is_committed());
    assert!(response.write("!").is_err());
    assert!(response.set_status(500).is_err());
    assert_eq!(response.body, "ok");
    response.close();
    response.close();
    assert!(response.set_header("X-After", "close").is_err());
}

#[test]
fn head_finishes_without_emitting_body() {
    let mut response = super::Response::new();
    response.write("hidden").unwrap();
    response.finish_for_method("HEAD").unwrap();
    assert!(response.is_committed());
    assert!(response.body.is_empty());
}

#[test]
fn allowed_methods_are_unique_and_sorted() {
    let routes = vec![
        Route {
            method: "POST".into(),
            pattern: "/x/:id".into(),
            order: 0,
        },
        Route {
            method: "GET".into(),
            pattern: "/x/:id".into(),
            order: 1,
        },
        Route {
            method: "GET".into(),
            pattern: "/x/:id".into(),
            order: 2,
        },
    ];
    assert_eq!(allowed_methods(&routes, "/x/7"), vec!["GET", "POST"]);
}

#[test]
fn route_patterns_reject_duplicate_or_malformed_parameters() {
    assert!(super::valid_route_pattern("/users/:id/posts/:post_id"));
    assert!(!super::valid_route_pattern("/users/:id/:id"));
    assert!(!super::valid_route_pattern("users/:id"));
    assert!(!super::valid_route_pattern("/users/:bad-name"));
    assert!(!super::valid_route_pattern("/users/%20name"));
}

#[test]
fn invalid_route_methods_do_not_match_or_pollute_allowed_methods() {
    let routes = vec![
        Route {
            method: "GET".into(),
            pattern: "/x".into(),
            order: 0,
        },
        Route {
            method: "bad method".into(),
            pattern: "/x".into(),
            order: 1,
        },
    ];
    assert!(select_route(&routes, "bad method", "/x").is_none());
    assert_eq!(allowed_methods(&routes, "/x"), vec!["GET"]);
}

#[test]
fn server_lifecycle_bounds_queue_and_cleanup() {
    let mut server = ServerState::new();
    server.add_route("GET".into(), "/health".into()).unwrap();
    server.start().unwrap();
    for _ in 0..128 {
        server.begin_request().unwrap();
    }
    assert_eq!(server.begin_request(), Err("server request queue is full"));
    server.finish_request();
    server.begin_request().unwrap();
    server.stop(1000).unwrap();
    assert_eq!(
        server.begin_request(),
        Err("server is not accepting requests")
    );
    server.close(1000).unwrap();
    assert_eq!(server.routes().len(), 1);
}

#[test]
fn server_admission_bounds_connections_before_worker_spawn() {
    let mut server = ServerState::new();
    server.start().unwrap();
    for _ in 0..crate::config::web_limits().active_connections {
        server.admit_connection().unwrap();
    }
    assert_eq!(
        server.admit_connection(),
        Err("server connection limit reached")
    );
    server.release_connection();
    server.admit_connection().unwrap();
}

#[test]
fn server_admission_rejects_n_plus_one_with_bounded_pool() {
    let mut server = ServerState::new();
    server
        .start_with_options(ServerOptions {
            active_connections: 2,
            worker_count: 1,
            pending_work: 1,
            ..ServerOptions::default()
        })
        .unwrap();
    server.install_worker_pool().unwrap();
    server.admit_connection().unwrap();
    server.admit_connection().unwrap();
    assert_eq!(
        server.admit_connection(),
        Err("server connection limit reached")
    );
    assert_eq!(server.tracked_worker_count(), 1);
    server.begin_stop(1000).unwrap();
    server.release_connection();
    server.release_connection();
    while !server.workers_finished() {
        std::thread::yield_now();
    }
}

#[test]
fn server_worker_capacity_is_checked_before_spawn() {
    let mut server = ServerState::new();
    let options = ServerOptions {
        worker_count: 1,
        ..ServerOptions::default()
    };
    server.start_with_options(options).unwrap();
    let worker = std::thread::spawn(|| std::thread::sleep(std::time::Duration::from_millis(20)));
    server.track_connection_worker(worker);
    assert!(!server.worker_capacity_available());
}

#[test]
fn server_worker_pool_has_fixed_workers_and_bounded_queue() {
    let mut server = ServerState::new();
    server
        .start_with_options(ServerOptions {
            worker_count: 1,
            pending_work: 1,
            ..ServerOptions::default()
        })
        .unwrap();
    server.install_worker_pool().unwrap();
    assert_eq!(server.tracked_worker_count(), 1);

    let (started_sender, started_receiver) = std::sync::mpsc::channel();
    let (release_sender, release_receiver) = std::sync::mpsc::channel();
    server
        .submit_connection_work(Box::new(move || {
            started_sender.send(()).unwrap();
            release_receiver.recv().unwrap();
        }))
        .unwrap();
    started_receiver.recv().unwrap();
    server.submit_connection_work(Box::new(|| {})).unwrap();
    assert_eq!(
        server.submit_connection_work(Box::new(|| {})),
        Err("server worker queue is full")
    );

    server.begin_stop(1000).unwrap();
    release_sender.send(()).unwrap();
    while !server.workers_finished() {
        std::thread::yield_now();
    }
}

#[test]
fn concurrent_handler_slots_are_server_owned_and_bounded() {
    let mut server = ServerState::new();
    server
        .start_with_options(ServerOptions {
            concurrent_handlers: true,
            worker_count: 1,
            ..ServerOptions::default()
        })
        .unwrap();
    server.install_worker_pool().unwrap();
    let slots = server.handler_slots().expect("handler slots");
    let permit = slots.try_acquire().expect("first handler slot");
    assert!(
        slots.try_acquire().is_err(),
        "second handler must be rejected at the cap"
    );
    drop(permit);
    let permit = slots.try_acquire().expect("slot must be reusable");
    drop(permit);
    server.begin_stop(1000).unwrap();
    while !server.workers_finished() {
        std::thread::yield_now();
    }
}

#[test]
fn stop_does_not_claim_success_with_an_active_concurrent_handler() {
    let mut server = ServerState::new();
    server
        .start_with_options(ServerOptions {
            concurrent_handlers: true,
            ..ServerOptions::default()
        })
        .unwrap();
    server.begin_handler_task();
    server.begin_stop(1_000).unwrap();
    assert_eq!(
        server.finish_stop(),
        Err("server drain timed out with active handlers")
    );
    server.finish_handler_task();
    server.finish_stop().expect("handler drain completed");
}

#[test]
fn server_stop_does_not_claim_success_before_connections_drain() {
    let mut server = ServerState::new();
    server.start().unwrap();
    server.admit_connection().unwrap();

    assert_eq!(
        server.stop(1000),
        Err("server drain timed out with active connections")
    );
    assert_eq!(server.active_connections(), 1);
    assert_eq!(
        server.close(1000),
        Err("server drain timed out with active connections")
    );

    server.release_connection();
    server.stop(1000).unwrap();
    server.close(1000).unwrap();
}

#[test]
fn server_reaps_finished_connection_workers() {
    let mut server = ServerState::new();
    let worker = std::thread::spawn(|| {});
    server.track_connection_worker(worker);
    std::thread::yield_now();
    server.reap_finished_workers();
    assert_eq!(server.active_connections(), 0);
}

#[test]
fn server_stop_cancels_an_admitted_socket_without_sleep() {
    let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind cancellation listener: {error}"),
    };
    let endpoint = listener
        .local_addr()
        .expect("cancellation listener address");
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect cancellation peer");
    let (accepted, _) = listener.accept().expect("accept cancellation peer");
    let accepted = crate::net::TcpStream::from_std(accepted);
    let mut server = ServerState::new();
    server.start().unwrap();
    server.admit_connection().unwrap();
    assert!(server.track_connection_socket(&accepted));

    server.begin_stop(1000).unwrap();
    let mut buffer = [0_u8; 1];
    assert_eq!(client.read(&mut buffer).unwrap(), 0);
    server.release_connection();
    server.finish_stop().unwrap();
}

#[test]
fn server_stop_is_idempotent_after_drain() {
    let mut server = ServerState::new();
    server.start().unwrap();
    server.stop(1000).unwrap();
    server.stop(1000).unwrap();
    server.close(1000).unwrap();
    server.close(1000).unwrap();
}

#[test]
fn failed_connection_worker_is_reaped_without_leaking_admission() {
    let state = std::sync::Arc::new(std::sync::Mutex::new(ServerState::new()));
    {
        let mut server = state.lock().unwrap();
        server.start().unwrap();
        server.admit_connection().unwrap();
    }
    let worker_state = state.clone();
    let (failed_sender, failed_receiver) = std::sync::mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        ServerState::run_connection_worker(&worker_state, || {
            panic!("controlled connection worker failure");
        });
        failed_sender.send(()).unwrap();
    });
    state.lock().unwrap().track_connection_worker(worker);
    failed_receiver.recv().unwrap();
    let mut server = state.lock().unwrap();
    for _ in 0..1000 {
        server.reap_finished_workers();
        if server.tracked_worker_count() == 0 {
            assert_eq!(server.active_connections(), 0);
            return;
        }
        drop(server);
        std::thread::yield_now();
        server = state.lock().unwrap();
    }
    panic!("failed connection worker was not reaped");
}

#[test]
fn server_stop_cancels_multiple_admitted_sockets() {
    let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind multi-cancellation listener: {error}"),
    };
    let endpoint = listener
        .local_addr()
        .expect("multi-cancellation listener address");
    let mut clients = Vec::new();
    let mut accepted = Vec::new();
    for _ in 0..2 {
        clients
            .push(std::net::TcpStream::connect(endpoint).expect("connect multi-cancellation peer"));
        accepted.push(crate::net::TcpStream::from_std(
            listener.accept().expect("accept multi-cancellation peer").0,
        ));
    }
    let mut server = ServerState::new();
    server.start().unwrap();
    for stream in &accepted {
        server.admit_connection().unwrap();
        assert!(server.track_connection_socket(stream));
    }
    server.begin_stop(1000).unwrap();
    for mut client in clients {
        let mut buffer = [0_u8; 1];
        assert_eq!(client.read(&mut buffer).unwrap(), 0);
        server.release_connection();
    }
    server.finish_stop().unwrap();
}

#[test]
fn server_stop_drains_a_slow_http_worker() {
    let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind slow-worker listener: {error}"),
    };
    let endpoint = listener.local_addr().expect("slow-worker listener address");
    let mut client = std::net::TcpStream::connect(endpoint).expect("connect slow-worker peer");
    let (accepted, _) = listener.accept().expect("accept slow-worker peer");
    let accepted = crate::net::TcpStream::from_std(accepted);
    let state = std::sync::Arc::new(std::sync::Mutex::new(ServerState::new()));
    {
        let mut server = state.lock().unwrap();
        server.start().unwrap();
        server.admit_connection().unwrap();
        assert!(server.track_connection_socket(&accepted));
    }
    let worker_state = state.clone();
    let (worker_done_sender, worker_done_receiver) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let result = crate::http::serve_connection(accepted, worker_state)
            .map_err(|error| error.to_string());
        worker_done_sender
            .send(result)
            .expect("slow worker completion receiver");
    });
    client
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\n")
        .expect("write partial request");
    {
        let mut server = state.lock().unwrap();
        server.track_connection_worker(worker);
        server.begin_stop(1000).unwrap();
    }

    let worker_result = worker_done_receiver
        .recv_timeout(std::time::Duration::from_secs(1))
        .expect("slow HTTP worker did not stop after socket cancellation");
    let mut server = state.lock().unwrap();
    server.reap_finished_workers();
    let expected_cancellation = worker_result
        .as_ref()
        .err()
        .is_none_or(|error| error.contains("IncompleteMessage"));
    assert!(
        expected_cancellation,
        "unexpected slow worker result: {worker_result:?}"
    );
    assert_eq!(server.active_connections(), 0);
    server.finish_stop().unwrap();
}

#[test]
fn server_dispatch_releases_queue_and_returns_route_outcomes() {
    let mut server = ServerState::new();
    server.add_route("GET".into(), "/health".into()).unwrap();
    server.start().unwrap();
    let mut matched = false;
    server
        .dispatch("GET", "/health", |outcome| {
            matched = matches!(outcome, RouteOutcome::Matched(_, _));
        })
        .unwrap();
    assert!(matched);
    let mut method_not_allowed = false;
    server
        .dispatch("POST", "/health", |outcome| {
            method_not_allowed = matches!(outcome, RouteOutcome::MethodNotAllowed(_));
        })
        .unwrap();
    assert!(method_not_allowed);
    server.stop(1000).unwrap();
    assert_eq!(
        server.dispatch("GET", "/health", |_| {}),
        Err("server is not accepting requests")
    );
}

#[test]
fn head_reuses_get_route() {
    let routes = vec![Route {
        method: "GET".into(),
        pattern: "/health".into(),
        order: 0,
    }];
    let (route, _) = route_for_request(&routes, "HEAD", "/health").unwrap();
    assert_eq!(route.method, "GET");
}

#[test]
fn dispatch_distinguishes_not_found_and_method_not_allowed() {
    let routes = vec![Route {
        method: "GET".into(),
        pattern: "/health".into(),
        order: 0,
    }];
    assert!(matches!(
        dispatch_route(&routes, "GET", "/health"),
        RouteOutcome::Matched(_, _)
    ));
    assert_eq!(
        dispatch_route(&routes, "POST", "/health"),
        RouteOutcome::MethodNotAllowed(vec!["GET".into()])
    );
    assert_eq!(
        dispatch_route(&routes, "GET", "/missing"),
        RouteOutcome::NotFound
    );
}

#[test]
fn query_preserves_duplicates_and_plus() {
    assert_eq!(query_values("a=1&a=2&x=a+b", "a").unwrap(), vec!["1", "2"]);
    assert_eq!(query_values("x=a+b", "x").unwrap(), vec!["a+b"]);
    let values = super::QueryValues::from_query("a=1&a=2", "a").unwrap();
    assert_eq!(values.count(), 2);
    assert_eq!(values.get(1), Some("2"));
    assert_eq!(values.get(2), None);
}

#[test]
fn headers_are_case_insensitive_and_preserve_duplicates() {
    let headers = vec![
        ("X-Test".into(), "one".into()),
        ("x-test".into(), "two".into()),
    ];
    assert_eq!(
        header_values(&headers, "X-TEST").unwrap(),
        vec!["one", "two"]
    );
    assert!(header_values(&headers, "bad:name").is_err());
    let values = super::HeaderValues::from_headers(&headers, "x-test").unwrap();
    assert_eq!(values.count(), 2);
    assert_eq!(values.get(0), Some("one"));
}

#[test]
fn body_limit_rejects_truncation_and_invalid_bounds() {
    assert_eq!(bounded_body("abc", 3).unwrap(), "abc");
    assert!(bounded_body("abcd", 3).is_err());
    assert!(bounded_body("", -1).is_err());
}

#[test]
fn request_canonicalizes_target_and_exposes_bounded_collections() {
    let request = super::Request::new(
        "GET",
        "/x?a=1&a=2",
        vec![("X-ID".into(), "7".into())],
        "body",
        "192.0.2.1".parse().unwrap(),
    )
    .unwrap();
    assert_eq!(request.target.path, "/x");
    assert_eq!(request.method(), "GET");
    assert_eq!(request.target(), "/x");
    assert_eq!(request.body(4).unwrap(), "body");
    assert_eq!(
        request.peer_address(),
        "192.0.2.1".parse::<IpAddr>().unwrap()
    );
    assert_eq!(
        request.effective_client_address(false),
        "192.0.2.1".parse::<IpAddr>().unwrap()
    );
    assert_eq!(request.query("a").unwrap().count(), 2);
    assert_eq!(request.header("x-id").unwrap().get(0), Some("7"));
    assert!(super::Request::new("get", "/", Vec::new(), "", "192.0.2.1".parse().unwrap()).is_err());
    assert!(
        super::Request::new(
            "GET",
            "/",
            Vec::new(),
            &"x".repeat(8 * 1024 * 1024 + 1),
            "192.0.2.1".parse().unwrap()
        )
        .is_err()
    );
}

#[test]
fn request_enforces_header_and_query_bounds() {
    let headers = vec![("x-test".into(), "a".repeat(64 * 1024))];
    assert!(super::Request::new("GET", "/", headers, "", "127.0.0.1".parse().unwrap()).is_err());
    let query = (0..101)
        .map(|index| format!("k{index}=v"))
        .collect::<Vec<_>>()
        .join("&");
    assert!(query_values(&query, "missing").is_err());
}

#[test]
fn client_url_validation_rejects_unsafe_forms() {
    assert!(validate_client_url("https://example.test/path").is_ok());
    for url in [
        "ftp://example.test",
        "https:///path",
        "https://u:p@example.test",
        "https://example.test/#frag",
    ] {
        assert!(validate_client_url(url).is_err());
    }
}

#[test]
fn ssrf_guard_rejects_private_and_empty_resolution_by_default() {
    assert!(validate_ssrf_destinations(&[], false).is_err());
    assert!(validate_ssrf_destinations(&["127.0.0.1".parse().unwrap()], false).is_err());
    assert!(validate_ssrf_destinations(&["192.0.2.1".parse().unwrap()], false).is_err());
    assert!(validate_ssrf_destinations(&["127.0.0.1".parse().unwrap()], true).is_ok());
}

#[test]
fn ssrf_guard_reclassifies_ipv4_mapped_ipv6_as_ipv4() {
    for address in [
        "::ffff:127.0.0.1",
        "::ffff:10.0.0.1",
        "::ffff:169.254.169.254",
        "::ffff:100.64.0.1",
    ] {
        assert!(
            validate_ssrf_destinations(&[address.parse().unwrap()], false).is_err(),
            "{address} must use IPv4 sensitivity rules"
        );
    }
}

#[test]
fn ssrf_guard_rejects_special_ipv4_ranges_but_allows_global_ipv4() {
    for address in [
        "100.64.0.1",
        "100.127.255.254",
        "192.0.2.1",
        "198.18.0.1",
        "198.51.100.1",
        "203.0.113.1",
    ] {
        assert!(
            validate_ssrf_destinations(&[address.parse().unwrap()], false).is_err(),
            "{address} must be rejected"
        );
    }
    assert!(validate_ssrf_destinations(&["8.8.8.8".parse().unwrap()], false).is_ok());
    assert!(validate_ssrf_destinations(&["2001:4860:4860::8888".parse().unwrap()], false).is_ok());
}

#[test]
fn egress_policy_is_immutable_and_fail_closed() {
    let policy = super::EgressPolicy::new(
        Some(vec!["http".into()]),
        Some(vec![crate::net::Cidr::parse("93.184.216.0/24").unwrap()]),
        Some(vec![80]),
        2,
        1000,
    )
    .unwrap();
    assert!(
        policy
            .validate("http", 80, &["93.184.216.34".parse().unwrap()])
            .is_ok()
    );
    assert!(
        policy
            .validate("http", 443, &["93.184.216.34".parse().unwrap()])
            .is_err()
    );
    assert!(
        policy
            .validate("http", 80, &["100.64.0.1".parse().unwrap()])
            .is_err()
    );
    assert!(super::EgressPolicy::new(Some(vec!["ftp".into()]), None, None, 1, 1000).is_err());
    let empty = super::EgressPolicy::from_csv("http", "", "", 1, 1000).unwrap();
    assert!(
        empty
            .validate("http", 80, &["93.184.216.34".parse().unwrap()])
            .is_err()
    );
    let too_many = std::iter::repeat_n("http", crate::config::web_limits().egress_list_max + 1)
        .collect::<Vec<_>>()
        .join(",");
    assert!(super::EgressPolicy::from_csv(&too_many, "", "", 1, 1000).is_err());
}

#[test]
fn ssrf_policy_can_be_exercised_with_the_local_fake_resolver() {
    let resolver = crate::test_support::FakeResolver::default();
    resolver.insert(
        "metadata.test",
        vec!["::ffff:169.254.169.254".parse().unwrap()],
    );
    let addresses = resolver
        .resolve(
            "metadata.test",
            crate::config::web_limits().resolved_addresses_max,
        )
        .unwrap();
    let addresses = addresses
        .into_iter()
        .map(crate::net::Address::as_std)
        .collect::<Vec<_>>();
    assert!(validate_ssrf_destinations(&addresses, false).is_err());
}

#[test]
fn proxy_provenance_requires_explicit_trust_and_valid_ip() {
    let peer = "192.0.2.10".parse().unwrap();
    assert_eq!(
        effective_client_address(peer, Some("198.51.100.4"), true),
        "198.51.100.4".parse::<IpAddr>().unwrap()
    );
    assert_eq!(
        effective_client_address(peer, Some("198.51.100.4"), false),
        peer
    );
    assert_eq!(
        effective_client_address(peer, Some("not-an-ip"), true),
        peer
    );
}

#[test]
fn request_does_not_trust_forwarded_for_by_default() {
    let peer = "192.0.2.10".parse().unwrap();
    let request = super::Request::new(
        "GET",
        "/",
        vec![("X-Forwarded-For".into(), "198.51.100.4".into())],
        "",
        peer,
    )
    .unwrap();
    assert_eq!(request.peer_address(), peer);
    assert_eq!(request.effective_client_address(false), peer);
}
