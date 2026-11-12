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
