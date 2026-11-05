// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The static file opener of the compiled server: every read goes through the
//! runtime file ABI, so the process filesystem policy (`--sandbox`,
//! `--read-root`) decides what a mount can serve, as it does for `FS.Open`.

#![allow(unsafe_code)]

use std::ffi::c_char;
use std::path::Path;

use super::static_files::{StaticFileContent, StaticFileError, StaticFileOpener};
use crate::web_abi::c_string;

/// Mode argument of `bn_rt_file_open` for reading.
const MODE_READ: i32 = 0;
const STATUS_OK: u32 = 0;
const STATUS_POLICY_DENIED: u32 = 3;
const STATUS_EOF: u32 = 4;
/// Bytes asked of the runtime per read.
const CHUNK: i64 = 64 * 1024;

// SAFETY: the foreign signatures match the exports of `bn_rt::file_abi`.
unsafe extern "C" {
    fn bn_rt_file_open(path: *const c_char, mode: i32, out: *mut u64) -> u32;
    fn bn_rt_file_read_bytes(handle: u64, buffer: *mut u8, len: i64, out: *mut i64) -> u32;
    fn bn_rt_file_close(handle: u64) -> u32;
    fn bn_rt_file_release(handle: u64) -> u32;
}

/// Opener that reads through the runtime's policy-checked file ABI.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PolicyFileOpener;

impl PolicyFileOpener {
    fn open(path: &Path) -> Result<u64, StaticFileError> {
        let text = path.to_string_lossy();
        let name = c_string(&text);
        if name.is_null() {
            return Err(StaticFileError::Io("out of memory".into()));
        }
        let mut handle = 0_u64;
        // SAFETY: `name` is a STRING built above and `handle` is a writable slot.
        let status = unsafe { bn_rt_file_open(name, MODE_READ, &raw mut handle) };
        // SAFETY: `name` came from `c_string` (malloc); the runtime copied the path.
        unsafe { libc::free(name.cast::<u8>().sub(crate::web_abi::HEADER_SIZE).cast()) };
        match status {
            STATUS_OK => Ok(handle),
            STATUS_POLICY_DENIED => Err(StaticFileError::Forbidden),
            _ if !path.exists() => Err(StaticFileError::NotFound),
            _ => Err(StaticFileError::Io("file could not be opened".into())),
        }
    }

    fn read_all(handle: u64, max_bytes: usize) -> Result<Vec<u8>, StaticFileError> {
        let mut bytes = Vec::new();
        let mut chunk = vec![0_u8; usize::try_from(CHUNK).unwrap_or(0)];
        loop {
            let mut count = 0_i64;
            // SAFETY: `chunk` holds `CHUNK` writable bytes and `count` is a writable slot.
            let status =
                unsafe { bn_rt_file_read_bytes(handle, chunk.as_mut_ptr(), CHUNK, &raw mut count) };
            let read = usize::try_from(count).unwrap_or(0);
            if status != STATUS_OK && status != STATUS_EOF {
                return Err(StaticFileError::Io("file could not be read".into()));
            }
            if read == 0 {
                return Ok(bytes);
            }
            if bytes.len() + read > max_bytes {
                return Err(StaticFileError::TooLarge);
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
    }
}

impl StaticFileOpener for PolicyFileOpener {
    fn read_file(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> Result<StaticFileContent, StaticFileError> {
        let handle = Self::open(path)?;
        let result = (|| {
            let meta = std::fs::metadata(path).map_err(|e| StaticFileError::Io(e.to_string()))?;
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
            let bytes = Self::read_all(handle, max_bytes)?;
            Ok(StaticFileContent { bytes, mtime_secs })
        })();
        // SAFETY: `handle` was opened above and is closed and released once.
        unsafe {
            bn_rt_file_close(handle);
            bn_rt_file_release(handle);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::PolicyFileOpener;
    use crate::web::static_files::{StaticFileError, StaticFileOpener};

    #[test]
    fn reads_through_the_runtime_and_reports_a_missing_file() {
        let directory = std::env::temp_dir().join(format!("bnweb-policy-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("directory");
        let file = directory.join("page.txt");
        std::fs::write(&file, b"page").expect("write");
        let opener = PolicyFileOpener;
        let content = opener.read_file(&file, 16).expect("read");
        assert_eq!(content.bytes, b"page");
        assert_eq!(opener.read_file(&file, 2), Err(StaticFileError::TooLarge));
        assert_eq!(
            opener.read_file(&directory.join("absent"), 16),
            Err(StaticFileError::NotFound)
        );
        std::fs::remove_dir_all(&directory).expect("cleanup");
    }
}
