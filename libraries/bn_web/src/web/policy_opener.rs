// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The static file opener of the compiled server. It calls the runtime's
//! static reader, the one the interpreter uses too, so the process filesystem
//! policy (`--sandbox`, `--read-root`) decides what a mount serves and a path
//! gets the same answer on both backends.
#![allow(unsafe_code)]

use std::path::Path;

use super::static_files::{StaticFileContent, StaticFileError, StaticFileOpener};

// Statuses of `bn_rt_static_file_read` (`bn_rt::static_file`).
const STATUS_OK: u32 = 0;
const STATUS_NOT_FOUND: u32 = 1;
const STATUS_FORBIDDEN: u32 = 2;
const STATUS_TOO_LARGE: u32 = 3;

// SAFETY: the foreign signature matches the export of `bn_rt::static_file`.
unsafe extern "C" {
    fn bn_rt_static_file_read(
        path: *const u8,
        path_len: usize,
        max_bytes: u64,
        out_data: *mut *mut u8,
        out_len: *mut u64,
        out_mtime: *mut u64,
    ) -> u32;
}

/// Opener that reads through the runtime's policy-checked static reader.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PolicyFileOpener;

impl StaticFileOpener for PolicyFileOpener {
    fn read_file(
        &self,
        path: &Path,
        max_bytes: usize,
    ) -> Result<StaticFileContent, StaticFileError> {
        // A path that is not UTF-8 names no file a mount can serve.
        let text = path.to_str().ok_or(StaticFileError::NotFound)?;
        let mut data = std::ptr::null_mut();
        let mut len = 0_u64;
        let mut mtime_secs = 0_u64;
        // SAFETY: `text` is live for the call and the out slots are writable.
        let status = unsafe {
            bn_rt_static_file_read(
                text.as_ptr(),
                text.len(),
                u64::try_from(max_bytes).unwrap_or(u64::MAX),
                &raw mut data,
                &raw mut len,
                &raw mut mtime_secs,
            )
        };
        match status {
            STATUS_OK => {}
            STATUS_NOT_FOUND => return Err(StaticFileError::NotFound),
            STATUS_FORBIDDEN => return Err(StaticFileError::Forbidden),
            STATUS_TOO_LARGE => return Err(StaticFileError::TooLarge),
            _ => return Err(StaticFileError::Io("file could not be read".into())),
        }
        let bytes = if data.is_null() {
            Vec::new()
        } else {
            let count = usize::try_from(len).unwrap_or(0);
            // SAFETY: on OK the runtime returned a `malloc` block of `len` bytes.
            let bytes = unsafe { std::slice::from_raw_parts(data, count) }.to_vec();
            // SAFETY: the block is ours to free, once.
            unsafe { libc::free(data.cast()) };
            bytes
        };
        Ok(StaticFileContent { bytes, mtime_secs })
    }
}

#[cfg(test)]
mod tests {
    use super::PolicyFileOpener;
    use crate::web::static_files::{StaticFileError, StaticFileOpener};

    /// The compiled server's opener reaches the runtime reader: the statuses
    /// map to the same outcomes the interpreter's opener returns.
    #[test]
    fn reads_through_the_runtime_and_maps_its_statuses() {
        let directory = std::env::temp_dir().join(format!("bnweb-policy-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("directory");
        let file = directory.join("page.txt");
        std::fs::write(&file, b"page").expect("write");
        std::fs::write(directory.join("empty.txt"), b"").expect("write empty");
        let opener = PolicyFileOpener;
        assert_eq!(opener.read_file(&file, 16).expect("read").bytes, b"page");
        assert!(
            opener
                .read_file(&directory.join("empty.txt"), 16)
                .expect("read empty")
                .bytes
                .is_empty()
        );
        assert_eq!(opener.read_file(&file, 2), Err(StaticFileError::TooLarge));
        assert_eq!(
            opener.read_file(&directory.join("absent"), 16),
            Err(StaticFileError::NotFound)
        );
        assert_eq!(
            opener.read_file(&directory, 16),
            Err(StaticFileError::NotFound)
        );
        std::fs::remove_dir_all(&directory).expect("cleanup");
    }
}
