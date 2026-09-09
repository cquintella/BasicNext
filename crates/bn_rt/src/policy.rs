//! Versioned execution-policy ceiling for compiled HOST calls.
#![allow(unsafe_code)] // C ABI exports are the native policy boundary.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::{
    ffi::{CStr, c_char},
    path::Path,
    sync::{Mutex, OnceLock},
};

pub const POLICY_CLOCK: u64 = 1 << 0;
pub const POLICY_CONSOLE: u64 = 1 << 1;
pub const POLICY_FILESYSTEM: u64 = 1 << 2;
pub const POLICY_NET: u64 = 1 << 3;
pub const POLICY_DISPATCH: u64 = 1 << 4;
pub const POLICY_RANDOM: u64 = 1 << 5;
pub const POLICY_ALL: u64 = POLICY_CLOCK
    | POLICY_CONSOLE
    | POLICY_FILESYSTEM
    | POLICY_NET
    | POLICY_DISPATCH
    | POLICY_RANDOM;
pub const POLICY_VERSION: u32 = 1;
pub const POLICY_OK: i32 = 0;
pub const POLICY_INVALID: i32 = 2;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PolicyState {
    ceiling: u64,
    effective: u64,
}

#[cfg(test)]
impl PolicyState {
    const fn new() -> Self {
        Self {
            ceiling: POLICY_ALL,
            effective: POLICY_ALL,
        }
    }

    const fn install_ceiling(self, ceiling: u64) -> Self {
        let ceiling = self.ceiling & ceiling;
        Self {
            ceiling,
            effective: self.effective & ceiling,
        }
    }

    const fn restrict(self, mask: u64) -> Self {
        Self {
            ceiling: self.ceiling,
            effective: self.effective & mask & self.ceiling,
        }
    }
}

static CEILING: AtomicU64 = AtomicU64::new(POLICY_ALL);
static EFFECTIVE: AtomicU64 = AtomicU64::new(POLICY_ALL);
static FILESYSTEM_SANDBOXED: AtomicBool = AtomicBool::new(false);
static FILESYSTEM_READ_ROOTS: OnceLock<Mutex<Vec<super::secure_fs::RootedDir>>> = OnceLock::new();
static FILESYSTEM_WRITE_ROOTS: OnceLock<Mutex<Vec<super::secure_fs::RootedDir>>> = OnceLock::new();

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    CEILING.store(POLICY_ALL, Ordering::Release);
    EFFECTIVE.store(POLICY_ALL, Ordering::Release);
    FILESYSTEM_SANDBOXED.store(false, Ordering::Release);
    FILESYSTEM_READ_ONLY.store(false, Ordering::Release);
    if let Some(roots) = FILESYSTEM_READ_ROOTS.get() {
        roots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    if let Some(roots) = FILESYSTEM_WRITE_ROOTS.get() {
        roots
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

#[must_use]
pub(crate) fn allows(capability: u64) -> bool {
    EFFECTIVE.load(Ordering::Acquire) & capability == capability
}

/// Installs or narrows the artifact ceiling. Repeated calls can never widen it.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_init(version: u32, ceiling: u64) -> i32 {
    if version != POLICY_VERSION || ceiling & !POLICY_ALL != 0 {
        return POLICY_INVALID;
    }
    CEILING.fetch_and(ceiling, Ordering::AcqRel);
    let installed = CEILING.load(Ordering::Acquire);
    EFFECTIVE.fetch_and(installed, Ordering::AcqRel);
    match std::env::var("BN_FS_POLICY").as_deref() {
        Ok("deny") => {
            EFFECTIVE.fetch_and(!POLICY_FILESYSTEM, Ordering::AcqRel);
        }
        Ok("read-only") => {
            FILESYSTEM_READ_ONLY.store(true, Ordering::Release);
        }
        Ok("") | Err(_) => {}
        Ok(_) => return POLICY_INVALID,
    }
    POLICY_OK
}

static FILESYSTEM_READ_ONLY: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_filesystem_sandboxed() -> i32 {
    FILESYSTEM_SANDBOXED.store(true, Ordering::Release);
    roots(true)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    roots(false)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
    POLICY_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_filesystem_root(write: i32, path: *const c_char) -> i32 {
    let Some(path) = path_text(path) else {
        return POLICY_INVALID;
    };
    let Ok(path) = canonical_root(Path::new(&path)) else {
        return POLICY_INVALID;
    };
    let target = roots(write != 0);
    target
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(path);
    POLICY_OK
}

fn roots(write: bool) -> &'static Mutex<Vec<super::secure_fs::RootedDir>> {
    if write {
        FILESYSTEM_WRITE_ROOTS.get_or_init(|| Mutex::new(Vec::new()))
    } else {
        FILESYSTEM_READ_ROOTS.get_or_init(|| Mutex::new(Vec::new()))
    }
}

fn path_text(path: *const c_char) -> Option<String> {
    if path.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(path).to_str().ok().map(str::to_owned) }
    }
}

fn canonical_root(path: &Path) -> Result<super::secure_fs::RootedDir, ()> {
    super::secure_fs::RootedDir::new(path).map_err(|_| ())
}

pub(crate) fn allows_path(path: &Path, write: bool) -> bool {
    if write && FILESYSTEM_READ_ONLY.load(Ordering::Acquire) {
        return false;
    }
    if !FILESYSTEM_SANDBOXED.load(Ordering::Acquire) {
        return true;
    }
    let guard = roots(write)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard.iter().any(|root| root.contains_resolved(path))
}

pub(crate) fn open_path(
    path: &Path,
    mode: super::secure_fs::OpenMode,
) -> std::io::Result<std::fs::File> {
    if mode != super::secure_fs::OpenMode::Read && FILESYSTEM_READ_ONLY.load(Ordering::Acquire) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "filesystem writes are denied by execution policy",
        ));
    }
    if !FILESYSTEM_SANDBOXED.load(Ordering::Acquire) {
        let mut options = std::fs::OpenOptions::new();
        match mode {
            super::secure_fs::OpenMode::Read => {
                options.read(true);
            }
            super::secure_fs::OpenMode::Write => {
                options.write(true).create(true).truncate(true);
            }
            super::secure_fs::OpenMode::Append => {
                options.append(true).create(true);
            }
        }
        return options.open(path);
    }
    let write = mode != super::secure_fs::OpenMode::Read;
    let guard = roots(write)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    guard
        .iter()
        .filter(|root| root.contains(path))
        .max_by_key(|root| root.path().components().count())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "filesystem path is outside execution policy",
            )
        })?
        .open(path, mode)
}

/// Restricts the effective policy; bits outside the artifact ceiling are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_restrict(mask: u64) -> i32 {
    if mask & !POLICY_ALL != 0 {
        return POLICY_INVALID;
    }
    let ceiling = CEILING.load(Ordering::Acquire);
    EFFECTIVE.fetch_and(mask & ceiling, Ordering::AcqRel);
    POLICY_OK
}

#[cfg(test)]
mod tests {
    use super::{POLICY_ALL, POLICY_CONSOLE, POLICY_INVALID, POLICY_VERSION, PolicyState};

    #[test]
    fn policy_masks_are_versioned_and_bounded() {
        assert_eq!(POLICY_VERSION, 1);
        assert_ne!(POLICY_ALL & POLICY_CONSOLE, 0);
        assert_eq!(
            super::bn_rt_policy_init(POLICY_VERSION, 1 << 63),
            POLICY_INVALID
        );
        assert_eq!(super::bn_rt_policy_restrict(1 << 63), POLICY_INVALID);
    }

    #[test]
    fn policy_intersection_cannot_widen_an_existing_restriction() {
        let state = PolicyState::new()
            .restrict(POLICY_CONSOLE)
            .install_ceiling(POLICY_ALL);
        assert_eq!(state.ceiling, POLICY_ALL);
        assert_eq!(state.effective, POLICY_CONSOLE);
        assert_eq!(state.restrict(POLICY_ALL).effective, POLICY_CONSOLE);
    }
}
