// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! One client transfer for both backends. A `BNWeb.Client` verb describes a
//! [`Transfer`]; the rules it is held to (the `maximum` a program passes, the
//! content type merged into its headers, the policy of a client without one,
//! the multipart body of a form) live here, and the two adapters only turn
//! handles and values into a `Transfer` and its outcome back into theirs.

use super::{ClientReply, EgressPolicy, Form};
use crate::web_error::{SizeLimit, WebFailure};

/// The boundary between the parts of a multipart form body.
const FORM_BOUNDARY: &str = "----BNWebFormBoundary7MA4YWxkTrZu0gW";
/// The header a body's media type travels in.
const CONTENT_TYPE: &str = "content-type";
/// The name a file part gets when its path has none.
const DEFAULT_FILE_NAME: &str = "file";

/// One outbound exchange, as a `BNWeb.Client` verb describes it.
#[derive(Debug)]
pub struct Transfer<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub body: &'a [u8],
    /// The media type of `body`, sent unless `headers` already names one.
    pub content_type: Option<&'a str>,
    pub headers: Vec<(String, String)>,
    /// The `maximum` the program passed, unchecked.
    pub maximum: i64,
    /// The client's policy; `None` is a client without one, which connects to
    /// any resolved destination.
    pub policy: Option<&'a EgressPolicy>,
}

/// What a transfer leaves for the access log of its client.
#[derive(Debug, Default)]
pub struct ClientAccess {
    pub status: i64,
    pub bytes: i64,
    pub duration_ms: i64,
    pub headers: Vec<(String, String)>,
}

/// The outcome of a [`Transfer`]: the reply or its failure, and the access
/// record when the exchange was attempted.
#[derive(Debug)]
pub struct TransferOutcome {
    pub result: Result<ClientReply, WebFailure>,
    pub access: Option<ClientAccess>,
}

impl Transfer<'_> {
    /// Performs the exchange. A `maximum` outside its bounds fails before any
    /// connection and leaves no access record.
    #[must_use]
    pub fn run(self) -> TransferOutcome {
        let maximum = match body_maximum(self.maximum) {
            Ok(maximum) => maximum,
            Err(failure) => {
                return TransferOutcome {
                    result: Err(failure),
                    access: None,
                };
            }
        };
        let headers = with_content_type(self.headers, self.content_type);
        let default_policy = EgressPolicy::default();
        let policy = self.policy.unwrap_or(&default_policy);
        let started = std::time::Instant::now();
        let outcome = crate::http::client_transfer_with_policy(
            self.method,
            self.url,
            self.body,
            &headers,
            policy,
        );
        let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
        let access = match &outcome {
            Ok(reply) => ClientAccess {
                status: i64::from(reply.status),
                bytes: i64::try_from(reply.body.len()).unwrap_or(i64::MAX),
                duration_ms,
                headers: reply.headers.clone(),
            },
            Err(_) => ClientAccess {
                duration_ms,
                ..ClientAccess::default()
            },
        };
        let result = outcome.and_then(|reply| {
            if reply.body.len() > maximum {
                Err(WebFailure::BodyLimitExceeded(format!(
                    "{}/{maximum}",
                    reply.body.len()
                )))
            } else {
                Ok(reply)
            }
        });
        TransferOutcome {
            result,
            access: Some(access),
        }
    }
}

/// The `maximum` of a reply body: negative is `INVALID_ARGUMENT`; above the
/// configured response body limit is `LIMIT`.
///
/// # Errors
///
/// The failure of a `maximum` outside `0..=max_response_body_bytes`.
pub fn body_maximum(maximum: i64) -> Result<usize, WebFailure> {
    let limit = bn_limits::web_limits().max_response_body_bytes;
    let maximum = usize::try_from(maximum).map_err(|_| {
        WebFailure::InvalidArgument(bn_catalog::text("web", "client.negative_maximum").into())
    })?;
    if maximum > limit {
        return Err(WebFailure::TooLarge {
            what: SizeLimit::BodyMaximum,
            limit,
        });
    }
    Ok(maximum)
}

/// `headers` with `content_type` added unless one of them already names the
/// media type.
#[must_use]
pub fn with_content_type(
    mut headers: Vec<(String, String)>,
    content_type: Option<&str>,
) -> Vec<(String, String)> {
    if let Some(content_type) = content_type
        && !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(CONTENT_TYPE))
    {
        headers.push((CONTENT_TYPE.into(), content_type.into()));
    }
    headers
}

impl Form {
    /// The `multipart/form-data` body of the form and its content type.
    #[must_use]
    pub fn multipart(&self) -> (Vec<u8>, String) {
        let mut body = Vec::new();
        for (name, value) in &self.fields {
            body.extend_from_slice(
                format!(
                    "--{FORM_BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        }
        for (name, path, content_type) in &self.files {
            let filename = std::path::Path::new(path)
                .file_name()
                .and_then(|file| file.to_str())
                .unwrap_or(DEFAULT_FILE_NAME);
            body.extend_from_slice(
                format!(
                    "--{FORM_BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(&form_file(path));
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{FORM_BOUNDARY}--\r\n").as_bytes());
        (
            body,
            format!("multipart/form-data;boundary={FORM_BOUNDARY}"),
        )
    }
}

/// The bytes of a form file part, read as both backends always have: whole,
/// outside the filesystem policy, and empty when the file cannot be read. The
/// bounded, policy-checked read replaces this one function.
#[expect(clippy::verbose_file_reads, reason = "kept as it was; see the note")]
fn form_file(path: &str) -> Vec<u8> {
    use std::io::Read as _;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    bytes
}

/// The methods the 0.3 `Client.Request` API sends.
const REQUEST_METHODS: [&str; 6] = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"];

/// The outcome of a 0.3 `Client.Request`: the response or its failure, and
/// the access record when the exchange was attempted.
#[derive(Debug)]
pub struct RequestOutcome {
    pub result: Result<super::Response, WebFailure>,
    pub access: Option<ClientAccess>,
}

/// `Client.Request` and `Client.RequestWithPolicy` (bnweb 0.3): a method of
/// [`REQUEST_METHODS`], a client URL, a text body; `policy` as in
/// [`Transfer::policy`].
#[must_use]
pub fn request(
    method: &str,
    url: &str,
    body: &str,
    policy: Option<&EgressPolicy>,
) -> RequestOutcome {
    let checked = if REQUEST_METHODS.contains(&method) {
        super::validate_client_url(url)
    } else {
        Err(WebFailure::UnsupportedMethod(method.into()))
    };
    if let Err(failure) = checked {
        return RequestOutcome {
            result: Err(failure),
            access: None,
        };
    }
    let default_policy = EgressPolicy::default();
    let started = std::time::Instant::now();
    let result = crate::http::client_request_with_policy(
        method,
        url,
        body,
        policy.unwrap_or(&default_policy),
    );
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let access = match &result {
        Ok(response) => ClientAccess {
            status: i64::from(response.status),
            bytes: i64::try_from(response.body.len()).unwrap_or(i64::MAX),
            duration_ms,
            headers: response.headers.clone(),
        },
        Err(_) => ClientAccess {
            duration_ms,
            ..ClientAccess::default()
        },
    };
    RequestOutcome {
        result,
        access: Some(access),
    }
}
