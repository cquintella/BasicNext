// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` contract names and texts, read from `share/bn/catalog/bnlog.txt`
//! (one `key = value` per line). Code refers to a key; the value lives only
//! in the data file.

/// The value of `key`. A missing key is a defect of this crate, caught by
/// `every_used_key_is_in_the_data_file`.
pub(crate) fn text(key: &str) -> &'static str {
    bn_catalog::text("bnlog", key)
}

/// The text of `key` with each `{name}` replaced by its value.
pub(crate) fn fill(key: &str, values: &[(&str, &dyn std::fmt::Display)]) -> String {
    bn_catalog::fill("bnlog", key, values)
}

/// The whitespace-separated words of `key`.
pub(crate) fn words(key: &str) -> impl Iterator<Item = &'static str> {
    bn_catalog::words("bnlog", key)
}

/// Every key the crate reads; `catalog_keys_match_the_data_file` checks the
/// data file holds each (`tests/catalog_keys.rs` checks the whole file).
#[cfg(test)]
pub(crate) const KEYS: &[&str] = &[
    "format.json",
    "format.syslog",
    "syslog.nil",
    "format.apache",
    "apache.remote",
    "apache.method",
    "apache.path",
    "apache.protocol",
    "apache.status",
    "apache.bytes",
    "apache.referrer",
    "apache.user_agent",
    "apache.absent",
    "apache.zone",
    "apache.month",
    "sensitive.exact",
    "sensitive.marker",
    "error.invalid_format.message",
    "error.invalid_format.cause",
    "error.duplicate_key.message",
    "error.duplicate_key.cause",
    "error.invalid_transport.cause",
];

#[cfg(test)]
mod tests {
    use super::KEYS;

    #[test]
    fn catalog_keys_match_the_data_file() {
        let mut expected = KEYS.to_vec();
        expected.sort_unstable();
        let source = bn_catalog::FILES
            .iter()
            .find(|(domain, _)| *domain == "bnlog")
            .map(|(_, contents)| *contents)
            .unwrap_or_default();
        let mut found: Vec<&str> = bn_catalog::parse(source)
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        found.sort_unstable();
        for key in &expected {
            assert!(found.contains(key), "bnlog.txt lacks {key}");
        }
    }
}
