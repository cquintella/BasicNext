// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNWeb` `Error`s (`language/0.6/bnweb.md` "Errors"), one producer for both
//! backends: the interpreter turns a [`WebFailure`] into an `Error` value,
//! and native tooling uses the same code and message definitions.

use bn_types::error_codes::web;

/// Why a `BNWeb` operation failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WebFailure {
    // 1: INVALID_ARGUMENT
    /// Generic or detailed invalid argument.
    InvalidArgument(String),
    /// Method outside standard HTTP methods.
    UnsupportedMethod(String),
    /// Status code outside 100..599.
    InvalidStatus(i128),
    /// Malformed or unsupported URL.
    InvalidUrl(String),
    /// Invalid header name or value.
    InvalidHeader(String),
    /// Invalid route pattern or method.
    InvalidRoute(String),
    /// Cookie age cannot be negative.
    NegativeCookieAge,
    /// `SameSite` policy string is not Strict, Lax, or None.
    InvalidSameSite,
    /// `SameSite=None` requires Secure cookie attribute.
    SameSiteNoneRequiresSecure,
    /// Session idle timeout is invalid (<= 0 or > 30 minutes).
    InvalidSessionIdleTimeout,
    /// Session store capacity is invalid (0 or > 10,000).
    InvalidSessionCapacity,
    /// Filter argument must be a FUNCTION value.
    FilterMustBeFunction,
    /// Route handler argument must be a FUNCTION value.
    RouteHandlerMustBeFunction,
    /// CIDR string or prefix is invalid.
    InvalidCidr(String),
    /// Cookie name/value/domain/path exceeded bounds.
    InvalidCookieBounds,
    /// Scraper selector is unsupported.
    UnsupportedSelector(String),
    /// Scraper HTML source is malformed.
    MalformedHtml,
    /// Server options outside limits.
    InvalidServerOptions,
    /// Server is already started.
    ServerAlreadyStarted,

    // 2: NOT_FOUND
    /// Requested entity was not found.
    NotFound(String),
    /// Cookie not found in jar.
    CookieNotFound,
    /// Session identifier not found or expired.
    SessionNotFound,
    /// Header not found in response.
    HeaderNotFound(String),
    /// Route not matched.
    RouteNotFound(String),
    /// Scraper selector did not match.
    SelectorNoMatch,

    // 3: OUT_OF_RANGE
    /// Collection index out of bounds.
    IndexOutOfRange { index: i128, count: usize },
    /// General out-of-range error.
    OutOfRange(String),

    // 4: LIMIT
    /// Limit exceeded.
    LimitExceeded(String),
    /// Server filter limit (64) reached.
    FilterLimitExceeded,
    /// Request or response body exceeded limit.
    BodyLimitExceeded(String),
    /// Egress redirect limit exceeded.
    RedirectLimitExceeded,
    /// Headers size or count exceeded limit.
    HeaderLimitExceeded,
    /// Session value exceeded 8 KiB.
    SessionValueTooLarge,

    // 5: CLOSED
    /// Resource is closed.
    Closed(String),
    /// Server is not accepting requests (stopping, stopped, or not started).
    ServerNotAccepting,
    /// Server is stopping or closed.
    ServerStoppingOrClosed,
    /// Response was already committed or closed.
    ResponseCommittedOrClosed,
    /// Response is closed.
    ResponseClosed,

    // 6: TIMEOUT
    /// Timeout exceeded.
    Timeout(String),
    /// HTTP handshake timed out.
    HandshakeTimeout,
    /// Server listener join timed out.
    ListenerJoinTimeout,
    /// Server worker drain timed out.
    WorkerDrainTimeout,

    // 7: EGRESS_DENIED
    /// Destination IP or redirect denied by egress policy.
    EgressDenied(String),
    /// SSRF policy denied local or private address.
    SsrfDenied(String),

    // 8: HTTP_FAILED
    /// Outbound request failed.
    HttpFailed(String),
    /// TLS handshake failed.
    TlsHandshakeFailed(String),
    /// Connection to remote destination failed.
    ConnectionFailed(String),
    /// Destination host resolved to no IP addresses.
    NoResolvedAddresses,
    /// Redirect response lacked Location header.
    MissingRedirectLocation,
    /// Inbound request dispatch failed.
    RequestDispatchFailed,
    /// Server listener thread join failed.
    ListenerJoinFailed,

    // 9: UNAVAILABLE
    /// Feature or provider is unavailable in this build.
    Unavailable(String),
    /// Named provider is unavailable.
    ProviderUnavailable(&'static str),
    /// Server internal state mutex is unavailable.
    ServerStateUnavailable,
}

impl WebFailure {
    /// `Error.Code`.
    #[must_use]
    pub const fn code(&self) -> i32 {
        match self {
            Self::InvalidArgument(_)
            | Self::UnsupportedMethod(_)
            | Self::InvalidStatus(_)
            | Self::InvalidUrl(_)
            | Self::InvalidHeader(_)
            | Self::InvalidRoute(_)
            | Self::NegativeCookieAge
            | Self::InvalidSameSite
            | Self::SameSiteNoneRequiresSecure
            | Self::InvalidSessionIdleTimeout
            | Self::InvalidSessionCapacity
            | Self::FilterMustBeFunction
            | Self::RouteHandlerMustBeFunction
            | Self::InvalidCidr(_)
            | Self::InvalidCookieBounds
            | Self::UnsupportedSelector(_)
            | Self::MalformedHtml
            | Self::InvalidServerOptions
            | Self::ServerAlreadyStarted => web::INVALID_ARGUMENT,

            Self::NotFound(_)
            | Self::CookieNotFound
            | Self::SessionNotFound
            | Self::HeaderNotFound(_)
            | Self::RouteNotFound(_)
            | Self::SelectorNoMatch => web::NOT_FOUND,

            Self::IndexOutOfRange { .. } | Self::OutOfRange(_) => web::OUT_OF_RANGE,

            Self::LimitExceeded(_)
            | Self::FilterLimitExceeded
            | Self::BodyLimitExceeded(_)
            | Self::RedirectLimitExceeded
            | Self::HeaderLimitExceeded
            | Self::SessionValueTooLarge => web::LIMIT,

            Self::Closed(_)
            | Self::ServerNotAccepting
            | Self::ServerStoppingOrClosed
            | Self::ResponseCommittedOrClosed
            | Self::ResponseClosed => web::CLOSED,

            Self::Timeout(_)
            | Self::HandshakeTimeout
            | Self::ListenerJoinTimeout
            | Self::WorkerDrainTimeout => web::TIMEOUT,

            Self::EgressDenied(_) | Self::SsrfDenied(_) => web::EGRESS_DENIED,

            Self::HttpFailed(_)
            | Self::TlsHandshakeFailed(_)
            | Self::ConnectionFailed(_)
            | Self::NoResolvedAddresses
            | Self::MissingRedirectLocation
            | Self::RequestDispatchFailed
            | Self::ListenerJoinFailed => web::HTTP_FAILED,

            Self::Unavailable(_) | Self::ProviderUnavailable(_) | Self::ServerStateUnavailable => {
                web::UNAVAILABLE
            }
        }
    }

    /// `Error.Message`: what failed.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::InvalidArgument(reason)
            | Self::InvalidUrl(reason)
            | Self::InvalidHeader(reason)
            | Self::InvalidRoute(reason)
            | Self::InvalidCidr(reason)
            | Self::NotFound(reason)
            | Self::HeaderNotFound(reason)
            | Self::RouteNotFound(reason)
            | Self::OutOfRange(reason)
            | Self::LimitExceeded(reason)
            | Self::BodyLimitExceeded(reason)
            | Self::Closed(reason)
            | Self::Timeout(reason)
            | Self::EgressDenied(reason)
            | Self::SsrfDenied(reason)
            | Self::HttpFailed(reason)
            | Self::TlsHandshakeFailed(reason)
            | Self::ConnectionFailed(reason)
            | Self::Unavailable(reason) => reason.clone(),

            Self::UnsupportedMethod(m) => format!("unsupported HTTP method \"{m}\""),
            Self::InvalidStatus(s) => {
                format!("status {s} outside valid HTTP status range 100..599")
            }
            Self::NegativeCookieAge => "negative cookie age".into(),
            Self::InvalidSameSite => "invalid SameSite policy".into(),
            Self::SameSiteNoneRequiresSecure => "SameSite=None requires Secure".into(),
            Self::InvalidSessionIdleTimeout => "invalid session idle timeout".into(),
            Self::InvalidSessionCapacity => "invalid session capacity".into(),
            Self::FilterMustBeFunction => "filter must be a FUNCTION".into(),
            Self::RouteHandlerMustBeFunction => "route handler must be a FUNCTION".into(),
            Self::InvalidCookieBounds => "invalid cookie bounds".into(),
            Self::UnsupportedSelector(s) => format!("unsupported selector \"{s}\""),
            Self::MalformedHtml => "malformed HTML".into(),
            Self::InvalidServerOptions => "invalid server options".into(),
            Self::ServerAlreadyStarted => "server is already started".into(),

            Self::CookieNotFound => "cookie not found".into(),
            Self::SessionNotFound => "session not found".into(),
            Self::SelectorNoMatch => "selector did not match".into(),

            Self::IndexOutOfRange { index, count } => {
                format!("index {index} is outside collection (count {count})")
            }

            Self::FilterLimitExceeded => "filter limit exceeded".into(),
            Self::RedirectLimitExceeded => "redirect limit exceeded".into(),
            Self::HeaderLimitExceeded => "headers exceed declared limit".into(),
            Self::SessionValueTooLarge => "session value too large".into(),

            Self::ServerNotAccepting => "server is not accepting requests".into(),
            Self::ServerStoppingOrClosed => "server is stopping or closed".into(),
            Self::ResponseCommittedOrClosed => "response is already committed or closed".into(),
            Self::ResponseClosed => "response is closed".into(),

            Self::HandshakeTimeout => "HTTP handshake timed out".into(),
            Self::ListenerJoinTimeout => "server listener join timed out".into(),
            Self::WorkerDrainTimeout => "server worker drain timed out".into(),

            Self::NoResolvedAddresses => "URL resolved to no addresses".into(),
            Self::MissingRedirectLocation => "redirect response missing Location".into(),
            Self::RequestDispatchFailed => "request dispatch failed".into(),
            Self::ListenerJoinFailed => "server listener join failed".into(),

            Self::ProviderUnavailable(p) => format!("{p} provider unavailable"),
            Self::ServerStateUnavailable => "server state unavailable".into(),
        }
    }

    /// `Error.Cause`: the violated rule or underlying cause.
    #[must_use]
    pub fn cause(&self) -> String {
        match self {
            Self::InvalidArgument(reason)
            | Self::InvalidUrl(reason)
            | Self::InvalidHeader(reason)
            | Self::InvalidRoute(reason)
            | Self::InvalidCidr(reason)
            | Self::NotFound(reason)
            | Self::HeaderNotFound(reason)
            | Self::RouteNotFound(reason)
            | Self::OutOfRange(reason)
            | Self::LimitExceeded(reason)
            | Self::BodyLimitExceeded(reason)
            | Self::Closed(reason)
            | Self::Timeout(reason)
            | Self::EgressDenied(reason)
            | Self::SsrfDenied(reason)
            | Self::HttpFailed(reason)
            | Self::TlsHandshakeFailed(reason)
            | Self::ConnectionFailed(reason)
            | Self::Unavailable(reason) => reason.clone(),

            Self::UnsupportedMethod(_) => {
                "HTTP method must be GET, POST, PUT, PATCH, DELETE, or HEAD".into()
            }
            Self::InvalidStatus(_) => "HTTP status code must be from 100 through 599".into(),
            Self::NegativeCookieAge => "cookie max age must be non-negative".into(),
            Self::InvalidSameSite => "SameSite policy must be Strict, Lax, or None".into(),
            Self::SameSiteNoneRequiresSecure => {
                "SameSite=None cookies must also declare the Secure attribute".into()
            }
            Self::InvalidSessionIdleTimeout => {
                "session idle timeout must be between 1 ms and 30 minutes".into()
            }
            Self::InvalidSessionCapacity => "session capacity must be between 1 and 10000".into(),
            Self::FilterMustBeFunction => "server filters must be typed function references".into(),
            Self::RouteHandlerMustBeFunction => {
                "route handlers must be typed function references".into()
            }
            Self::InvalidCookieBounds => {
                "cookie name, value, domain, or path exceeded bounds".into()
            }
            Self::UnsupportedSelector(_) => {
                "selectors must be non-empty alphanumeric tags excluding script".into()
            }
            Self::MalformedHtml => "HTML markup could not be parsed".into(),
            Self::InvalidServerOptions => {
                "server options exceed permitted configuration limits".into()
            }
            Self::ServerAlreadyStarted => "server cannot be started while already running".into(),

            Self::CookieNotFound => {
                "the cookie does not exist in this jar for the given path".into()
            }
            Self::SessionNotFound => "the session does not exist or has expired".into(),
            Self::SelectorNoMatch => {
                "the selector did not match any element in the HTML document".into()
            }

            Self::IndexOutOfRange { .. } => {
                "the collection index is outside the valid range".into()
            }

            Self::FilterLimitExceeded => "a server may register at most 64 filters".into(),
            Self::RedirectLimitExceeded => {
                "the request exceeded the maximum number of allowed redirects".into()
            }
            Self::HeaderLimitExceeded => {
                "total header size or count exceeded permitted bounds".into()
            }
            Self::SessionValueTooLarge => "session values must not exceed 8192 bytes".into(),

            Self::ServerNotAccepting => {
                "the server is not currently running or accepting inbound connections".into()
            }
            Self::ServerStoppingOrClosed => "the server is stopping or has closed".into(),
            Self::ResponseCommittedOrClosed => {
                "a committed or closed response cannot be modified".into()
            }
            Self::ResponseClosed => "the response stream has been closed".into(),

            Self::HandshakeTimeout => {
                "the remote host did not complete the HTTP handshake in time".into()
            }
            Self::ListenerJoinTimeout => "the server listener thread did not exit in time".into(),
            Self::WorkerDrainTimeout => "server worker tasks did not finish within timeout".into(),

            Self::NoResolvedAddresses => {
                "DNS resolution yielded no IP addresses for the target".into()
            }
            Self::MissingRedirectLocation => {
                "a 3xx redirect response did not include a Location header".into()
            }
            Self::RequestDispatchFailed => "an error occurred while dispatching the request".into(),
            Self::ListenerJoinFailed => "the server listener thread panicked or failed".into(),

            Self::ProviderUnavailable(p) => {
                format!("the {p} provider is not available on this platform or build")
            }
            Self::ServerStateUnavailable => "could not acquire internal server state lock".into(),
        }
    }

    /// Construct a [`WebFailure`] from a runtime error string.
    #[must_use]
    pub fn from_message(message: &str) -> Self {
        match message {
            "unsupported HTTP method" => Self::UnsupportedMethod(message.into()),
            "status must be 100..599" | "invalid response status" => Self::InvalidStatus(0),
            "filter must be a FUNCTION" => Self::FilterMustBeFunction,
            "route handler must be a FUNCTION" => Self::RouteHandlerMustBeFunction,
            "filter limit exceeded" => Self::FilterLimitExceeded,
            "invalid route" => Self::InvalidRoute(message.into()),
            "server is already started" => Self::ServerAlreadyStarted,
            "server is stopping or closed" => Self::ServerStoppingOrClosed,
            "server is not accepting requests" => Self::ServerNotAccepting,
            "invalid server options" => Self::InvalidServerOptions,
            "server listener join timed out" => Self::ListenerJoinTimeout,
            "server listener join failed" => Self::ListenerJoinFailed,
            "server worker drain timed out" | "server drain timed out with active connections" => {
                Self::WorkerDrainTimeout
            }
            "server state unavailable" => Self::ServerStateUnavailable,
            "negative cookie age" => Self::NegativeCookieAge,
            "invalid SameSite policy" => Self::InvalidSameSite,
            "SameSite=None requires Secure" => Self::SameSiteNoneRequiresSecure,
            "cookie not found" => Self::CookieNotFound,
            "invalid cookie bounds" => Self::InvalidCookieBounds,
            "invalid session idle timeout" => Self::InvalidSessionIdleTimeout,
            "invalid session capacity" => Self::InvalidSessionCapacity,
            "session not found" => Self::SessionNotFound,
            "session value too large" => Self::SessionValueTooLarge,
            "unsupported selector" => Self::UnsupportedSelector(message.into()),
            "selector did not match" => Self::SelectorNoMatch,
            "malformed HTML" => Self::MalformedHtml,
            "response is already committed or closed" => Self::ResponseCommittedOrClosed,
            "response is closed" => Self::ResponseClosed,
            "response header is not present" => Self::HeaderNotFound(message.into()),
            "redirect limit exceeded" => Self::RedirectLimitExceeded,
            "redirect response missing Location" => Self::MissingRedirectLocation,
            "HTTP handshake timed out" => Self::HandshakeTimeout,
            "URL resolved to no addresses" => Self::NoResolvedAddresses,
            "request dispatch failed" => Self::RequestDispatchFailed,
            "index is outside collection" => Self::IndexOutOfRange {
                index: -1,
                count: 0,
            },
            s if s.contains("provider unavailable") => Self::Unavailable(s.into()),
            s if s.contains("destinations")
                || s.contains("private or local")
                || s.contains("egress") =>
            {
                Self::EgressDenied(s.into())
            }
            s if s.contains("TLS handshake failed") => Self::TlsHandshakeFailed(s.into()),
            s if s.contains("connect") => Self::ConnectionFailed(s.into()),
            s if s.contains("exceed")
                || s.contains("limit")
                || s.contains("too large")
                || s.contains("scripts are not allowed") =>
            {
                Self::BodyLimitExceeded(s.into())
            }
            s if s.contains("URL")
                || s.contains("authority")
                || s.contains("percent escape")
                || s.contains("UTF-8 in target") =>
            {
                Self::InvalidUrl(s.into())
            }
            s if s.contains("header") => Self::InvalidHeader(s.into()),
            s if s.contains("CIDR") => Self::InvalidCidr(s.into()),
            other => Self::InvalidArgument(other.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WebFailure;
    use bn_types::error_codes::web;

    #[test]
    fn failures_carry_code_message_and_cause() {
        let inv = WebFailure::UnsupportedMethod("INVALID".into());
        assert_eq!(inv.code(), web::INVALID_ARGUMENT);
        assert_eq!(inv.message(), "unsupported HTTP method \"INVALID\"");
        assert!(!inv.cause().is_empty());

        let nf = WebFailure::CookieNotFound;
        assert_eq!(nf.code(), web::NOT_FOUND);
        assert_eq!(nf.message(), "cookie not found");

        let oor = WebFailure::IndexOutOfRange { index: 5, count: 2 };
        assert_eq!(oor.code(), web::OUT_OF_RANGE);

        let lim = WebFailure::FilterLimitExceeded;
        assert_eq!(lim.code(), web::LIMIT);

        let cls = WebFailure::ServerNotAccepting;
        assert_eq!(cls.code(), web::CLOSED);

        let to = WebFailure::HandshakeTimeout;
        assert_eq!(to.code(), web::TIMEOUT);

        let egress = WebFailure::SsrfDenied("blocked private address".into());
        assert_eq!(egress.code(), web::EGRESS_DENIED);

        let http = WebFailure::NoResolvedAddresses;
        assert_eq!(http.code(), web::HTTP_FAILED);

        let unavail = WebFailure::ProviderUnavailable("BNWeb");
        assert_eq!(unavail.code(), web::UNAVAILABLE);
    }
}
