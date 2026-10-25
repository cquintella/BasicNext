// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The two record STRUCTs of `BNLog` (`Log.Event`, `Log.Apache`; bnlog.md,
//! "Records") as field maps: a field left at its type's zero value is unset
//! and left out, so the logger's default can take its place.

use crate::catalog;
use crate::fields::{FIELD_VALUE_MAX_BYTES, FieldMap};

/// The fields of a `Log.Event`, in declaration order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Event {
    pub service: String,
    pub request_id: String,
    pub route: String,
    pub user: String,
    pub detail: String,
    pub code: i64,
}

/// The fields of a `Log.Apache`, in declaration order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Apache {
    pub remote: String,
    pub method: String,
    pub path: String,
    pub protocol: String,
    pub status: i64,
    pub bytes: i64,
    pub referrer: String,
    pub agent: String,
}

/// Inserts `value` under the catalog name `key` when it is set.
fn insert_text(fields: &mut FieldMap, key: &str, value: &str) {
    if !value.is_empty() {
        fields.insert(catalog::text(key).to_owned(), value.to_owned());
    }
}

fn insert_number(fields: &mut FieldMap, key: &str, value: i64) {
    if value != 0 {
        fields.insert(catalog::text(key).to_owned(), value.to_string());
    }
}

impl Event {
    /// The set fields, under their own names (`request_id`).
    #[must_use]
    pub fn fields(&self) -> FieldMap {
        let mut fields = FieldMap::new();
        insert_text(&mut fields, "event.service", &self.service);
        insert_text(&mut fields, "event.request_id", &self.request_id);
        insert_text(&mut fields, "event.route", &self.route);
        insert_text(&mut fields, "event.user", &self.user);
        insert_text(&mut fields, "event.detail", &self.detail);
        insert_number(&mut fields, "event.code", self.code);
        fields
    }
}

impl Apache {
    /// The set fields, under the `http.` names the APACHE format reads.
    #[must_use]
    pub fn fields(&self) -> FieldMap {
        let mut fields = FieldMap::new();
        insert_text(&mut fields, "apache.remote", &self.remote);
        insert_text(&mut fields, "apache.method", &self.method);
        insert_text(&mut fields, "apache.path", &self.path);
        insert_text(&mut fields, "apache.protocol", &self.protocol);
        insert_number(&mut fields, "apache.status", self.status);
        insert_number(&mut fields, "apache.bytes", self.bytes);
        insert_text(&mut fields, "apache.referrer", &self.referrer);
        insert_text(&mut fields, "apache.user_agent", &self.agent);
        fields
    }
}

/// One HTTP client access record (bnweb.md, "Client and security policy").
/// The path has no query, `http.remote` is the target host without userinfo,
/// and a sensitive header is left out. Status `0` stays, so a transport
/// failure is visible.
#[must_use]
pub fn http_client_access(
    method: &str,
    url: &str,
    status: i64,
    bytes: i64,
    duration_ms: i64,
    headers: &[(&str, &str)],
) -> FieldMap {
    let mut fields = FieldMap::new();
    insert_text(&mut fields, "apache.remote", &access_host(url));
    insert_text(&mut fields, "apache.method", method);
    insert_text(&mut fields, "apache.path", &access_path(url));
    insert_text(&mut fields, "apache.protocol", "HTTP/1.1");
    fields.insert(
        catalog::text("apache.status").to_owned(),
        status.to_string(),
    );
    insert_number(&mut fields, "apache.bytes", bytes);
    if duration_ms >= 0 {
        fields.insert(
            catalog::text("apache.duration_ms").to_owned(),
            duration_ms.to_string(),
        );
    }
    let prefix = catalog::text("apache.header_prefix");
    for (name, value) in headers {
        if !header_name_ok(name) || value.contains(['\n', '\r', '\0']) {
            continue;
        }
        let key = format!("{prefix}{}", name.to_ascii_lowercase());
        if crate::is_sensitive(&key) || value.is_empty() {
            continue;
        }
        fields.insert(key, bound_field(value));
    }
    fields
}

/// The host of `url`, without userinfo and without a port.
fn access_host(url: &str) -> String {
    let rest = url
        .split('#')
        .next()
        .unwrap_or(url)
        .split_once("://")
        .map_or("", |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if let Some(host) = authority.strip_prefix('[') {
        return host.split(']').next().unwrap_or("").to_owned();
    }
    authority.split(':').next().unwrap_or("").to_owned()
}

/// The path of `url`, starting with `/`, with the query and fragment removed.
fn access_path(url: &str) -> String {
    let rest = url
        .split('#')
        .next()
        .unwrap_or(url)
        .split_once("://")
        .map_or(url, |(_, rest)| rest);
    let Some((_, path)) = rest.split_once('/') else {
        return "/".to_owned();
    };
    let path = path.split('?').next().unwrap_or(path);
    if path.is_empty() {
        "/".to_owned()
    } else {
        format!("/{path}")
    }
}

fn header_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn bound_field(value: &str) -> String {
    if value.len() <= FIELD_VALUE_MAX_BYTES {
        return value.to_owned();
    }
    let mut end = FIELD_VALUE_MAX_BYTES;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_fields_are_left_out() {
        let event = Event {
            user: "ana".into(),
            code: 3,
            ..Event::default()
        };
        assert_eq!(
            event.fields(),
            FieldMap::from([("code".into(), "3".into()), ("user".into(), "ana".into())])
        );
        let request = Apache {
            method: "GET".into(),
            status: 200,
            agent: "curl".into(),
            ..Apache::default()
        };
        assert_eq!(
            request.fields(),
            FieldMap::from([
                ("http.method".into(), "GET".into()),
                ("http.status".into(), "200".into()),
                ("http.user_agent".into(), "curl".into()),
            ])
        );
        assert!(Event::default().fields().is_empty());
    }

    #[test]
    fn client_access_drops_the_query_userinfo_and_sensitive_headers() {
        let fields = http_client_access(
            "GET",
            "http://user:secret@127.0.0.1:9/item?token=1",
            0,
            0,
            12,
            &[
                ("Content-Type", "text/plain"),
                ("Set-Cookie", "a=b"),
                ("X-Empty", ""),
            ],
        );
        assert_eq!(fields.get("http.method").map(String::as_str), Some("GET"));
        assert_eq!(fields.get("http.path").map(String::as_str), Some("/item"));
        assert_eq!(
            fields.get("http.remote").map(String::as_str),
            Some("127.0.0.1")
        );
        assert_eq!(
            fields.get("http.protocol").map(String::as_str),
            Some("HTTP/1.1")
        );
        assert_eq!(fields.get("http.status").map(String::as_str), Some("0"));
        assert_eq!(
            fields.get("http.duration_ms").map(String::as_str),
            Some("12")
        );
        assert_eq!(
            fields.get("http.header.content-type").map(String::as_str),
            Some("text/plain")
        );
        assert!(!fields.contains_key("http.header.set-cookie"));
        assert!(!fields.contains_key("http.header.x-empty"));
        assert!(
            fields
                .values()
                .all(|value| !value.contains("token") && !value.contains("secret"))
        );
        assert!(!fields.contains_key("http.bytes_sent"));
    }
}
