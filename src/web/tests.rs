use super::{
    Route, RouteOutcome, ServerState, allowed_methods, bounded_body, canonical_target,
    dispatch_route, effective_client_address, header_values, query_values, route_for_request,
    select_route, validate_client_url, validate_ssrf_destinations,
};
use std::net::IpAddr;
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
    assert!(validate_ssrf_destinations(&["192.0.2.1".parse().unwrap()], false).is_ok());
    assert!(validate_ssrf_destinations(&["127.0.0.1".parse().unwrap()], true).is_ok());
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
