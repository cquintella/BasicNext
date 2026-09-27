#![allow(unsafe_code)]

use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

pub type BNFileHandle = u64;
pub const BN_FILE_OK: u32 = 0;
pub const BN_FILE_INVALID: u32 = 1;
pub const BN_FILE_ERROR: u32 = 2;
pub const BN_FILE_POLICY_DENIED: u32 = 3;

fn authorize() -> Result<(), u32> {
    if super::policy::allows(super::policy::POLICY_FILESYSTEM) {
        Ok(())
    } else {
        super::fail(
            "EXECUTION_POLICY_DENIED",
            "HOST.FileSystem is denied by execution policy",
        );
        Err(BN_FILE_POLICY_DENIED)
    }
}

struct FileRegistry {
    next: Option<BNFileHandle>,
    files: HashMap<BNFileHandle, FileResource>,
}

struct FileResource {
    file: File,
}

impl FileRegistry {
    fn reserve(&mut self) -> Result<BNFileHandle, u32> {
        let id = self.next.ok_or(BN_FILE_ERROR)?;
        self.next = id.checked_add(1);
        Ok(id)
    }
}

fn files() -> &'static Mutex<FileRegistry> {
    static FILES: OnceLock<Mutex<FileRegistry>> = OnceLock::new();
    FILES.get_or_init(|| {
        Mutex::new(FileRegistry {
            next: Some(1),
            files: HashMap::new(),
        })
    })
}
fn text(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(ptr) }
            .to_str()
            .ok()
            .map(str::to_owned)
    }
}

pub(crate) fn read_handle(handle: BNFileHandle) -> Result<String, u32> {
    authorize()?;
    let mut guard = files()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let file = &mut guard.files.get_mut(&handle).ok_or(BN_FILE_INVALID)?.file;
    let mut value = String::new();
    file.read_to_string(&mut value).map_err(|_| BN_FILE_ERROR)?;
    Ok(value)
}

pub(crate) fn write_handle(handle: BNFileHandle, value: &str) -> Result<(), u32> {
    authorize()?;
    let mut guard = files()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .files
        .get_mut(&handle)
        .ok_or(BN_FILE_INVALID)?
        .file
        .write_all(value.as_bytes())
        .map_err(|_| BN_FILE_ERROR)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_open(path: *const c_char, mode: i32, out: *mut BNFileHandle) -> u32 {
    let (Some(path), false) = (text(path), out.is_null()) else {
        return BN_FILE_INVALID;
    };
    // The ABI caller supplies a writable handle slot; failure never exposes an
    // uninitialized handle to generated code.
    unsafe { out.write(0) };
    if !(0..=2).contains(&mode) {
        return BN_FILE_INVALID;
    }
    if let Err(status) = authorize() {
        return status;
    }
    let mut guard = files()
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
    let open_mode = match mode {
        0 => super::secure_fs::OpenMode::Read,
        1 => super::secure_fs::OpenMode::Write,
        2 => super::secure_fs::OpenMode::Append,
        _ => return BN_FILE_INVALID,
    };
    let file = match super::policy::open_path(Path::new(&path), open_mode) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            super::fail(
                "EXECUTION_POLICY_DENIED",
                "filesystem path is outside execution policy",
            );
            return BN_FILE_POLICY_DENIED;
        }
        Err(_) => return BN_FILE_ERROR,
    };
    entry.insert(FileResource { file });
    unsafe {
        out.write(id);
    }
    BN_FILE_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_close(handle: BNFileHandle) -> u32 {
    files()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .files
        .remove(&handle)
        .map_or(BN_FILE_INVALID, |_| BN_FILE_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_read_all(handle: BNFileHandle, out: *mut *mut c_char) -> u32 {
    if out.is_null() {
        return BN_FILE_INVALID;
    }
    unsafe { out.write(std::ptr::null_mut()) };
    let value = match read_handle(handle) {
        Ok(value) => value,
        Err(status) => return status,
    };
    let bytes = value.as_bytes();
    let Ok(len) = bytes.len().checked_add(1).ok_or(()) else {
        return BN_FILE_ERROR;
    };
    let ptr = unsafe { libc::malloc(len) }.cast::<u8>();
    if ptr.is_null() {
        return BN_FILE_ERROR;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        ptr.add(bytes.len()).write(0);
        out.write(ptr.cast());
    }
    BN_FILE_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_write(handle: BNFileHandle, data: *const c_char) -> u32 {
    let Some(data) = text(data) else {
        return BN_FILE_INVALID;
    };
    write_handle(handle, &data).map_or_else(|status| status, |()| BN_FILE_OK)
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_file_string_free(data: *mut c_char) {
    if !data.is_null() {
        unsafe {
            libc::free(data.cast());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let name = CString::new(path.to_str().unwrap()).unwrap();
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(name.as_ptr(), 0, &mut handle), BN_FILE_OK);
        super::super::policy::bn_rt_policy_restrict(0);
        let mut denied = 99;
        let status = bn_rt_file_open(name.as_ptr(), 1, &mut denied);
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
        let mut handles = [0; 3];
        let paths: Vec<_> = (0..3)
            .map(|i| std::env::temp_dir().join(format!("bn-handles-{}-{i}", std::process::id())))
            .collect();
        for (i, path) in paths.iter().enumerate() {
            std::fs::write(path, format!("file{i}")).unwrap();
            if i == 2 {
                assert_eq!(bn_rt_file_close(handles[0]), BN_FILE_OK);
            }
            let path = CString::new(path.to_str().unwrap()).unwrap();
            assert_eq!(
                bn_rt_file_open(path.as_ptr(), 0, &mut handles[i]),
                BN_FILE_OK
            );
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
        let path = std::env::temp_dir().join(format!("bn-file-{}-{}.txt", std::process::id(), 1));
        let path_c = CString::new(path.to_string_lossy().as_bytes()).expect("path");
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(path_c.as_ptr(), 1, &mut handle), BN_FILE_OK);
        let data = CString::new("hello\n").expect("data");
        assert_eq!(bn_rt_file_write(handle, data.as_ptr()), BN_FILE_OK);
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        assert_eq!(bn_rt_file_open(path_c.as_ptr(), 0, &mut handle), BN_FILE_OK);
        let mut output = std::ptr::null_mut();
        assert_eq!(bn_rt_file_read_all(handle, &mut output), BN_FILE_OK);
        let actual = unsafe { CStr::from_ptr(output) }.to_bytes();
        assert_eq!(actual, b"hello\n");
        bn_rt_file_string_free(output);
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

        let name = CString::new(root.join("value.txt").to_string_lossy().as_bytes()).unwrap();
        let mut handle = 0;
        assert_eq!(bn_rt_file_open(name.as_ptr(), 0, &mut handle), BN_FILE_OK);
        assert_eq!(read_handle(handle).unwrap(), "inside");
        assert_eq!(bn_rt_file_close(handle), BN_FILE_OK);
        assert_eq!(
            std::fs::read_to_string(outside.join("value.txt")).unwrap(),
            "outside"
        );
        std::fs::remove_file(root).unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }
}
