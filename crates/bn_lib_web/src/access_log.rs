// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The access-log record `Server.Dispatch` writes (split from `lib.rs` by
//! domain).

#![allow(clippy::wildcard_imports)]
use super::*;

impl WebProvider {
    /// Access-log record for a served request, written through `BNLog`'s
    /// public contract (`Fields` + `Logger.Log`) exactly as a BN program would.
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
        let log = "BNLog";
        let Some(fields) = core.library_allocate(log, "Fields", span) else {
            return Ok(());
        };
        let fields = fields?;
        let mut entries = vec![
            ("http.method", method.to_string()),
            ("http.path", path.to_string()),
            ("http.status", status.to_string()),
        ];
        if let Some(request_id) = request_id {
            entries.push(("request_id", request_id.to_string()));
        }
        for (key, value) in entries {
            core.library_call(
                log,
                "Fields.SetString",
                vec![
                    fields.clone(),
                    Value::String(key.into()),
                    Value::String(shared_string(value)),
                ],
                span,
            )?;
        }
        let result = core.library_call(
            log,
            "Logger.Log",
            vec![
                Value::LogLogger(logger_id),
                Value::Integer(3, IntegerType::Int32),
                Value::String("web dispatch".into()),
                fields.clone(),
            ],
            span,
        );
        let _ = core.library_release(&fields, span);
        result.map(|_| ())
    }
}
