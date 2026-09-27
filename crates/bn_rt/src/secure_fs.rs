//! Descriptor-relative filesystem access for rooted execution policies.

#![allow(unsafe_code)] // Narrow libc boundary for descriptor-relative openat/unlinkat.

use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenMode {
    Read,
    Write,
    Append,
}

#[derive(Clone, Debug)]
pub struct RootedDir {
    path: PathBuf,
    canonical_path: PathBuf,
    directory: Arc<File>,
}

impl RootedDir {
    /// Opens and pins a filesystem-policy root.
    ///
    /// # Errors
    ///
    /// Returns an error if `path` is missing, is not a directory, or cannot be
    /// opened without following a symlink at the final component.
    pub fn new(path: &Path) -> io::Result<Self> {
        let configured_path = std::path::absolute(path)?;
        let canonical_path = configured_path.canonicalize()?;
        if !canonical_path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "filesystem policy root is not a directory",
            ));
        }
        #[cfg(unix)]
        let directory = {
            use std::os::unix::fs::MetadataExt as _;
            use std::os::unix::fs::OpenOptionsExt as _;
            let expected = std::fs::metadata(&canonical_path)?;
            let directory = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW)
                .open(&canonical_path)?;
            let actual = directory.metadata()?;
            if (expected.dev(), expected.ino()) != (actual.dev(), actual.ino()) {
                return Err(denied("filesystem policy root changed while it was opened"));
            }
            directory
        };
        #[cfg(not(unix))]
        let directory = File::open(&canonical_path)?;
        Ok(Self {
            path: configured_path,
            canonical_path,
            directory: Arc::new(directory),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn contains(&self, path: &Path) -> bool {
        self.relative(path).is_ok()
    }

    #[must_use]
    pub fn contains_resolved(&self, path: &Path) -> bool {
        let candidate = if path.exists() {
            path.canonicalize().ok()
        } else {
            path.parent()
                .and_then(|parent| parent.canonicalize().ok())
                .and_then(|parent| path.file_name().map(|name| parent.join(name)))
        };
        candidate.is_some_and(|candidate| candidate.starts_with(&self.canonical_path))
    }

    /// Opens `path` relative to the pinned root without following symlinks.
    ///
    /// # Errors
    ///
    /// Returns `PermissionDenied` for paths outside this root, paths containing
    /// parent traversal, and platforms without descriptor-relative traversal.
    pub fn open(&self, path: &Path, mode: OpenMode) -> io::Result<File> {
        let relative = self.relative(path)?;
        #[cfg(unix)]
        {
            self.open_unix(&relative, mode)
        }
        #[cfg(not(unix))]
        {
            let _ = (relative, mode);
            Err(denied(
                "rooted filesystem access is unavailable on this platform",
            ))
        }
    }

    /// Removes a regular file relative to the pinned root.
    ///
    /// # Errors
    ///
    /// Returns an error if traversal would leave the root, any component is a
    /// symlink, or the target is not a regular file.
    pub fn remove_file(&self, path: &Path) -> io::Result<()> {
        let relative = self.relative(path)?;
        #[cfg(unix)]
        {
            self.remove_unix(&relative)
        }
        #[cfg(not(unix))]
        {
            let _ = relative;
            Err(denied(
                "rooted filesystem access is unavailable on this platform",
            ))
        }
    }

    fn relative(&self, path: &Path) -> io::Result<PathBuf> {
        let absolute = std::path::absolute(path)?;
        let relative = absolute
            .strip_prefix(&self.path)
            .or_else(|_| absolute.strip_prefix(&self.canonical_path))
            .map_err(|_| denied("filesystem path is outside the configured root"))?;
        if relative.as_os_str().is_empty()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(denied("filesystem path contains invalid traversal"));
        }
        Ok(relative.to_path_buf())
    }

    #[cfg(unix)]
    fn open_unix(&self, relative: &Path, mode: OpenMode) -> io::Result<File> {
        use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};

        let mut directory = self.directory.try_clone()?;
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                continue;
            };
            let name = c_name(name)?;
            if components.peek().is_some() {
                // SAFETY: `directory` owns a live directory descriptor and
                // `name` is a NUL-terminated single path component.
                let descriptor = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                    )
                };
                if descriptor < 0 {
                    return Err(traversal_error());
                }
                // SAFETY: successful `openat` returned a new owned descriptor.
                directory = File::from(unsafe { OwnedFd::from_raw_fd(descriptor) });
                continue;
            }
            let flags = match mode {
                OpenMode::Read => libc::O_RDONLY,
                OpenMode::Write => libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
                OpenMode::Append => libc::O_WRONLY | libc::O_CREAT | libc::O_APPEND,
            } | libc::O_CLOEXEC
                | libc::O_NOFOLLOW;
            // SAFETY: the descriptor and component satisfy `openat`; the mode
            // argument is supplied for the two branches that include O_CREAT.
            let descriptor =
                unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags, 0o666) };
            if descriptor < 0 {
                return Err(traversal_error());
            }
            // SAFETY: successful `openat` returned a new owned descriptor.
            return Ok(File::from(unsafe { OwnedFd::from_raw_fd(descriptor) }));
        }
        Err(denied("filesystem path has no file component"))
    }

    #[cfg(unix)]
    fn remove_unix(&self, relative: &Path) -> io::Result<()> {
        use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd};

        let mut directory = self.directory.try_clone()?;
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                continue;
            };
            let name = c_name(name)?;
            if components.peek().is_some() {
                // SAFETY: `directory` owns a live directory descriptor and
                // `name` is a NUL-terminated single path component.
                let descriptor = unsafe {
                    libc::openat(
                        directory.as_raw_fd(),
                        name.as_ptr(),
                        libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                    )
                };
                if descriptor < 0 {
                    return Err(traversal_error());
                }
                // SAFETY: successful `openat` returned a new owned descriptor.
                directory = File::from(unsafe { OwnedFd::from_raw_fd(descriptor) });
                continue;
            }
            // SAFETY: final-component inspection is relative to the pinned
            // parent and O_NOFOLLOW rejects a symlink target.
            let target = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if target < 0 {
                return Err(traversal_error());
            }
            // SAFETY: successful `openat` returned a new owned descriptor.
            let target = File::from(unsafe { OwnedFd::from_raw_fd(target) });
            if !target.metadata()?.is_file() {
                return Err(denied("filesystem deletion target is not a regular file"));
            }
            // SAFETY: the live parent descriptor and component are the same
            // pair validated above; unlinkat never follows a final symlink.
            let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
            return if result == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            };
        }
        Err(denied("filesystem path has no file component"))
    }
}

fn denied(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

#[cfg(unix)]
fn traversal_error() -> io::Error {
    let error = io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) {
        denied("filesystem path traverses a symlink")
    } else {
        error
    }
}

#[cfg(unix)]
fn c_name(name: &std::ffi::OsStr) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt as _;
    std::ffi::CString::new(name.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "filesystem path contains NUL"))
}

#[cfg(test)]
mod tests {
    use super::{OpenMode, RootedDir};
    use std::io::{Read, Write};

    fn fixture(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "bn-secure-fs-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[cfg(unix)]
    #[test]
    fn final_symlinks_cannot_be_read_or_truncated() {
        let base = fixture("final-link");
        let root = base.join("root");
        let outside = base.join("outside.txt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("link.txt")).unwrap();
        let rooted = RootedDir::new(&root).unwrap();

        assert!(rooted.open(&root.join("link.txt"), OpenMode::Read).is_err());
        assert!(
            rooted
                .open(&root.join("link.txt"), OpenMode::Write)
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_symlinks_cannot_escape_the_root() {
        let base = fixture("parent-link");
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "secret").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("redirect")).unwrap();
        let rooted = RootedDir::new(&root).unwrap();

        assert!(
            rooted
                .open(&root.join("redirect/secret.txt"), OpenMode::Read)
                .is_err()
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn held_root_descriptor_prevents_root_replacement_race() {
        let base = fixture("root-swap");
        let root = base.join("root");
        let moved = base.join("original-root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("value.txt"), "inside").unwrap();
        std::fs::write(outside.join("value.txt"), "outside").unwrap();
        let rooted = RootedDir::new(&root).unwrap();
        std::fs::rename(&root, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &root).unwrap();

        let mut file = rooted
            .open(&root.join("value.txt"), OpenMode::Write)
            .unwrap();
        file.write_all(b"safe").unwrap();
        drop(file);
        assert_eq!(
            std::fs::read_to_string(outside.join("value.txt")).unwrap(),
            "outside"
        );
        assert_eq!(
            std::fs::read_to_string(moved.join("value.txt")).unwrap(),
            "safe"
        );
        std::fs::remove_file(root).unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rooted_read_uses_the_opened_file() {
        let base = fixture("read");
        let root = base.join("root");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("value.txt"), "inside").unwrap();
        let rooted = RootedDir::new(&root).unwrap();

        let mut text = String::new();
        rooted
            .open(&root.join("value.txt"), OpenMode::Read)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "inside");
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_be_appended_or_removed() {
        let base = fixture("append-delete-link");
        let root = base.join("root");
        let outside = base.join("outside.txt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&outside, "secret").unwrap();
        let link = root.join("link.txt");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let rooted = RootedDir::new(&root).unwrap();

        assert!(rooted.open(&link, OpenMode::Append).is_err());
        assert!(rooted.remove_file(&link).is_err());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
        assert!(link.is_symlink());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_intermediate_symlink_swap_never_reads_outside() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let base = fixture("concurrent-swap");
        let root = base.join("root");
        let live = root.join("live");
        let parked = root.join("parked");
        let outside = base.join("outside");
        std::fs::create_dir_all(&live).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(live.join("value.txt"), "inside").unwrap();
        std::fs::write(outside.join("value.txt"), "outside").unwrap();
        let rooted = RootedDir::new(&root).unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let attacker_running = Arc::clone(&running);
        let attacker_live = live.clone();
        let attacker_parked = parked.clone();
        let attacker_outside = outside.clone();
        let attacker = std::thread::spawn(move || {
            for _ in 0..2_000 {
                std::fs::rename(&attacker_live, &attacker_parked).unwrap();
                std::os::unix::fs::symlink(&attacker_outside, &attacker_live).unwrap();
                std::fs::remove_file(&attacker_live).unwrap();
                std::fs::rename(&attacker_parked, &attacker_live).unwrap();
            }
            attacker_running.store(false, Ordering::Release);
        });

        while running.load(Ordering::Acquire) {
            if let Ok(mut file) = rooted.open(&live.join("value.txt"), OpenMode::Read) {
                let mut text = String::new();
                file.read_to_string(&mut text).unwrap();
                assert_eq!(text, "inside");
            }
        }
        attacker.join().unwrap();
        std::fs::remove_dir_all(base).unwrap();
    }
}
