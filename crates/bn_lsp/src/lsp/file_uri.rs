//! `file:` URIs ↔ native paths (RFC 8089 over RFC 3986). Clients send
//! percent-encoded URIs (`file:///home/a%20b/x.bn`, and on Windows
//! `file:///c%3A/Users/x.bn`); stripping `file://` alone breaks both.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

/// Native path for a `file:` URI with an empty or `localhost` authority.
/// Returns `None` for other schemes, remote hosts, or undecodable text.
pub(crate) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    uri_to_native(uri, cfg!(windows)).map(PathBuf::from)
}

/// `file:` URI for an absolute native path.
pub(crate) fn path_to_file_uri(path: &Path) -> String {
    native_to_uri(&path.to_string_lossy(), cfg!(windows))
}

fn uri_to_native(uri: &str, windows: bool) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let path = if rest.starts_with('/') {
        rest
    } else {
        rest.strip_prefix("localhost")
            .filter(|path| path.starts_with('/'))?
    };
    let path = percent_decode(path)?;
    // `/C:/dir` names drive C: on Windows; the leading slash is URI syntax.
    let bytes = path.as_bytes();
    if windows && bytes.len() >= 3 && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        Some(path[1..].to_owned())
    } else {
        Some(path)
    }
}

fn native_to_uri(path: &str, windows: bool) -> String {
    let path = if windows {
        // `canonicalize` returns verbatim paths (`\\?\C:\x`, `\\?\UNC\h\s`);
        // the prefix is Win32 API syntax, not part of the file's name.
        let path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(path).to_owned()
        };
        path.replace('\\', "/")
    } else {
        path.to_owned()
    };
    let mut uri = String::from("file://");
    if !path.starts_with('/') {
        uri.push('/');
    }
    for byte in path.bytes() {
        // RFC 3986 `pchar` plus `/`: unreserved, sub-delims, `:` and `@`.
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::{native_to_uri, uri_to_native};

    #[test]
    fn unix_uris_decode_percent_escapes() {
        assert_eq!(
            uri_to_native("file:///home/a%20b/caf%C3%A9.bn", false).as_deref(),
            Some("/home/a b/café.bn")
        );
        assert_eq!(
            uri_to_native("file://localhost/tmp/x.bn", false).as_deref(),
            Some("/tmp/x.bn")
        );
    }

    #[test]
    fn windows_verbatim_paths_become_plain_file_uris() {
        assert_eq!(
            native_to_uri(r"\\?\C:\Users\a b\main.bn", true),
            "file:///C:/Users/a%20b/main.bn"
        );
        assert_eq!(
            native_to_uri(r"\\?\UNC\host\share\x.bn", true),
            "file:////host/share/x.bn"
        );
        assert_eq!(native_to_uri(r"C:\x.bn", true), "file:///C:/x.bn");
    }

    #[test]
    fn windows_uris_drop_the_slash_before_the_drive() {
        // VS Code lower-cases the drive and encodes the colon.
        assert_eq!(
            uri_to_native("file:///c%3A/Users/a%20b/main.bn", true).as_deref(),
            Some("c:/Users/a b/main.bn")
        );
        assert_eq!(
            uri_to_native("file:///C:/x.bn", true).as_deref(),
            Some("C:/x.bn")
        );
        // Off Windows the same text is an ordinary absolute path.
        assert_eq!(
            uri_to_native("file:///C:/x.bn", false).as_deref(),
            Some("/C:/x.bn")
        );
    }

    #[test]
    fn non_file_or_remote_or_malformed_uris_are_rejected() {
        assert_eq!(uri_to_native("untitled:Untitled-1", false), None);
        assert_eq!(uri_to_native("file://server/share/x.bn", false), None);
        assert_eq!(uri_to_native("file:///bad%2", false), None);
        assert_eq!(uri_to_native("file:///bad%zz", false), None);
        assert_eq!(uri_to_native("file:///bad%FF", false), None);
    }

    #[test]
    fn paths_encode_to_uris_that_decode_back() {
        assert_eq!(
            native_to_uri("/home/a b/x#1.bn", false),
            "file:///home/a%20b/x%231.bn"
        );
        assert_eq!(
            native_to_uri(r"C:\Users\a b\x.bn", true),
            "file:///C:/Users/a%20b/x.bn"
        );
        for (path, windows) in [("/tmp/a b/café %.bn", false), ("C:/Users/a b/x.bn", true)] {
            assert_eq!(
                uri_to_native(&native_to_uri(path, windows), windows).as_deref(),
                Some(path)
            );
        }
    }
}
