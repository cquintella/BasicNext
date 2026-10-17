// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Access-log sink and logger wiring for `BNWeb` under `bni`.

#![allow(clippy::wildcard_imports)]
use std::io::{Read, Write};
use super::*;

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
            .map(|file| Box::new(file) as Box<dyn std::io::Write>)
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
        if !self.policy.allows_path(path, false) {
            return Err(bn_web::StaticFileError::Forbidden);
        }
        let mut file = self
            .policy
            .open(path, bn_rt::secure_fs::OpenMode::Read)
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => bn_web::StaticFileError::NotFound,
                std::io::ErrorKind::PermissionDenied => bn_web::StaticFileError::Forbidden,
                _ => bn_web::StaticFileError::Io(e.to_string()),
            })?;
        let meta = file
            .metadata()
            .map_err(|e| bn_web::StaticFileError::Io(e.to_string()))?;
        if !meta.is_file() {
            return Err(bn_web::StaticFileError::NotFound);
        }
        if meta.len() > max_bytes as u64 {
            return Err(bn_web::StaticFileError::TooLarge);
        }
        let mtime_secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let cap = usize::try_from(meta.len()).unwrap_or(max_bytes);
        let mut bytes = Vec::with_capacity(cap);
        file.read_to_end(&mut bytes)
            .map_err(|e| bn_web::StaticFileError::Io(e.to_string()))?;
        Ok(bn_web::StaticFileContent { bytes, mtime_secs })
    }
}


