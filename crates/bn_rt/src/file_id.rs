// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Cross-platform file identity and symlink detection for TOCTOU mitigation.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FileIdentity {
    #[cfg(unix)]
    dev: u64,
    #[cfg(unix)]
    ino: u64,
    #[cfg(windows)]
    volume: u32,
    #[cfg(windows)]
    index: u64,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct ByHandleFileInformation {
    dw_file_attributes: u32,
    ft_creation_time: [u32; 2],
    ft_last_access_time: [u32; 2],
    ft_last_write_time: [u32; 2],
    dw_volume_serial_number: u32,
    n_file_size_high: u32,
    n_file_size_low: u32,
    n_number_of_links: u32,
    n_file_index_high: u32,
    n_file_index_low: u32,
}

#[cfg(windows)]
extern "system" {
    fn GetFileInformationByHandle(
        handle: *mut std::ffi::c_void,
        info: *mut ByHandleFileInformation,
    ) -> i32;
}

#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

impl FileIdentity {
    pub(crate) fn from_path(path: &Path) -> Option<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            let meta = std::fs::metadata(path).ok()?;
            Some(Self {
                dev: meta.dev(),
                ino: meta.ino(),
            })
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle as _;
            let file = std::fs::File::open(path).ok()?;
            let mut info = ByHandleFileInformation::default();
            // SAFETY: `file` is a valid open file handle, and `info` is a valid
            // memory buffer matching the Win32 `BY_HANDLE_FILE_INFORMATION` structure.
            #[allow(unsafe_code)]
            let success =
                unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut info) };
            if success == 0 {
                return None;
            }
            let index =
                (u64::from(info.n_file_index_high) << 32) | u64::from(info.n_file_index_low);
            Some(Self {
                volume: info.dw_volume_serial_number,
                index,
            })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            None
        }
    }
}

pub(crate) fn is_symlink_or_reparse_point(path: &Path) -> bool {
    #[cfg(unix)]
    {
        std::fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        std::fs::symlink_metadata(path)
            .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        false
    }
}
