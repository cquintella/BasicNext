// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The access-log record `Server.Dispatch` writes (split from `lib.rs` by
//! domain).

#![allow(clippy::wildcard_imports)]
use super::*;

impl WebProvider {
    /// Access-log record for a served request, written through the `BNLog`
    /// provider as an access record (bnlog.md, "`BNWeb` integration"), with
    /// the request ID beyond the `Log.Apache` fields.
    pub(super) fn log_web_dispatch(
        &mut self,
        core: &mut dyn CoreContext,
        server_handle: Handle,
        method: &str,
        path: &str,
        status: i128,
        request_id: Option<&str>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(&logger_id) = self.loggers.get(&server_handle) else {
            return Ok(());
        };
        let mut entries = vec![
            ("http.method", method.to_string()),
            ("http.path", path.to_string()),
            ("http.status", status.to_string()),
        ];
        if let Some(request_id) = request_id {
            entries.push(("request_id", request_id.to_string()));
        }
        let mut arguments = vec![Value::LogLogger(logger_id)];
        for (key, value) in entries {
            arguments.push(Value::String(key.into()));
            arguments.push(Value::String(shared_string(value)));
        }
        core.library_call(
            bn_lib_log::NAME,
            bn_lib_log::WEB_ACCESS_MEMBER,
            arguments,
            span,
        )
        .map(|_| ())
    }
}
