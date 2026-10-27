// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI handle table for `BNCompress.Bytes` and `BNCompress.ZipArchive` in compiled binaries.
//! The interpreter keeps its own table in `bn_lib_compress`; both hand out opaque `u64`
//! handles and neither observes the other.

use std::collections::HashMap;
use std::ffi::c_char;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use bn_core_compress::{
    decode_hex, deflate, encode_hex, gunzip, gzip, inflate, CompressError, ZipArchiveHandle,
};
use crate::{c_str, c_string};

pub(crate) const BN_COMPRESS_OK: i32 = 0;
pub(crate) const BN_COMPRESS_INVALID_ARGUMENT: i32 = 1;
pub(crate) const BN_COMPRESS_INVALID_HANDLE: i32 = 2;

#[derive(Default)]
struct Buffers {
    values: Mutex<HashMap<u64, Vec<u8>>>,
    holders: Mutex<HashMap<u64, usize>>,
}

#[derive(Default)]
struct Archives {
    values: Mutex<HashMap<u64, ZipArchiveHandle>>,
    holders: Mutex<HashMap<u64, usize>>,
}

fn next_handle() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn with_buffers<T>(operation: impl FnOnce(&mut HashMap<u64, Vec<u8>>) -> T) -> T {
    let buffers = bn_core_copy::table::<Buffers>();
    operation(
        &mut buffers
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

fn with_archives<T>(operation: impl FnOnce(&mut HashMap<u64, ZipArchiveHandle>) -> T) -> T {
    let archives = bn_core_copy::table::<Archives>();
    operation(
        &mut archives
            .values
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    )
}

fn store_buffer(bytes: Vec<u8>) -> u64 {
    let handle = next_handle();
    with_buffers(|buffers| buffers.insert(handle, bytes));
    let buffers = bn_core_copy::table::<Buffers>();
    buffers
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

fn store_archive(archive: ZipArchiveHandle) -> u64 {
    let handle = next_handle();
    with_archives(|archives| archives.insert(handle, archive));
    let archives = bn_core_copy::table::<Archives>();
    archives
        .holders
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(handle, 1);
    handle
}

fn get_buffer(handle: u64) -> Option<Vec<u8>> {
    with_buffers(|buffers| buffers.get(&handle).cloned())
}

fn failed(err: &CompressError, operation: &str) -> i32 {
    crate::set_error_report(
        err.code,
        operation,
        &err.message,
        &err.message,
    );
    err.code
}

// ---------------------------------------------------------------------------
// Bytes C ABI
// ---------------------------------------------------------------------------

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_from_text(text: *const c_char) -> u64 {
    store_buffer(
        c_str(text)
            .map(|text| text.as_bytes().to_vec())
            .unwrap_or_default(),
    )
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_from_hex(text: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let text = c_str(text).unwrap_or_default();
    let Some(bytes) = decode_hex(text) else {
        return failed(
            &CompressError::invalid_argument("Malformed hexadecimal string"),
            "FromHex",
        );
    };
    let handle = store_buffer(bytes);
    // SAFETY: caller guarantees non-null writable slot.
    unsafe { *out = handle };
    BN_COMPRESS_OK
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_length(handle: u64) -> i64 {
    with_buffers(|buffers| {
        buffers
            .get(&handle)
            .and_then(|bytes| i64::try_from(bytes.len()).ok())
            .unwrap_or(-1)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_to_hex(handle: u64) -> *mut c_char {
    let hex = with_buffers(|buffers| buffers.get(&handle).map(|b| encode_hex(b))).unwrap_or_default();
    c_string(&hex)
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_to_text(handle: u64, out: *mut *mut c_char) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(bytes) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "ToText");
    };
    match String::from_utf8(bytes) {
        Ok(s) => {
            // SAFETY: pointer was checked non-null.
            unsafe { *out = c_string(&s) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(
            &CompressError::corrupt_data(format!("Invalid UTF-8: {e}")),
            "ToText",
        ),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_retain(handle: u64) -> i32 {
    let buffers = bn_core_copy::table::<Buffers>();
    let mut holders = buffers.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        BN_COMPRESS_OK
    } else {
        BN_COMPRESS_INVALID_HANDLE
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_bytes_release(handle: u64) -> i32 {
    if handle == 0 {
        return BN_COMPRESS_OK;
    }
    let buffers = bn_core_copy::table::<Buffers>();
    let mut holders = buffers.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count -= 1;
        if *count == 0 {
            holders.remove(&handle);
            buffers
                .values
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&handle);
        }
        BN_COMPRESS_OK
    } else {
        BN_COMPRESS_INVALID_HANDLE
    }
}

// ---------------------------------------------------------------------------
// Compression C ABI
// ---------------------------------------------------------------------------

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_deflate(handle: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(data) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "Deflate");
    };
    match deflate(&data) {
        Ok(compressed) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_buffer(compressed) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "Deflate"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_inflate(handle: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(data) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "Inflate");
    };
    match inflate(&data) {
        Ok(decompressed) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_buffer(decompressed) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "Inflate"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_gzip(handle: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(data) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "Gzip");
    };
    match gzip(&data) {
        Ok(compressed) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_buffer(compressed) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "Gzip"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_gunzip(handle: u64, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(data) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "Gunzip");
    };
    match gunzip(&data) {
        Ok(decompressed) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_buffer(decompressed) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "Gunzip"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_deflate_text(text: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let text = c_str(text).unwrap_or_default();
    match deflate(text.as_bytes()) {
        Ok(compressed) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_buffer(compressed) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "DeflateText"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_inflate_text(handle: u64, out: *mut *mut c_char) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(data) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "InflateText");
    };
    match inflate(&data) {
        Ok(decompressed) => match String::from_utf8(decompressed) {
            Ok(s) => {
                // SAFETY: pointer checked non-null.
                unsafe { *out = c_string(&s) };
                BN_COMPRESS_OK
            }
            Err(e) => failed(
                &CompressError::corrupt_data(format!("Invalid UTF-8: {e}")),
                "InflateText",
            ),
        },
        Err(e) => failed(&e, "InflateText"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_gzip_text(text: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let text = c_str(text).unwrap_or_default();
    match gzip(text.as_bytes()) {
        Ok(compressed) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_buffer(compressed) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "GzipText"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_compress_gunzip_text(handle: u64, out: *mut *mut c_char) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let Some(data) = get_buffer(handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "GunzipText");
    };
    match gunzip(&data) {
        Ok(decompressed) => match String::from_utf8(decompressed) {
            Ok(s) => {
                // SAFETY: pointer checked non-null.
                unsafe { *out = c_string(&s) };
                BN_COMPRESS_OK
            }
            Err(e) => failed(
                &CompressError::corrupt_data(format!("Invalid UTF-8: {e}")),
                "GunzipText",
            ),
        },
        Err(e) => failed(&e, "GunzipText"),
    }
}

// ---------------------------------------------------------------------------
// ZipArchive C ABI
// ---------------------------------------------------------------------------

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_create(path: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let path = c_str(path).unwrap_or_default();
    match ZipArchiveHandle::create(path) {
        Ok(archive) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_archive(archive) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "ZipArchive.Create"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_open(path: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let path = c_str(path).unwrap_or_default();
    match ZipArchiveHandle::open(path) {
        Ok(archive) => {
            // SAFETY: pointer checked non-null.
            unsafe { *out = store_archive(archive) };
            BN_COMPRESS_OK
        }
        Err(e) => failed(&e, "ZipArchive.Open"),
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_add_file(handle: u64, entry_name: *const c_char, disk_path: *const c_char) -> i32 {
    let entry_name = c_str(entry_name).unwrap_or_default();
    let disk_path = c_str(disk_path).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.AddFile");
        };
        match archive.add_file(entry_name, disk_path) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.AddFile"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_add_text(handle: u64, entry_name: *const c_char, content: *const c_char) -> i32 {
    let entry_name = c_str(entry_name).unwrap_or_default();
    let content = c_str(content).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.AddText");
        };
        match archive.add_text(entry_name, content) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.AddText"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_add_stored_text(handle: u64, entry_name: *const c_char, content: *const c_char) -> i32 {
    let entry_name = c_str(entry_name).unwrap_or_default();
    let content = c_str(content).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.AddStoredText");
        };
        match archive.add_stored_text(entry_name, content) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.AddStoredText"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_add_bytes(handle: u64, entry_name: *const c_char, bytes_handle: u64) -> i32 {
    let entry_name = c_str(entry_name).unwrap_or_default();
    let Some(data) = get_buffer(bytes_handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "ZipArchive.AddBytes");
    };
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.AddBytes");
        };
        match archive.add_bytes(entry_name, &data, false) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.AddBytes"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_add_stored_bytes(handle: u64, entry_name: *const c_char, bytes_handle: u64) -> i32 {
    let entry_name = c_str(entry_name).unwrap_or_default();
    let Some(data) = get_buffer(bytes_handle) else {
        return failed(&CompressError::not_found("Buffer not found"), "ZipArchive.AddStoredBytes");
    };
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.AddStoredBytes");
        };
        match archive.add_stored_bytes(entry_name, &data) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.AddStoredBytes"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_extract_all(handle: u64, destination_dir: *const c_char) -> i32 {
    let destination_dir = c_str(destination_dir).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.ExtractAll");
        };
        match archive.extract_all(destination_dir) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.ExtractAll"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_extract_file(handle: u64, entry_name: *const c_char, disk_path: *const c_char) -> i32 {
    let entry_name = c_str(entry_name).unwrap_or_default();
    let disk_path = c_str(disk_path).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.ExtractFile");
        };
        match archive.extract_file(entry_name, disk_path) {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.ExtractFile"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_extract_text(handle: u64, entry_name: *const c_char, out: *mut *mut c_char) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let entry_name = c_str(entry_name).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.ExtractText");
        };
        match archive.extract_text(entry_name) {
            Ok(text) => {
                // SAFETY: pointer checked non-null.
                unsafe { *out = c_string(&text) };
                BN_COMPRESS_OK
            }
            Err(e) => failed(&e, "ZipArchive.ExtractText"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_extract_bytes(handle: u64, entry_name: *const c_char, out: *mut u64) -> i32 {
    if out.is_null() {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    let entry_name = c_str(entry_name).unwrap_or_default();
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.ExtractBytes");
        };
        match archive.extract_bytes(entry_name) {
            Ok(bytes) => {
                // SAFETY: pointer checked non-null.
                unsafe { *out = store_buffer(bytes) };
                BN_COMPRESS_OK
            }
            Err(e) => failed(&e, "ZipArchive.ExtractBytes"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_entry_count(handle: u64) -> i64 {
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return -1;
        };
        archive.entry_count().map(|c| c as i64).unwrap_or(-1)
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_entry_name(handle: u64, index: i64, out: *mut *mut c_char) -> i32 {
    if out.is_null() || index < 0 {
        return BN_COMPRESS_INVALID_ARGUMENT;
    }
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return failed(&CompressError::not_found("ZipArchive not found"), "ZipArchive.EntryName");
        };
        match archive.entry_name(index as usize) {
            Ok(name) => {
                // SAFETY: pointer checked non-null.
                unsafe { *out = c_string(&name) };
                BN_COMPRESS_OK
            }
            Err(e) => failed(&e, "ZipArchive.EntryName"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_close(handle: u64) -> i32 {
    with_archives(|archives| {
        let Some(archive) = archives.get_mut(&handle) else {
            return BN_COMPRESS_OK;
        };
        match archive.close() {
            Ok(()) => BN_COMPRESS_OK,
            Err(e) => failed(&e, "ZipArchive.Close"),
        }
    })
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_retain(handle: u64) -> i32 {
    let archives = bn_core_copy::table::<Archives>();
    let mut holders = archives.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count += 1;
        BN_COMPRESS_OK
    } else {
        BN_COMPRESS_INVALID_HANDLE
    }
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_zip_release(handle: u64) -> i32 {
    if handle == 0 {
        return BN_COMPRESS_OK;
    }
    let archives = bn_core_copy::table::<Archives>();
    let mut holders = archives.holders.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(count) = holders.get_mut(&handle) {
        *count -= 1;
        if *count == 0 {
            holders.remove(&handle);
            if let Some(mut archive) = archives
                .values
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&handle)
            {
                let _ = archive.close();
            }
        }
        BN_COMPRESS_OK
    } else {
        BN_COMPRESS_INVALID_HANDLE
    }
}
