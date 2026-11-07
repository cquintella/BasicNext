// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Access-log sink and logger wiring for `BNWeb` under `bni`.

#![allow(clippy::wildcard_imports)]
use super::*;
use std::io::Write;

pub(super) fn write_client_access(
    logger: &bn_core_log::Logger,
    sink: &InterpLogSink,
    method: &str,
    url: &str,
    status: i64,
    bytes: i64,
    duration_ms: i64,
    headers: &[(&str, &str)],
) {
    let fields = bn_core_log::http_client_access(method, url, status, bytes, duration_ms, headers);
    let _ = logger.write_access(sink, &fields);
}

pub(super) fn with_log_provider<R>(
    core: &mut dyn CoreContext,
    edit: impl FnOnce(&mut bn_lib_log::LogProvider) -> R,
) -> Result<R, String> {
    let mut provider = core
        .library_take(bn_lib_log::NAME)
        .ok_or_else(|| "BNLog provider unavailable".to_string())?;
    let result = provider
        .as_any_mut()
        .and_then(|any| any.downcast_mut::<bn_lib_log::LogProvider>())
        .map(edit);
    core.library_insert(bn_lib_log::NAME, provider);
    result.ok_or_else(|| "BNLog provider unavailable".to_string())
}

#[derive(Clone)]
pub(super) struct InterpLogSink {
    pub(super) host: HostEnv,
}

impl bn_core_log::LogSink for InterpLogSink {
    fn timestamp(&self) -> String {
        bn_rt::format_rfc3339(bn_rt::timestamp_ms())
    }
    fn console(&self, line: &str) -> std::io::Result<()> {
        let mut out = std::io::stdout().lock();
        writeln!(out, "{line}")
    }
    fn flush_console(&self) -> std::io::Result<()> {
        std::io::stdout().flush()
    }
    fn open_append(&self, path: &std::path::Path) -> std::io::Result<Box<dyn std::io::Write>> {
        self.host
            .filesystem()
            .open(path, bn_rt::secure_fs::OpenMode::Append)
            .map(|file| -> Box<dyn std::io::Write> { Box::new(file) })
    }
}

#[derive(Clone)]
pub(super) struct InterpStaticFileOpener {
    pub(super) policy: bn_rt::FsPolicy,
}

impl bn_web::StaticFileOpener for InterpStaticFileOpener {
    fn read_file(
        &self,
        path: &std::path::Path,
        max_bytes: usize,
    ) -> Result<bn_web::StaticFileContent, bn_web::StaticFileError> {
        // The one static reader of both backends (`bn_rt::static_file`).
        bn_rt::static_file::read(&self.policy, path, max_bytes)
            .map(|file| bn_web::StaticFileContent {
                bytes: file.bytes,
                mtime_secs: file.mtime_secs,
            })
            .map_err(|error| match error {
                bn_rt::static_file::StaticReadError::NotFound => bn_web::StaticFileError::NotFound,
                bn_rt::static_file::StaticReadError::Forbidden => {
                    bn_web::StaticFileError::Forbidden
                }
                bn_rt::static_file::StaticReadError::TooLarge => bn_web::StaticFileError::TooLarge,
                bn_rt::static_file::StaticReadError::Io(message) => {
                    bn_web::StaticFileError::Io(message)
                }
            })
    }
}
