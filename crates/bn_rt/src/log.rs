// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNLog` record and formatting re-exported from `bn_core_log`.

pub use bn_core_log::{FileTransport, Level, Record, dispatch_log};

#[cfg(test)]
mod tests {
    use super::{Level, Record};
    use std::collections::BTreeMap;

    /// `Logger.Log` on both backends: the call's fields override the
    /// logger's context, and the timestamp is RFC 3339 UTC.
    #[test]
    fn records_merge_call_fields_over_context() {
        let context = BTreeMap::from([
            ("service".to_owned(), "api".to_owned()),
            ("route".to_owned(), "/".to_owned()),
        ]);
        let provided = BTreeMap::from([("route".to_owned(), "/users".to_owned())]);
        let record = Record::with_timestamp(
            "2026-10-08T12:00:00Z".to_owned(),
            "web",
            Level::Info,
            "hit",
            &context,
            &provided,
        );
        assert_eq!(record.fields["service"], "api");
        assert_eq!(record.fields["route"], "/users");
        assert_eq!(record.label, "web");
        assert!(record.timestamp.ends_with('Z') && record.timestamp.contains('T'));
    }

    #[test]
    fn json_redacts_sensitive_fields_and_escapes_controls() {
        let record = Record {
            timestamp: "now".into(),
            label: "app".into(),
            level: Level::Info,
            message: "hello\nworld".into(),
            fields: BTreeMap::from([
                ("user".into(), "ana".into()),
                ("authorization".into(), "secret".into()),
            ]),
        };
        let json = record.json_line().unwrap();
        assert!(json.contains("hello\\nworld"));
        assert!(json.contains("\"user\":\"ana\""));
        assert!(!json.contains("secret"));
    }
}
