// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! One file a `BNWeb.Server` static mount serves, read under the filesystem
//! policy. One producer for both backends: the interpreter's opener calls
//! [`read`], the compiled server reaches it through
//! [`bn_rt_static_file_read`], so a path gets the same answer on each.
//!
//! Only a regular file is served. The open never waits (a FIFO in the
//! directory would hold a server thread until a writer appeared), and the
//! type and size come from the opened descriptor, not from the path.
#![allow(unsafe_code)] // C ABI export for the compiled server.

use std::io::Read as _;
use std::path::Path;

use bn_core_fs::FsEntryPolicy as _;

use super::policy::FsPolicy;

/// Why a static file was not served.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StaticReadError {
    /// Missing, or not a regular file (a directory, a FIFO, a device).
    NotFound,
    /// The policy refuses the path, or the system refuses access.
    Forbidden,
    /// Larger than the server's limit.
    TooLarge,
    /// Any other failure, with the system's message.
    Io(String),
}

/// The bytes of a served file and its modification time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StaticFile {
    pub bytes: Vec<u8>,
    pub mtime_secs: u64,
}

/// Reads `path` for a static mount: a regular file of at most `max_bytes`
/// bytes that `policy` lets this process read.
///
/// # Errors
///
/// [`StaticReadError`]: the reason the file is not served.
pub fn read(
    policy: &FsPolicy,
    path: &Path,
    max_bytes: usize,
) -> Result<StaticFile, StaticReadError> {
    if !policy.allows_path(path, false) {
        return Err(StaticReadError::Forbidden);
    }
    let file = policy
        .open_regular(path)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::InvalidInput => {
                StaticReadError::NotFound
            }
            // Windows refuses to open a directory as a file.
            std::io::ErrorKind::PermissionDenied if path.is_dir() => StaticReadError::NotFound,
            std::io::ErrorKind::PermissionDenied => StaticReadError::Forbidden,
            _ => StaticReadError::Io(error.to_string()),
        })?;
    let meta = file
        .metadata()
        .map_err(|error| StaticReadError::Io(error.to_string()))?;
    let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if meta.len() > limit {
        return Err(StaticReadError::TooLarge);
    }
    let mtime_secs = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs());
    let mut bytes = Vec::with_capacity(usize::try_from(meta.len()).unwrap_or(max_bytes));
    // One byte past the limit shows a file that grew after the size check.
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| StaticReadError::Io(error.to_string()))?;
    if bytes.len() > max_bytes {
        return Err(StaticReadError::TooLarge);
    }
    Ok(StaticFile { bytes, mtime_secs })
}

/// Status of [`bn_rt_static_file_read`].
pub const STATIC_FILE_OK: u32 = 0;
pub const STATIC_FILE_NOT_FOUND: u32 = 1;
pub const STATIC_FILE_FORBIDDEN: u32 = 2;
pub const STATIC_FILE_TOO_LARGE: u32 = 3;
pub const STATIC_FILE_ERROR: u32 = 4;
pub const STATIC_FILE_INVALID: u32 = 5;

/// [`read`] for the compiled server, under the process policy. `path` is
/// `path_len` UTF-8 bytes. On `STATIC_FILE_OK`, `*out_data` is a `malloc`
/// block of `*out_len` bytes the caller frees with `free` (null when the
/// file is empty) and `*out_mtime` the modification time in seconds.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_static_file_read(
    path: *const u8,
    path_len: usize,
    max_bytes: u64,
    out_data: *mut *mut u8,
    out_len: *mut u64,
    out_mtime: *mut u64,
) -> u32 {
    if path.is_null() || out_data.is_null() || out_len.is_null() || out_mtime.is_null() {
        return STATIC_FILE_INVALID;
    }
    // SAFETY: the caller passes `path_len` readable bytes at `path`.
    let bytes = unsafe { std::slice::from_raw_parts(path, path_len) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return STATIC_FILE_NOT_FOUND;
    };
    if !super::policy::allows(super::policy::POLICY_FILESYSTEM) {
        return STATIC_FILE_FORBIDDEN;
    }
    let max_bytes = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    let file = match super::policy::with_fs(|policy| read(policy, Path::new(text), max_bytes)) {
        Ok(file) => file,
        Err(StaticReadError::NotFound) => return STATIC_FILE_NOT_FOUND,
        Err(StaticReadError::Forbidden) => return STATIC_FILE_FORBIDDEN,
        Err(StaticReadError::TooLarge) => return STATIC_FILE_TOO_LARGE,
        Err(StaticReadError::Io(_)) => return STATIC_FILE_ERROR,
    };
    let data = if file.bytes.is_empty() {
        std::ptr::null_mut()
    } else {
        // SAFETY: a fresh allocation of the file's length; null is checked.
        let block = unsafe { libc::malloc(file.bytes.len()) }.cast::<u8>();
        if block.is_null() {
            return STATIC_FILE_ERROR;
        }
        // SAFETY: `block` holds `file.bytes.len()` bytes and does not overlap.
        unsafe { std::ptr::copy_nonoverlapping(file.bytes.as_ptr(), block, file.bytes.len()) };
        block
    };
    // SAFETY: the three out pointers were checked non-null; the caller owns them.
    unsafe { out_data.write(data) };
    // SAFETY: as above.
    unsafe { out_len.write(u64::try_from(file.bytes.len()).unwrap_or(u64::MAX)) };
    // SAFETY: as above.
    unsafe { out_mtime.write(file.mtime_secs) };
    STATIC_FILE_OK
}

#[cfg(test)]
mod tests {
    use super::{StaticReadError, read};
    use crate::policy::FsPolicy;
    use crate::secure_fs::RootedDir;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A read that does not finish in this time is taken as blocked.
    const BLOCKED_AFTER: Duration = Duration::from_secs(5);

    struct Tree {
        base: PathBuf,
        inside: PathBuf,
        outside: PathBuf,
    }

    impl Tree {
        fn new(label: &str) -> Self {
            let base =
                std::env::temp_dir().join(format!("bn-static-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            std::fs::create_dir_all(&base).expect("base");
            let base = base.canonicalize().expect("canonical base");
            let inside = base.join("inside");
            let outside = base.join("outside");
            std::fs::create_dir_all(&inside).expect("inside");
            std::fs::create_dir_all(&outside).expect("outside");
            std::fs::write(inside.join("page.txt"), b"page").expect("page");
            std::fs::write(outside.join("secret.txt"), b"secret").expect("secret");
            std::fs::create_dir_all(inside.join("folder")).expect("folder");
            #[cfg(unix)]
            {
                let fifo = std::ffi::CString::new(inside.join("pipe").to_str().expect("fifo path"))
                    .expect("fifo name");
                // SAFETY: `fifo` is a NUL-terminated path for this call.
                assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0, "mkfifo");
            }
            Self {
                base,
                inside,
                outside,
            }
        }

        fn rooted(&self) -> FsPolicy {
            FsPolicy::rooted(
                vec![RootedDir::new(&self.inside).expect("root")],
                Vec::new(),
            )
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// Runs `read` on another thread; a read still running after
    /// `BLOCKED_AFTER` fails the test instead of hanging it.
    #[expect(
        clippy::unwrap_in_result,
        reason = "a read that blocks is the failure this test helper reports"
    )]
    fn read_bounded(policy: FsPolicy, path: &Path, max: usize) -> Result<Vec<u8>, StaticReadError> {
        let (sender, receiver) = mpsc::channel();
        let path = path.to_path_buf();
        std::thread::spawn(move || {
            let _ = sender.send(read(&policy, &path, max).map(|file| file.bytes));
        });
        receiver
            .recv_timeout(BLOCKED_AFTER)
            .expect("the read finished instead of blocking")
    }

    #[test]
    fn a_static_read_serves_only_regular_files_inside_the_policy() {
        let tree = Tree::new("policy");
        for policy in [FsPolicy::unrestricted(), tree.rooted()] {
            assert_eq!(
                read_bounded(policy.clone(), &tree.inside.join("page.txt"), 16),
                Ok(b"page".to_vec())
            );
            assert_eq!(
                read_bounded(policy.clone(), &tree.inside.join("page.txt"), 2),
                Err(StaticReadError::TooLarge)
            );
            assert_eq!(
                read_bounded(policy.clone(), &tree.inside.join("absent"), 16),
                Err(StaticReadError::NotFound)
            );
            assert_eq!(
                read_bounded(policy.clone(), &tree.inside.join("folder"), 16),
                Err(StaticReadError::NotFound)
            );
            #[cfg(unix)]
            assert_eq!(
                read_bounded(policy, &tree.inside.join("pipe"), 16),
                Err(StaticReadError::NotFound),
                "a FIFO is refused without blocking"
            );
        }
        assert_eq!(
            read_bounded(tree.rooted(), &tree.outside.join("secret.txt"), 16),
            Err(StaticReadError::Forbidden),
            "outside the read root"
        );
    }
}
