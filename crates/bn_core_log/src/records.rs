// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The two record STRUCTs of `BNLog` (`Log.Event`, `Log.Apache`; bnlog.md,
//! "Records") as field maps: a field left at its type's zero value is unset
//! and left out, so the logger's default can take its place.

use crate::catalog;
use crate::fields::FieldMap;

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
}
