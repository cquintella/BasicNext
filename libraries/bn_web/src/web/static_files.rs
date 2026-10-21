// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Static file serving for `BNWeb.Server` (E.S0.f):
//! - `Server.Static(prefix, directory)`
//! - Path sanitization: `..`, absolute, `NUL`, and encoded separators rejected
//! - Symbolic links leaving the directory return 404
//! - Dot segments (`.git`, `.env`) are never served (404)
//! - No directory listing; directories serve `index.html` if present
//! - `Content-Type` from MIME data table
//! - Bounded size (HTTP 413)
//! - `ETag` and `Last-Modified` with conditional `304 Not Modified`
//! - `GET` and `HEAD` only (HTTP 405)

use std::path::{Path, PathBuf};

/// In-memory representation of static file content and timestamp.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticFileContent {
    pub bytes: Vec<u8>,
    pub mtime_secs: u64,
}

/// Errors produced by static file openers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaticFileError {
    NotFound,
    Forbidden,
    TooLarge,
    Io(String),
}

/// Trait supplied by adapters to read static files with filesystem policy enforcement.
pub trait StaticFileOpener: Send + Sync {
    fn read_file(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> Result<StaticFileContent, StaticFileError>;
}

/// Standard opener using `std::fs`.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SystemFileOpener;

impl StaticFileOpener for SystemFileOpener {
    fn read_file(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> Result<StaticFileContent, StaticFileError> {
        let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => StaticFileError::NotFound,
            std::io::ErrorKind::PermissionDenied => StaticFileError::Forbidden,
            _ => StaticFileError::Io(e.to_string()),
        })?;
        if !meta.is_file() {
            return Err(StaticFileError::NotFound);
        }
        if meta.len() > max_bytes as u64 {
            return Err(StaticFileError::TooLarge);
        }
        let mtime_secs = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        let bytes = std::fs::read(path).map_err(|e| StaticFileError::Io(e.to_string()))?;
        Ok(StaticFileContent { bytes, mtime_secs })
    }
}

/// A registered static mount point.
#[derive(Clone, Debug)]
pub struct StaticMount {
    pub prefix: String,
    pub directory: PathBuf,
    pub canonical_root: PathBuf,
}

impl StaticMount {
    /// Creates and validates a static mount point.
    pub fn new(prefix: &str, directory: &str) -> Result<Self, &'static str> {
        if prefix.is_empty() || !prefix.starts_with('/') {
            return Err("prefix must start with /");
        }
        if directory.is_empty() {
            return Err("directory cannot be empty");
        }
        let dir_path = PathBuf::from(directory);
        let canonical_root = std::fs::canonicalize(&dir_path)
            .map_err(|_| "static directory not found or invalid")?;
        if !canonical_root.is_dir() {
            return Err("static path is not a directory");
        }
        // Normalize prefix: "/" remains "/", others lose trailing slash.
        let prefix = if prefix == "/" {
            "/".to_string()
        } else {
            prefix.trim_end_matches('/').to_string()
        };
        Ok(Self {
            prefix,
            directory: dir_path,
            canonical_root,
        })
    }

    /// Checks if a request path matches this static mount.
    /// Returns the relative subpath if matched.
    #[must_use]
    pub(crate) fn match_path<'a>(&self, path: &'a str) -> Option<&'a str> {
        if self.prefix == "/" {
            let sub = path.strip_prefix('/').unwrap_or(path);
            return Some(sub);
        }
        if path == self.prefix {
            return Some("");
        }
        let with_slash = format!("{}/", self.prefix);
        if let Some(sub) = path.strip_prefix(&with_slash) {
            return Some(sub);
        }
        None
    }
}

/// Result of static path sanitization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SanitizationError {
    BadRequest(&'static str),
    NotFound,
}

/// Sanitizes a request subpath against path traversal, encoded separators, and dotfiles.
pub(crate) fn sanitize_subpath(
    raw_target: &str,
    subpath: &str,
) -> Result<PathBuf, SanitizationError> {
    // 1. Encoded separators or NUL check on the raw target
    if raw_target.contains("%2f")
        || raw_target.contains("%2F")
        || raw_target.contains("%5c")
        || raw_target.contains("%5C")
        || raw_target.contains('\0')
        || raw_target.contains("%00")
    {
        return Err(SanitizationError::BadRequest(
            "encoded separator or NUL is rejected",
        ));
    }

    let trimmed = subpath.trim_start_matches('/');
    if trimmed.is_empty() {
        return Ok(PathBuf::from("index.html"));
    }

    let mut clean = PathBuf::new();
    for segment in trimmed.split(['/', '\\']) {
        if segment.is_empty() {
            continue;
        }
        if segment == ".." {
            return Err(SanitizationError::BadRequest("path traversal is rejected"));
        }
        if segment == "." {
            continue;
        }
        // Dotfiles or hidden segments (.git, .env, etc.) are never served (404)
        if segment.starts_with('.') {
            return Err(SanitizationError::NotFound);
        }
        // Drive letters or colons on Windows
        if segment.contains(':') {
            return Err(SanitizationError::BadRequest("invalid path segment"));
        }
        clean.push(segment);
    }

    if clean.as_os_str().is_empty() {
        Ok(PathBuf::from("index.html"))
    } else {
        Ok(clean)
    }
}

/// Resolves MIME Content-Type from file extension using static table.
#[must_use]
pub(crate) fn mime_type_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("txt" | "text" | "log") => "text/plain; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("webp") => "image/webp",
        Some("wasm") => "application/wasm",
        Some("pdf") => "application/pdf",
        Some("xml") => "application/xml",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        Some("tar") => "application/x-tar",
        Some("mp3") => "audio/mpeg",
        Some("mp4") => "video/mp4",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        _ => "application/octet-stream",
    }
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Formats unix timestamp as RFC 7231 IMF-fixdate (e.g. `Sun, 06 Nov 1994 08:49:37 GMT`).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::unreadable_literal
)]
pub(crate) fn format_http_date(secs_since_epoch: u64) -> String {
    let mut days = (secs_since_epoch / 86400) as i64;
    let day_secs = (secs_since_epoch % 86400) as u32;
    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    // Unix epoch 1970-01-01 was Thursday (index 4)
    let wday = ((days + 4) % 7 + 7) % 7;

    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    if m <= 2 {
        y += 1;
    }

    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DAYS[wday as usize],
        d,
        MONTHS[(m - 1) as usize],
        y,
        hour,
        minute,
        second
    )
}

/// Parses an RFC 7231 HTTP date string into seconds since epoch.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::unreadable_literal
)]
pub(crate) fn parse_http_date(s: &str) -> Option<u64> {
    let s = s.trim();
    // Sun, 06 Nov 1994 08:49:37 GMT
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() != 6 || parts[5] != "GMT" {
        return None;
    }
    let day: u32 = parts[1].parse().ok()?;
    let month_idx = MONTHS.iter().position(|&m| m == parts[2])? as u32 + 1;
    let year: i64 = parts[3].parse().ok()?;
    let time_parts: Vec<&str> = parts[4].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }
    let hour: u32 = time_parts[0].parse().ok()?;
    let minute: u32 = time_parts[1].parse().ok()?;
    let second: u32 = time_parts[2].parse().ok()?;

    let mut y = year;
    let m = month_idx;
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe as i64 - 719468;
    if days < 0 {
        return None;
    }
    let total_secs =
        (days as u64) * 86400 + (hour as u64) * 3600 + (minute as u64) * 60 + (second as u64);
    Some(total_secs)
}

/// Outcome of serving a static file.
#[derive(Clone, Debug)]
pub enum StaticServeOutcome {
    Ok {
        status: u16,
        content_type: &'static str,
        content_length: usize,
        etag: String,
        last_modified: String,
        body: Vec<u8>,
    },
    NotModified {
        etag: String,
        last_modified: String,
    },
    MethodNotAllowed,
    BadRequest(&'static str),
    PayloadTooLarge,
    NotFound,
    Forbidden,
    InternalError(String),
}

/// Evaluates static file serving for a request.
#[allow(clippy::too_many_arguments)]
pub fn serve_static_file(
    mount: &StaticMount,
    raw_target: &str,
    subpath: &str,
    method: &str,
    if_none_match: Option<&str>,
    if_modified_since: Option<&str>,
    max_body_bytes: usize,
    opener: &dyn StaticFileOpener,
) -> StaticServeOutcome {
    if method != "GET" && method != "HEAD" {
        return StaticServeOutcome::MethodNotAllowed;
    }

    let sanitized = match sanitize_subpath(raw_target, subpath) {
        Ok(path) => path,
        Err(SanitizationError::BadRequest(msg)) => return StaticServeOutcome::BadRequest(msg),
        Err(SanitizationError::NotFound) => return StaticServeOutcome::NotFound,
    };

    let mut candidate = mount.directory.join(&sanitized);

    // If candidate path is a directory, serve index.html (no listing)
    if candidate.is_dir() {
        candidate = candidate.join("index.html");
    }

    // Symbolic link check: canonicalized target must reside inside mount.canonical_root
    let canonical = match std::fs::canonicalize(&candidate) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return StaticServeOutcome::NotFound;
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            return StaticServeOutcome::Forbidden;
        }
        Err(e) => return StaticServeOutcome::InternalError(e.to_string()),
    };

    if !canonical.starts_with(&mount.canonical_root) {
        // Symbolic link escaping directory root returns 404 per specification
        return StaticServeOutcome::NotFound;
    }

    let content = match opener.read_file(&canonical, max_body_bytes) {
        Ok(c) => c,
        Err(StaticFileError::NotFound) => return StaticServeOutcome::NotFound,
        Err(StaticFileError::Forbidden) => return StaticServeOutcome::Forbidden,
        Err(StaticFileError::TooLarge) => return StaticServeOutcome::PayloadTooLarge,
        Err(StaticFileError::Io(err)) => return StaticServeOutcome::InternalError(err),
    };

    let etag = format!("\"{:x}-{:x}\"", content.bytes.len(), content.mtime_secs);
    let last_modified = format_http_date(content.mtime_secs);

    // Conditional GET: If-None-Match
    if let Some(inm) = if_none_match {
        let matches = inm
            .split(',')
            .any(|item| item.trim() == "*" || item.trim() == etag);
        if matches {
            return StaticServeOutcome::NotModified {
                etag,
                last_modified,
            };
        }
    } else if let Some(ims) = if_modified_since
        && let Some(header_time) = parse_http_date(ims)
        && header_time >= content.mtime_secs
    {
        return StaticServeOutcome::NotModified {
            etag,
            last_modified,
        };
    }

    let content_type = mime_type_for_path(&canonical);
    let content_length = content.bytes.len();
    let body = if method == "HEAD" {
        Vec::new()
    } else {
        content.bytes
    };

    StaticServeOutcome::Ok {
        status: 200,
        content_type,
        content_length,
        etag,
        last_modified,
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_date_round_trips() {
        let now = 1729166977; // Sample timestamp
        let formatted = format_http_date(now);
        let parsed = parse_http_date(&formatted).expect("parsed date");
        assert_eq!(now, parsed);
    }

    #[test]
    fn mime_type_detection() {
        assert_eq!(
            mime_type_for_path(Path::new("file.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            mime_type_for_path(Path::new("style.css")),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            mime_type_for_path(Path::new("app.js")),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_type_for_path(Path::new("data.json")),
            "application/json"
        );
        assert_eq!(mime_type_for_path(Path::new("image.png")), "image/png");
        assert_eq!(mime_type_for_path(Path::new("image.svg")), "image/svg+xml");
        assert_eq!(
            mime_type_for_path(Path::new("unknown.xyz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn path_sanitization_rejections() {
        // Path traversal
        assert!(matches!(
            sanitize_subpath("/static/../secret", "../secret"),
            Err(SanitizationError::BadRequest(_))
        ));
        // Encoded separators
        assert!(matches!(
            sanitize_subpath("/static/%2e%2e%2fsecret", "..%2fsecret"),
            Err(SanitizationError::BadRequest(_))
        ));
        assert!(matches!(
            sanitize_subpath("/static/%5cetc%5cpasswd", "%5cetc%5cpasswd"),
            Err(SanitizationError::BadRequest(_))
        ));
        // NUL
        assert!(matches!(
            sanitize_subpath("/static/foo\0bar", "foo\0bar"),
            Err(SanitizationError::BadRequest(_))
        ));
        // Dotfiles return 404
        assert_eq!(
            sanitize_subpath("/static/.git/config", ".git/config"),
            Err(SanitizationError::NotFound)
        );
        assert_eq!(
            sanitize_subpath("/static/.env", ".env"),
            Err(SanitizationError::NotFound)
        );
    }

    #[test]
    fn static_mount_matching() {
        let temp = std::env::temp_dir();
        let mount = StaticMount::new("/assets", temp.to_str().unwrap()).unwrap();
        assert_eq!(mount.match_path("/assets/app.js"), Some("app.js"));
        assert_eq!(mount.match_path("/assets"), Some(""));
        assert_eq!(mount.match_path("/assets/"), Some(""));
        assert_eq!(mount.match_path("/assets_other"), None);
    }
}
