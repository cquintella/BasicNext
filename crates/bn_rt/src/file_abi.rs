#![allow(unsafe_code)]

//! C ABI for `HOST.FileSystem`. The semantics are [`super::file`], shared
//! with the interpreter; this layer keeps the handle table, converts C
//! values, and records each failure's message for the emitted `Error`
//! ([`super::set_error`], read back through `bn_rt_error_take`).

use std::collections::HashMap;
use std::ffi::c_char;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::file::{FileError, OpenFile};

pub type BNFileHandle = u64;
pub const BN_FILE_OK: u32 = 0;
pub const BN_FILE_INVALID: u32 = 1;
pub const BN_FILE_ERROR: u32 = 2;
pub const BN_FILE_POLICY_DENIED: u32 = 3;
/// `EOF` from `ReadLine` / `ReadBytes`: a success, not an `Error`.
pub const BN_FILE_EOF: u32 = 4;

fn authorize() -> Result<(), u32> {
    if super::policy::allows(super::policy::POLICY_FILESYSTEM) {
        Ok(())
    } else {
        // The Error carries the denial; nothing is printed (as in `bni`).
        super::set_error("HOST.FileSystem is denied by execution policy");
        Err(BN_FILE_POLICY_DENIED)
    }
}

struct FileSlot {
    file: OpenFile,
    references: u32,
}

struct FileRegistry {
    next: Option<BNFileHandle>,
    files: HashMap<BNFileHandle, FileSlot>,
}

impl FileRegistry {
    fn reserve(&mut self) -> Result<BNFileHandle, u32> {
        let id = self.next.ok_or(BN_FILE_ERROR)?;
        self.next = id.checked_add(1);
        Ok(id)
    }
}

/// The open files of the module copy that runs on this thread
/// (`bn_core_copy`): a task's close when the task ends.
struct FileTable(Mutex<FileRegistry>);

impl Default for FileTable {
    fn default() -> Self {
        Self(Mutex::new(FileRegistry {
            next: Some(1),
            files: HashMap::new(),
        }))
    }
}

fn files() -> Arc<FileTable> {
    bn_core_copy::table::<FileTable>()
}

fn text(ptr: *const c_char) -> Option<String> {
    super::string::string_text(ptr)
}

/// A core failure: its report becomes the next `Error`.
fn failed(error: &FileError) -> u32 {
    super::set_error_report(
        error.code(),
        error.operation(),
        error.message(),
        error.cause(),
    );
    BN_FILE_ERROR
}

/// Runs `f` on an open-or-closed file under the capability check; an unknown
/// handle is `INVALID`.
fn with_file<T>(
    handle: BNFileHandle,
    f: impl FnOnce(&mut OpenFile) -> Result<T, FileError>,
) -> Result<T, u32> {
    authorize()?;
    with_file_unchecked(handle, f)
}

/// `Close` needs no capability: it only releases what `Open` granted.
fn with_file_unchecked<T>(
    handle: BNFileHandle,
    f: impl FnOnce(&mut OpenFile) -> Result<T, FileError>,
) -> Result<T, u32> {
    let table = files();
    let mut guard = table
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(slot) = guard.files.get_mut(&handle) else {
        super::set_error(bn_catalog::text("file", "msg.file_handle_is_invalid_2"));
        return Err(BN_FILE_INVALID);
    };
    f(&mut slot.file).map_err(|error| failed(&error))
}

pub fn read_handle(handle: BNFileHandle) -> Result<String, u32> {
    with_file(handle, OpenFile::read_all)
}

pub fn write_handle(handle: BNFileHandle, value: &str) -> Result<(), u32> {
    with_file(handle, |file| file.write(value, false))
}

/// Writes an owned NUL-terminated copy of `value` to `out` (freed with
/// `crate::string::bn_rt_str_release`).
fn write_owned(out: *mut *mut c_char, value: &str) -> u32 {
    let ptr = super::string::c_string_from_bytes(value.as_bytes());
    if ptr.is_null() {
        super::set_error("out of memory");
        return BN_FILE_ERROR;
    }
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe {
        out.write(ptr);
    }
    BN_FILE_OK
}

fn path_argument(path: *const c_char) -> Result<String, u32> {
    text(path).ok_or_else(|| {
        super::set_error("path is not valid UTF-8");
        BN_FILE_INVALID
    })
}

/// `NEW FS.File()`: a file that was never opened (host.md); its methods,
/// `Close` aside, return `Error(FS.CLOSED)` naming that cause.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_new(out: *mut BNFileHandle) -> u32 {
    if out.is_null() {
        return BN_FILE_INVALID;
    }
    let table = files();
    let mut guard = table
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let id = match guard.reserve() {
        Ok(id) => id,
        Err(status) => return status,
    };
    guard.files.insert(
        id,
        FileSlot {
            file: super::file::OpenFile::default(),
            references: 1,
        },
    );
    // SAFETY: emitted code passes a writable handle slot.
    unsafe { out.write(id) };
    BN_FILE_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_open(path: *const c_char, mode: i32, out: *mut BNFileHandle) -> u32 {
    if out.is_null() {
        return BN_FILE_INVALID;
    }
    // The ABI caller supplies a writable handle slot; failure never exposes an
    // uninitialized handle to generated code.
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { out.write(0) };
    let path = match path_argument(path) {
        Ok(path) => path,
        Err(status) => return status,
    };
    let mode = match super::file::open_mode(Path::new(&path), mode.into()) {
        Ok(mode) => mode,
        Err(error) => return failed(&error),
    };
    if let Err(status) = authorize() {
        return status;
    }
    let table = files();
    let mut guard = table
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let id = match guard.reserve() {
        Ok(id) => id,
        Err(status) => return status,
    };
    // Reservation precedes create/truncate, and IDs are never recycled.
    let std::collections::hash_map::Entry::Vacant(entry) = guard.files.entry(id) else {
        return BN_FILE_ERROR;
    };
    match super::policy::with_fs(|policy| super::file::open(policy, Path::new(&path), mode)) {
        Ok(file) => {
            entry.insert(FileSlot {
                file,
                references: 1,
            });
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { out.write(id) };
            BN_FILE_OK
        }
        Err(error) => failed(&error),
    }
}

/// `Close()`: the handle stays valid as a closed file until `RELEASE`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_close(handle: BNFileHandle) -> u32 {
    with_file_unchecked(handle, OpenFile::close).map_or_else(|status| status, |()| BN_FILE_OK)
}

/// One more binding holds `handle`. The last `bn_rt_file_release` drops it.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_retain(handle: BNFileHandle) -> u32 {
    let table = files();
    let mut guard = table
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(slot) = guard.files.get_mut(&handle) else {
        super::set_error(bn_catalog::text("file", "msg.file_handle_is_invalid_2"));
        return BN_FILE_INVALID;
    };
    slot.references = match slot.references.checked_add(1) {
        Some(count) => count,
        None => {
            super::set_error(bn_catalog::text(
                "file",
                "msg.file_handle_count_overflowed_2",
            ));
            return BN_FILE_ERROR;
        }
    };
    BN_FILE_OK
}

/// One fewer binding. The last one drops the handle and closes an open file.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_release(handle: BNFileHandle) -> u32 {
    let table = files();
    let mut guard = table
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(slot) = guard.files.get_mut(&handle) else {
        super::set_error(bn_catalog::text("file", "msg.file_handle_is_invalid_2"));
        return BN_FILE_INVALID;
    };
    if slot.references > 1 {
        slot.references -= 1;
        return BN_FILE_OK;
    }
    guard.files.remove(&handle);
    BN_FILE_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_read_all(handle: BNFileHandle, out: *mut *mut c_char) -> u32 {
    if out.is_null() {
        return BN_FILE_INVALID;
    }
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { out.write(std::ptr::null_mut()) };
    match read_handle(handle) {
        Ok(value) => write_owned(out, &value),
        Err(status) => status,
    }
}

/// `ReadLine()`: `BN_FILE_OK` with the line in `out`, or `BN_FILE_EOF`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_read_line(handle: BNFileHandle, out: *mut *mut c_char) -> u32 {
    if out.is_null() {
        return BN_FILE_INVALID;
    }
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { out.write(std::ptr::null_mut()) };
    match with_file(handle, OpenFile::read_line) {
        Ok(Some(line)) => write_owned(out, &line),
        Ok(None) => BN_FILE_EOF,
        Err(status) => status,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_write(handle: BNFileHandle, data: *const c_char) -> u32 {
    let Some(data) = text(data) else {
        return BN_FILE_INVALID;
    };
    write_handle(handle, &data).map_or_else(|status| status, |()| BN_FILE_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_write_line(handle: BNFileHandle, data: *const c_char) -> u32 {
    let Some(data) = text(data) else {
        return BN_FILE_INVALID;
    };
    with_file(handle, |file| file.write(&data, true)).map_or_else(|status| status, |()| BN_FILE_OK)
}

/// `ReadBytes(buffer)`: `BN_FILE_OK` with the count in `out`, or
/// `BN_FILE_EOF`. `buffer` holds `len` bytes.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_read_bytes(
    handle: BNFileHandle,
    buffer: *mut u8,
    len: i64,
    out: *mut i64,
) -> u32 {
    let (Ok(len), false) = (usize::try_from(len), out.is_null() || buffer.is_null()) else {
        return BN_FILE_INVALID;
    };
    // SAFETY: `out` was checked non-null and points at a writable `i64` slot.
    unsafe { out.write(0) };
    // SAFETY: the caller passes a live buffer of `len` bytes.
    let buffer = unsafe { std::slice::from_raw_parts_mut(buffer, len) };
    match with_file(handle, |file| file.read_bytes(buffer)) {
        Ok(Some(count)) => {
            unsafe { out.write(i64::try_from(count).unwrap_or(i64::MAX)) };
            BN_FILE_OK
        }
        Ok(None) => BN_FILE_EOF,
        Err(status) => status,
    }
}

/// `WriteBytes(buffer, count)`; the caller has checked `count` against
/// `LEN(buffer)`.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_write_bytes(
    handle: BNFileHandle,
    buffer: *const u8,
    count: i64,
) -> u32 {
    let (Ok(count), false) = (usize::try_from(count), buffer.is_null()) else {
        return BN_FILE_INVALID;
    };
    // SAFETY: the caller passes a live buffer of at least `count` bytes.
    let bytes = unsafe { std::slice::from_raw_parts(buffer, count) };
    with_file(handle, |file| file.write_bytes(bytes)).map_or_else(|status| status, |()| BN_FILE_OK)
}

/// `FS.Exists(path)`: `out` receives 1 or 0.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_fs_exists(path: *const c_char, out: *mut i32) -> u32 {
    if out.is_null() {
        return BN_FILE_INVALID;
    }
    // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
    unsafe { out.write(0) };
    let path = match path_argument(path) {
        Ok(path) => path,
        Err(status) => return status,
    };
    if let Err(status) = authorize() {
        return status;
    }
    match super::policy::with_fs(|policy| super::file::exists(policy, Path::new(&path))) {
        Ok(found) => {
            // SAFETY: the pointer was checked non-null and points at a writable slot the caller allocated.
            unsafe { out.write(i32::from(found)) };
            BN_FILE_OK
        }
        Err(error) => failed(&error),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_fs_delete_file(path: *const c_char) -> u32 {
    let path = match path_argument(path) {
        Ok(path) => path,
        Err(status) => return status,
    };
    if let Err(status) = authorize() {
        return status;
    }
    super::policy::with_fs(|policy| super::file::delete_file(policy, Path::new(&path)))
        .map_or_else(|error| failed(&error), |()| BN_FILE_OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;
    #[cfg(unix)]
    use std::ffi::CString;

    #[test]
    fn exhausted_handles_never_wrap() {
        let mut registry = FileRegistry {
            next: Some(u64::MAX),
            files: HashMap::new(),
        };
        assert_eq!(registry.reserve(), Ok(u64::MAX));
        assert_eq!(registry.reserve(), Err(BN_FILE_ERROR));
        assert_eq!(registry.reserve(), Err(BN_FILE_ERROR));
    }

    #[test]
    #[allow(clippy::borrow_as_ptr)]
    fn filesystem_policy_denial_preserves_files() {
        const CHILD: &str = "BN_FILE_POLICY_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "file_abi::tests::filesystem_policy_denial_preserves_files",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
            return;
        }
        let path = std::env::temp_dir().join(format!("bn-file-policy-{}", std::process::id()));
        std::fs::write(&path, b"preserve").unwrap();
        let name = crate::string::c_string_from_bytes(path.to_str().unwrap().as_bytes());
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(name, 0, &mut handle), BN_FILE_OK);
        super::super::policy::restrict_for_tests(0);
        let mut denied = 99;
        let status = bn_rt_file_open(name, 1, &mut denied);
        assert_ne!(status, BN_FILE_OK);
        assert_eq!(denied, 0);
        assert!(read_handle(handle).is_err());
        assert!(write_handle(handle, "bad").is_err());
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        assert_eq!(std::fs::read(&path).unwrap(), b"preserve");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    #[allow(clippy::borrow_as_ptr)]
    fn closing_one_file_does_not_replace_another_handle() {
        // Other tests restrict the process-wide policy; hold its lock.
        let _policy = super::super::policy::reset_for_tests();
        let mut handles = [0; 3];
        let paths: Vec<_> = (0..3)
            .map(|i| std::env::temp_dir().join(format!("bn-handles-{}-{i}", std::process::id())))
            .collect();
        for (i, path) in paths.iter().enumerate() {
            std::fs::write(path, format!("file{i}")).unwrap();
            if i == 2 {
                assert_eq!(bn_rt_file_close(handles[0]), BN_FILE_OK);
            }
            let path = crate::string::c_string_from_bytes(path.to_str().unwrap().as_bytes());
            assert_eq!(bn_rt_file_open(path, 0, &mut handles[i]), BN_FILE_OK);
        }
        assert_ne!(handles[1], handles[2]);
        assert!(read_handle(handles[0]).is_err());
        assert_eq!(read_handle(handles[1]).unwrap(), "file1");
        assert_eq!(read_handle(handles[2]).unwrap(), "file2");
        for handle in &handles[1..] {
            assert_eq!(bn_rt_file_close(*handle), BN_FILE_OK);
        }
        for path in paths {
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    #[allow(clippy::borrow_as_ptr)]
    fn file_abi_round_trip_and_close() {
        // Other tests restrict the process-wide policy; hold its lock.
        let _policy = super::super::policy::reset_for_tests();
        let path = std::env::temp_dir().join(format!("bn-file-{}-{}.txt", std::process::id(), 1));
        let path_c = crate::string::c_string_from_bytes(path.to_string_lossy().as_bytes());
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(path_c, 1, &mut handle), BN_FILE_OK);
        let data = crate::string::c_string_from_bytes(b"hello\n");
        assert_eq!(bn_rt_file_write(handle, data), BN_FILE_OK);
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        assert_eq!(bn_rt_file_open(path_c, 0, &mut handle), BN_FILE_OK);
        let mut output = std::ptr::null_mut();
        assert_eq!(bn_rt_file_read_all(handle, &mut output), BN_FILE_OK);
        // SAFETY: the pointer is non-null and NUL-terminated for this read.
        let actual = unsafe { CStr::from_ptr(output) }.to_bytes();
        assert_eq!(actual, b"hello\n");
        crate::string::bn_rt_str_release(output);
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        let _ = std::fs::remove_file(path);
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::borrow_as_ptr)]
    fn rooted_file_open_cannot_be_redirected_after_policy_configuration() {
        const CHILD: &str = "BN_FILE_ROOT_SWAP_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "file_abi::tests::rooted_file_open_cannot_be_redirected_after_policy_configuration",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
            return;
        }

        let base = std::env::temp_dir().join(format!("bn-file-root-swap-{}", std::process::id()));
        let root = base.join("root");
        let moved = base.join("configured-root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("value.txt"), "inside").unwrap();
        std::fs::write(outside.join("value.txt"), "outside").unwrap();
        assert_eq!(super::super::policy::bn_rt_policy_filesystem_sandboxed(), 0);
        let root_name = CString::new(root.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(
            super::super::policy::bn_rt_policy_filesystem_root(0, root_name.as_ptr()),
            0
        );
        std::fs::rename(&root, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &root).unwrap();

        let name =
            crate::string::c_string_from_bytes(root.join("value.txt").to_string_lossy().as_bytes());
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(name, 0, &mut handle), BN_FILE_OK);
        assert_eq!(read_handle(handle).unwrap(), "inside");
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        assert_eq!(
            std::fs::read_to_string(outside.join("value.txt")).unwrap(),
            "outside"
        );
        std::fs::remove_file(root).unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    #[allow(clippy::borrow_as_ptr)]
    fn last_release_drops_the_file_and_close_does_not() {
        let _policy = super::super::policy::reset_for_tests();
        let path = std::env::temp_dir().join(format!("bn-file-retain-{}", std::process::id()));
        std::fs::write(&path, b"kept").unwrap();
        let name = crate::string::c_string_from_bytes(path.to_str().unwrap().as_bytes());
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(name, 0, &mut handle), BN_FILE_OK);
        assert_eq!(bn_rt_file_retain(handle), BN_FILE_OK);
        assert_eq!(bn_rt_file_release(handle), BN_FILE_OK);
        assert_eq!(read_handle(handle).unwrap(), "kept");
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        assert!(read_handle(handle).is_err());
        assert_eq!(bn_rt_file_release(handle), BN_FILE_OK);
        assert_eq!(bn_rt_file_release(handle), BN_FILE_INVALID);
        assert!(!files().0.lock().unwrap().files.contains_key(&handle));
        assert_ne!(
            bn_rt_file_open(std::ptr::null(), 0, &mut handle),
            BN_FILE_OK
        );
        let bad_utf8 = [0xFFu8, 0x00u8];
        assert_ne!(
            bn_rt_file_open(bad_utf8.as_ptr().cast(), 0, &mut handle),
            BN_FILE_OK
        );
        std::fs::remove_file(path).unwrap();
    }
}
