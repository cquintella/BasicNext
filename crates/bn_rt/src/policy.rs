//! Execution policy for HOST calls (bucket 0.5.2a): one value type,
//! [`Policy`], shared by the interpreter (`HostEnv` owns one) and the compiled
//! image (one `RwLock` static installed by `bn_rt_policy_init`); one
//! environment parser, [`Policy::narrow_from_env`]; the filesystem decision
//! ([`FsPolicy`]) and the HOST.Exec ceilings live here and nowhere else. The
//! C ABI at the bottom is what LLVM-emitted `Start` calls before user code.
#![allow(unsafe_code)] // C ABI exports are the native policy boundary.

use std::{
    ffi::{CStr, c_char},
    fs::{File, OpenOptions},
    io,
    path::Path,
    sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Duration,
};

use super::secure_fs::{OpenMode, RootedDir};

pub const POLICY_CLOCK: u64 = 1 << 0;
pub const POLICY_CONSOLE: u64 = 1 << 1;
pub const POLICY_FILESYSTEM: u64 = 1 << 2;
pub const POLICY_NET: u64 = 1 << 3;
pub const POLICY_DISPATCH: u64 = 1 << 4;
pub const POLICY_RANDOM: u64 = 1 << 5;
pub const POLICY_EXEC: u64 = 1 << 6;
pub const POLICY_ALL: u64 = POLICY_CLOCK
    | POLICY_CONSOLE
    | POLICY_FILESYSTEM
    | POLICY_NET
    | POLICY_DISPATCH
    | POLICY_RANDOM
    | POLICY_EXEC;
pub const POLICY_VERSION: u32 = 1;
pub const POLICY_OK: i32 = 0;
pub const POLICY_INVALID: i32 = 2;

/// Compiled HOST.Exec ceilings (D-H1-02). Policy may reduce these, never widen.
pub const EXEC_CAPTURE_DEFAULT: usize = 16 * 1024 * 1024;
pub const EXEC_TIMEOUT_DEFAULT: Duration = Duration::from_secs(60);

const FS_OUTSIDE_POLICY: &str = "filesystem path is outside execution policy";
const FS_WRITES_DENIED: &str = "filesystem writes are denied by execution policy";

/// One side (read or write) of the filesystem scope. `Rooted` with no roots
/// denies every path.
#[derive(Clone, Debug)]
enum Scope {
    Unrestricted,
    Rooted(Vec<RootedDir>),
}

impl Scope {
    fn denies_everything(&self) -> bool {
        matches!(self, Self::Rooted(roots) if roots.is_empty())
    }

    fn root_for(&self, path: &Path) -> io::Result<Option<&RootedDir>> {
        match self {
            Self::Unrestricted => Ok(None),
            Self::Rooted(roots) => roots
                .iter()
                .filter(|root| root.contains(path))
                .max_by_key(|root| root.path().components().count())
                .map(Some)
                .ok_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, FS_OUTSIDE_POLICY)),
        }
    }
}

/// The filesystem decision: which paths may be read, written, or removed.
#[derive(Clone, Debug)]
pub struct FsPolicy {
    read: Scope,
    write: Scope,
    read_only: bool,
}

impl FsPolicy {
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            read: Scope::Unrestricted,
            write: Scope::Unrestricted,
            read_only: false,
        }
    }

    /// Every path denied (a sandbox with no roots).
    #[must_use]
    pub const fn denied() -> Self {
        Self {
            read: Scope::Rooted(Vec::new()),
            write: Scope::Rooted(Vec::new()),
            read_only: false,
        }
    }

    /// Reads and writes confined to canonical directory roots; an empty list
    /// denies that operation.
    #[must_use]
    pub const fn rooted(read_roots: Vec<RootedDir>, write_roots: Vec<RootedDir>) -> Self {
        Self {
            read: Scope::Rooted(read_roots),
            write: Scope::Rooted(write_roots),
            read_only: false,
        }
    }

    /// Adds a root to the read (or write) scope; an unrestricted scope becomes
    /// rooted at that directory.
    pub fn add_root(&mut self, write: bool, root: RootedDir) {
        let scope = if write {
            &mut self.write
        } else {
            &mut self.read
        };
        match scope {
            Scope::Rooted(roots) => roots.push(root),
            Scope::Unrestricted => *scope = Scope::Rooted(vec![root]),
        }
    }

    /// Denies writes and deletions while keeping the read scope.
    pub const fn set_read_only(&mut self) {
        self.read_only = true;
    }

    /// Whether `HOST.FileSystem` may be bound at all (some path could succeed).
    #[must_use]
    pub fn allows_capability(&self) -> bool {
        !self.read.denies_everything() || !(self.read_only || self.write.denies_everything())
    }

    #[must_use]
    pub fn allows_path(&self, path: &Path, write: bool) -> bool {
        if write && self.read_only {
            return false;
        }
        match if write { &self.write } else { &self.read } {
            Scope::Unrestricted => true,
            Scope::Rooted(roots) => roots.iter().any(|root| root.contains_resolved(path)),
        }
    }

    /// # Errors
    ///
    /// Propagates the I/O error; `PermissionDenied` when writes are denied or
    /// the path is outside the policy roots.
    pub fn open(&self, path: &Path, mode: OpenMode) -> io::Result<File> {
        let write = mode != OpenMode::Read;
        if write && self.read_only {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                FS_WRITES_DENIED,
            ));
        }
        let scope = if write { &self.write } else { &self.read };
        match scope.root_for(path)? {
            Some(root) => root.open(path, mode),
            None => {
                let mut options = OpenOptions::new();
                match mode {
                    OpenMode::Read => {
                        options.read(true);
                    }
                    OpenMode::Write => {
                        options.write(true).create(true).truncate(true);
                    }
                    OpenMode::Append => {
                        options.append(true).create(true);
                    }
                }
                options.open(path)
            }
        }
    }

    /// # Errors
    ///
    /// Propagates the I/O error; `PermissionDenied` when writes are denied or
    /// the path is outside the write roots.
    pub fn remove_file(&self, path: &Path) -> io::Result<()> {
        if self.read_only {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                FS_WRITES_DENIED,
            ));
        }
        match self.write.root_for(path)? {
            Some(root) => root.remove_file(path),
            None => std::fs::remove_file(path),
        }
    }
}

/// A policy input that the environment supplied with a value outside its domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PolicyError {
    pub variable: &'static str,
    pub value: String,
    pub expected: &'static str,
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid {} '{}' (expected {})",
            self.variable, self.value, self.expected
        )
    }
}

impl std::error::Error for PolicyError {}

/// The execution policy in force for one process: capability bits (tighten-only
/// under an artifact ceiling), HOST.Exec ceilings, and the filesystem scope.
#[derive(Clone, Debug)]
pub struct Policy {
    ceiling: u64,
    effective: u64,
    exec_timeout: Duration,
    exec_capture_limit: usize,
    fs: FsPolicy,
}

impl Policy {
    /// Everything allowed, compiled ceilings in force, filesystem unrestricted.
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            ceiling: POLICY_ALL,
            effective: POLICY_ALL,
            exec_timeout: EXEC_TIMEOUT_DEFAULT,
            exec_capture_limit: EXEC_CAPTURE_DEFAULT,
            fs: FsPolicy::unrestricted(),
        }
    }

    #[must_use]
    pub const fn allows(&self, capability: u64) -> bool {
        self.effective & capability == capability
    }

    /// The HOST.Exec policy for one call.
    #[must_use]
    pub const fn exec(&self) -> bn_host_exec::Policy {
        bn_host_exec::Policy {
            allowed: self.allows(POLICY_EXEC),
            timeout: self.exec_timeout,
            capture_limit: self.exec_capture_limit,
        }
    }

    #[must_use]
    pub const fn fs(&self) -> &FsPolicy {
        &self.fs
    }

    pub const fn fs_mut(&mut self) -> &mut FsPolicy {
        &mut self.fs
    }

    /// Installs or narrows the artifact ceiling; repeated calls never widen it.
    pub const fn install_ceiling(&mut self, ceiling: u64) {
        self.ceiling &= ceiling;
        self.effective &= self.ceiling;
    }

    /// Restricts the effective capabilities; bits outside the ceiling are ignored.
    pub const fn restrict(&mut self, mask: u64) {
        self.effective &= mask & self.ceiling;
    }

    pub const fn deny_exec(&mut self) {
        self.restrict(!POLICY_EXEC);
    }

    /// Denies the capability bit and every path.
    pub fn deny_filesystem(&mut self) {
        self.restrict(!POLICY_FILESYSTEM);
        self.fs = FsPolicy::denied();
    }

    /// Reduces the HOST.Exec wall-clock ceiling; larger values have no effect.
    pub fn reduce_exec_timeout(&mut self, timeout: Duration) {
        self.exec_timeout = self.exec_timeout.min(timeout);
    }

    /// Reduces the HOST.Exec per-stream capture ceiling; larger values have no effect.
    pub const fn reduce_exec_capture_limit(&mut self, bytes: usize) {
        if bytes < self.exec_capture_limit {
            self.exec_capture_limit = bytes;
        }
    }

    /// Narrows this policy from the four environment inputs
    /// (`BN_FS_POLICY`, `BN_EXEC_POLICY`, `BN_EXEC_CAPTURE_LIMIT`,
    /// `BN_EXEC_TIMEOUT_MS`), read through `get` so callers decide where the
    /// environment comes from and read it once. Absent or empty inputs leave
    /// the policy unchanged; inputs can only narrow it.
    ///
    /// # Errors
    ///
    /// Returns the first malformed input **without applying any of them**.
    pub fn narrow_from_env(
        &mut self,
        get: impl Fn(&str) -> Option<String>,
    ) -> Result<(), PolicyError> {
        let text = |variable: &'static str| get(variable).filter(|value| !value.is_empty());
        let integer = |variable: &'static str| -> Result<Option<u64>, PolicyError> {
            text(variable)
                .map(|value| {
                    value.parse::<u64>().map_err(|_| PolicyError {
                        variable,
                        value,
                        expected: "a non-negative integer",
                    })
                })
                .transpose()
        };
        let fs = match text("BN_FS_POLICY").as_deref() {
            None => None,
            Some("deny") => Some(true),
            Some("read-only") => Some(false),
            Some(value) => {
                return Err(PolicyError {
                    variable: "BN_FS_POLICY",
                    value: value.to_owned(),
                    expected: "deny or read-only",
                });
            }
        };
        let exec_denied = match text("BN_EXEC_POLICY").as_deref() {
            None => false,
            Some("deny") => true,
            Some(value) => {
                return Err(PolicyError {
                    variable: "BN_EXEC_POLICY",
                    value: value.to_owned(),
                    expected: "deny",
                });
            }
        };
        let capture_limit = integer("BN_EXEC_CAPTURE_LIMIT")?;
        let timeout_ms = integer("BN_EXEC_TIMEOUT_MS")?;
        match fs {
            Some(true) => self.deny_filesystem(),
            Some(false) => self.fs.set_read_only(),
            None => {}
        }
        if exec_denied {
            self.deny_exec();
        }
        if let Some(bytes) = capture_limit {
            self.reduce_exec_capture_limit(usize::try_from(bytes).unwrap_or(usize::MAX));
        }
        if let Some(ms) = timeout_ms {
            self.reduce_exec_timeout(Duration::from_millis(ms));
        }
        Ok(())
    }
}

/// The one policy static of the compiled image (D-P-02). `RwLock`, not
/// `OnceLock`: `bn_rt_policy_filesystem_root` and `bn_rt_policy_restrict`
/// tighten it after `bn_rt_policy_init`.
static POLICY: RwLock<Policy> = RwLock::new(Policy::unrestricted());

fn current() -> RwLockReadGuard<'static, Policy> {
    POLICY.read().unwrap_or_else(PoisonError::into_inner)
}

fn current_mut() -> RwLockWriteGuard<'static, Policy> {
    POLICY.write().unwrap_or_else(PoisonError::into_inner)
}

/// Tests that mutate the process-wide policy hold this guard and start from
/// an unrestricted policy; `Drop` restores it so unrelated tests never observe
/// a restricted static.
#[cfg(test)]
pub(crate) struct PolicyTestGuard(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
impl Drop for PolicyTestGuard {
    fn drop(&mut self) {
        *current_mut() = Policy::unrestricted();
    }
}

#[cfg(test)]
pub(crate) fn reset_for_tests() -> PolicyTestGuard {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    *current_mut() = Policy::unrestricted();
    PolicyTestGuard(guard)
}

#[must_use]
pub(crate) fn allows(capability: u64) -> bool {
    current().allows(capability)
}

/// The effective HOST.Exec policy, read once per call.
pub(crate) fn exec_policy() -> bn_host_exec::Policy {
    current().exec()
}

pub(crate) fn allows_path(path: &Path, write: bool) -> bool {
    current().fs().allows_path(path, write)
}

pub(crate) fn open_path(path: &Path, mode: OpenMode) -> io::Result<File> {
    current().fs().open(path, mode)
}

/// Installs or narrows the artifact ceiling and applies the environment
/// inputs. A malformed input is reported on stderr, returns `POLICY_INVALID`
/// and applies **nothing**; emitted `Start` then calls
/// [`bn_rt_policy_check`], which stops the process.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_init(version: u32, ceiling: u64) -> i32 {
    if version != POLICY_VERSION || ceiling & !POLICY_ALL != 0 {
        super::fail(
            "CONFIG_INVALID",
            "execution policy ceiling is not a version 1 capability mask",
        );
        return POLICY_INVALID;
    }
    let mut next = current().clone();
    next.install_ceiling(ceiling);
    if let Err(error) = next.narrow_from_env(|name| std::env::var(name).ok()) {
        super::fail("CONFIG_INVALID", &error.to_string());
        return POLICY_INVALID;
    }
    *current_mut() = next;
    POLICY_OK
}

/// Called by emitted `Start` right after `bn_rt_policy_init`: a non-`POLICY_OK`
/// status ends the process with exit status 2 before any user code runs
/// (D-P-04: malformed policy is fail-closed on both backends).
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_check(status: i32) {
    if status != POLICY_OK {
        std::process::exit(2);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_filesystem_sandboxed() -> i32 {
    *current_mut().fs_mut() = FsPolicy::denied();
    POLICY_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_filesystem_root(write: i32, path: *const c_char) -> i32 {
    let Some(path) = path_text(path) else {
        return POLICY_INVALID;
    };
    let Ok(root) = RootedDir::new(Path::new(&path)) else {
        return POLICY_INVALID;
    };
    current_mut().fs_mut().add_root(write != 0, root);
    POLICY_OK
}

fn path_text(path: *const c_char) -> Option<String> {
    if path.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(path).to_str().ok().map(str::to_owned) }
    }
}

/// Restricts the effective policy; bits outside the artifact ceiling are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_restrict(mask: u64) -> i32 {
    if mask & !POLICY_ALL != 0 {
        return POLICY_INVALID;
    }
    current_mut().restrict(mask);
    POLICY_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_masks_are_versioned_and_bounded() {
        assert_eq!(POLICY_VERSION, 1);
        assert_ne!(POLICY_ALL & POLICY_CONSOLE, 0);
        assert_eq!(bn_rt_policy_init(POLICY_VERSION, 1 << 63), POLICY_INVALID);
        assert_eq!(bn_rt_policy_restrict(1 << 63), POLICY_INVALID);
    }

    #[test]
    fn policy_intersection_cannot_widen_an_existing_restriction() {
        let mut policy = Policy::unrestricted();
        policy.restrict(POLICY_CONSOLE);
        policy.install_ceiling(POLICY_ALL);
        assert_eq!(policy.ceiling, POLICY_ALL);
        assert_eq!(policy.effective, POLICY_CONSOLE);
        policy.restrict(POLICY_ALL);
        assert_eq!(policy.effective, POLICY_CONSOLE);
        policy.install_ceiling(POLICY_NET);
        assert_eq!(policy.effective, 0);
        assert!(!policy.allows(POLICY_CONSOLE));
    }

    #[test]
    fn exec_ceilings_only_tighten() {
        let mut policy = Policy::unrestricted();
        policy.reduce_exec_timeout(Duration::from_secs(90));
        policy.reduce_exec_capture_limit(EXEC_CAPTURE_DEFAULT * 2);
        assert_eq!(policy.exec().timeout, EXEC_TIMEOUT_DEFAULT);
        assert_eq!(policy.exec().capture_limit, EXEC_CAPTURE_DEFAULT);
        policy.reduce_exec_timeout(Duration::from_millis(5));
        policy.reduce_exec_capture_limit(7);
        policy.reduce_exec_timeout(Duration::from_secs(1));
        assert_eq!(policy.exec().timeout, Duration::from_millis(5));
        assert_eq!(policy.exec().capture_limit, 7);
        assert!(policy.exec().allowed);
        policy.deny_exec();
        assert!(!policy.exec().allowed);
    }

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn narrow_from_env_covers_every_input_and_applies_nothing_on_error() {
        let mut policy = Policy::unrestricted();
        policy.narrow_from_env(env(&[])).expect("absent inputs");
        policy
            .narrow_from_env(env(&[
                ("BN_FS_POLICY", ""),
                ("BN_EXEC_POLICY", ""),
                ("BN_EXEC_CAPTURE_LIMIT", ""),
                ("BN_EXEC_TIMEOUT_MS", ""),
            ]))
            .expect("empty inputs");
        assert!(policy.allows(POLICY_ALL));
        assert_eq!(policy.exec().capture_limit, EXEC_CAPTURE_DEFAULT);

        let cases: [(&str, &str, &str); 4] = [
            ("BN_FS_POLICY", "bogus", "deny or read-only"),
            ("BN_EXEC_POLICY", "allow", "deny"),
            ("BN_EXEC_CAPTURE_LIMIT", "abc", "a non-negative integer"),
            ("BN_EXEC_TIMEOUT_MS", "-1", "a non-negative integer"),
        ];
        for (variable, value, expected) in cases {
            let mut policy = Policy::unrestricted();
            let error = policy
                .narrow_from_env(env(&[
                    (variable, value),
                    ("BN_EXEC_POLICY", "deny"),
                    ("BN_EXEC_CAPTURE_LIMIT", "10"),
                ]))
                .unwrap_err();
            assert_eq!(
                error,
                PolicyError {
                    variable,
                    value: value.to_owned(),
                    expected
                }
            );
            assert_eq!(
                error.to_string(),
                format!("invalid {variable} '{value}' (expected {expected})")
            );
            // Nothing applied: the valid inputs in the same call did not land.
            assert!(policy.allows(POLICY_EXEC));
            assert_eq!(policy.exec().capture_limit, EXEC_CAPTURE_DEFAULT);
        }

        let mut policy = Policy::unrestricted();
        policy
            .narrow_from_env(env(&[
                ("BN_FS_POLICY", "read-only"),
                ("BN_EXEC_POLICY", "deny"),
                ("BN_EXEC_CAPTURE_LIMIT", "1024"),
                ("BN_EXEC_TIMEOUT_MS", "200"),
            ]))
            .expect("valid inputs");
        assert!(!policy.allows(POLICY_EXEC));
        assert!(policy.allows(POLICY_FILESYSTEM));
        assert!(!policy.fs().allows_path(Path::new("/tmp"), true));
        assert!(policy.fs().allows_path(Path::new("/tmp"), false));
        assert_eq!(policy.exec().capture_limit, 1024);
        assert_eq!(policy.exec().timeout, Duration::from_millis(200));
        // A second narrowing cannot widen.
        policy
            .narrow_from_env(env(&[
                ("BN_EXEC_CAPTURE_LIMIT", "4096"),
                ("BN_EXEC_TIMEOUT_MS", "9000"),
            ]))
            .expect("valid inputs");
        assert_eq!(policy.exec().capture_limit, 1024);
        assert_eq!(policy.exec().timeout, Duration::from_millis(200));

        let mut policy = Policy::unrestricted();
        policy
            .narrow_from_env(env(&[("BN_FS_POLICY", "deny")]))
            .expect("deny");
        assert!(!policy.allows(POLICY_FILESYSTEM));
        assert!(!policy.fs().allows_capability());
        assert!(!policy.fs().allows_path(Path::new("/"), false));
    }

    #[test]
    fn filesystem_scope_semantics() {
        assert!(FsPolicy::unrestricted().allows_capability());
        assert!(!FsPolicy::denied().allows_capability());
        let mut read_only = FsPolicy::unrestricted();
        read_only.set_read_only();
        assert!(read_only.allows_capability());
        assert!(read_only.allows_path(Path::new("/"), false));
        assert!(!read_only.allows_path(Path::new("/"), true));
        assert_eq!(
            read_only
                .open(Path::new("/nonexistent/x"), OpenMode::Write)
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert_eq!(
            FsPolicy::denied()
                .open(Path::new("/nonexistent/x"), OpenMode::Read)
                .unwrap_err()
                .to_string(),
            FS_OUTSIDE_POLICY
        );
    }
}
