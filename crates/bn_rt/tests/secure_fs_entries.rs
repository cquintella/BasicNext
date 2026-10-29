// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Entry operations of `FsPolicy` (bucket 0.6.12, FS-CORE-1): owner-only
//! creates, removal, rename and change of directory never follow a final
//! symbolic link, never act outside the policy roots, and treat a root as
//! readable but not writable. Unix only: the Windows host runs the same
//! rules with junctions in `secure_fs_windows_tests.rs`.

#![cfg(unix)]

use bn_rt::FsPolicy;
use bn_rt::secure_fs::{EntryKind, RootedDir, is_policy_refusal};
use std::fs;
use std::io::{ErrorKind, Write as _};
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const PRIVATE_DIRECTORY: u32 = 0o700;
const PRIVATE_FILE: u32 = 0o600;
const PERMISSION_BITS: u32 = 0o777;
const SWAP_ROUNDS: usize = 2000;

/// A fresh `root` (the policy root) and `outside` next to it.
struct Fixture {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!("bn-entries-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&outside).expect("create outside");
        Self {
            root: root.canonicalize().expect("canonical root"),
            outside: outside.canonicalize().expect("canonical outside"),
            base,
        }
    }

    fn rooted(&self) -> FsPolicy {
        let root = || RootedDir::new(&self.root).expect("pin root");
        FsPolicy::rooted(vec![root()], vec![root()])
    }

    fn outside_is_empty(&self) -> bool {
        fs::read_dir(&self.outside)
            .expect("read outside")
            .next()
            .is_none()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & PERMISSION_BITS
}

#[test]
fn created_entries_are_owner_only_with_or_without_a_root() {
    let fixture = Fixture::new("private");
    for (policy, directory) in [
        (fixture.rooted(), fixture.root.join("rooted")),
        (FsPolicy::unrestricted(), fixture.root.join("plain")),
    ] {
        policy
            .create_private_directory(&directory)
            .expect("directory");
        assert_eq!(mode(&directory), PRIVATE_DIRECTORY);
        let file = directory.join("secret.txt");
        let mut handle = policy.create_private_file(&file).expect("file");
        handle.write_all(b"secret").expect("write");
        assert_eq!(mode(&file), PRIVATE_FILE);
        assert_eq!(fs::read_to_string(&file).expect("read"), "secret");
    }
}

#[test]
fn an_existing_name_is_never_created_through() {
    let fixture = Fixture::new("existing");
    fs::write(fixture.root.join("present.txt"), "keep").expect("write");
    symlink(
        fixture.outside.join("target"),
        fixture.root.join("dangling"),
    )
    .expect("link");
    for policy in [fixture.rooted(), FsPolicy::unrestricted()] {
        for name in ["present.txt", "dangling"] {
            let path = fixture.root.join(name);
            let file = policy.create_private_file(&path).map(drop);
            let directory = policy.create_private_directory(&path);
            for result in [file, directory] {
                assert_eq!(
                    result.unwrap_err().kind(),
                    ErrorKind::AlreadyExists,
                    "{name}"
                );
            }
        }
    }
    assert_eq!(
        fs::read_to_string(fixture.root.join("present.txt")).expect("read"),
        "keep"
    );
    assert!(fixture.outside_is_empty());
}

#[test]
fn a_linked_directory_on_the_way_is_refused_under_a_root() {
    let fixture = Fixture::new("linked");
    symlink(&fixture.outside, fixture.root.join("linked")).expect("link");
    let policy = fixture.rooted();
    let inside = fixture.root.join("linked").join("new");
    for error in [
        policy.create_private_file(&inside).map(drop).unwrap_err(),
        policy.create_private_directory(&inside).unwrap_err(),
    ] {
        assert!(is_policy_refusal(&error), "{error}");
    }
    let outside = fixture.base.join("elsewhere");
    assert!(is_policy_refusal(
        &policy.create_private_directory(&outside).unwrap_err()
    ));
    assert!(fixture.outside_is_empty());
    assert!(!outside.exists());
}

#[test]
fn entry_kinds_never_follow_a_final_link() {
    let fixture = Fixture::new("kinds");
    fs::write(fixture.root.join("file"), "x").expect("write");
    fs::create_dir(fixture.root.join("directory")).expect("directory");
    symlink(fixture.root.join("directory"), fixture.root.join("link")).expect("link");
    let policy = fixture.rooted();
    for (name, kind) in [
        ("file", EntryKind::File),
        ("directory", EntryKind::Directory),
        ("link", EntryKind::Symlink),
    ] {
        assert_eq!(
            policy.entry_kind(&fixture.root.join(name)).expect(name),
            kind
        );
    }
    assert_eq!(
        policy.entry_kind(&fixture.root).expect("root"),
        EntryKind::Directory
    );
    let missing = policy
        .entry_kind(&fixture.root.join("missing"))
        .unwrap_err();
    assert_eq!(missing.kind(), ErrorKind::NotFound);
    let read = policy.open_entry(&fixture.root.join("link")).unwrap_err();
    assert_eq!(read.raw_os_error(), Some(libc::ELOOP));
}

#[test]
fn only_an_empty_directory_is_removed() {
    let fixture = Fixture::new("remove");
    let policy = fixture.rooted();
    let empty = fixture.root.join("empty");
    let full = fixture.root.join("full");
    fs::create_dir(&empty).expect("empty");
    fs::create_dir(&full).expect("full");
    fs::write(full.join("x"), "x").expect("write");
    fs::write(fixture.root.join("file"), "x").expect("write");
    symlink(&full, fixture.root.join("link")).expect("link");

    policy.remove_empty_directory(&empty).expect("remove empty");
    assert!(!empty.exists());
    let not_empty = policy.remove_empty_directory(&full).unwrap_err();
    assert_eq!(not_empty.kind(), ErrorKind::DirectoryNotEmpty);
    for name in ["link", "file"] {
        let error = policy
            .remove_empty_directory(&fixture.root.join(name))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::NotADirectory, "{name}");
    }
    assert!(fixture.root.join("link").exists() && full.join("x").exists());
    let missing = policy
        .remove_empty_directory(&fixture.root.join("missing"))
        .unwrap_err();
    assert_eq!(missing.kind(), ErrorKind::NotFound);
    assert!(is_policy_refusal(
        &policy.remove_empty_directory(&fixture.root).unwrap_err()
    ));
    assert!(fixture.root.is_dir());
}

#[test]
fn rename_replaces_a_regular_file_only() {
    let fixture = Fixture::new("rename");
    let policy = fixture.rooted();
    let root = &fixture.root;
    fs::write(root.join("a"), "first").expect("write");
    policy
        .rename_entry(&root.join("a"), &root.join("b"))
        .expect("rename");
    fs::write(root.join("c"), "second").expect("write");
    policy
        .rename_entry(&root.join("c"), &root.join("b"))
        .expect("replace");
    assert_eq!(fs::read_to_string(root.join("b")).expect("read"), "second");

    fs::create_dir(root.join("directory")).expect("directory");
    symlink(fixture.outside.join("target"), root.join("link")).expect("link");
    for target in ["directory", "link"] {
        let error = policy
            .rename_entry(&root.join("b"), &root.join(target))
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AlreadyExists, "{target}");
    }
    assert!(root.join("b").exists() && fixture.outside_is_empty());
    for (from, to) in [
        (root.clone(), root.join("x")),
        (root.join("b"), root.clone()),
    ] {
        assert!(is_policy_refusal(
            &policy.rename_entry(&from, &to).unwrap_err()
        ));
    }
    let away = fixture.base.join("away");
    assert!(is_policy_refusal(
        &policy.rename_entry(&root.join("b"), &away).unwrap_err()
    ));
    assert!(!away.exists());
}

/// A directory that becomes a link to `outside` while files are created in
/// it: the create walks the parent without following it, so nothing ever
/// lands outside the root (bucket risk R4).
#[test]
fn a_swapped_parent_never_receives_a_created_file() {
    let fixture = Fixture::new("swap");
    let policy = fixture.rooted();
    let racing = fixture.root.join("racing");
    let parked = fixture.root.join("parked");
    fs::create_dir(&racing).expect("racing");
    let running = Arc::new(AtomicBool::new(true));
    let swapper = {
        let (racing, parked, outside) = (racing.clone(), parked.clone(), fixture.outside.clone());
        let running = Arc::clone(&running);
        std::thread::spawn(move || {
            while running.load(Ordering::Relaxed) {
                let _ = fs::rename(&racing, &parked);
                let _ = symlink(&outside, &racing);
                let _ = fs::remove_file(&racing);
                let _ = fs::rename(&parked, &racing);
            }
        })
    };
    for round in 0..SWAP_ROUNDS {
        let file = racing.join(format!("f{round}"));
        let _ = policy.create_private_file(&file);
    }
    running.store(false, Ordering::Relaxed);
    swapper.join().expect("swapper");
    assert!(fixture.outside_is_empty());
}

/// A FIFO opened for reading would block until a writer comes; the read
/// open is non-blocking, so a FIFO swapped in for a checked file cannot hang
/// a copy (end-of-sprint audit, bucket 0.6.12).
#[test]
fn a_fifo_never_blocks_the_read_open() {
    const OPEN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);
    let fixture = Fixture::new("fifo");
    let fifo = fixture.root.join("pipe");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(made.success());
    let policy = fixture.rooted();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(policy.open_entry(&fifo).map(drop).is_ok());
    });
    assert!(
        receiver.recv_timeout(OPEN_DEADLINE).is_ok(),
        "the open blocked"
    );
    assert_eq!(
        fixture
            .rooted()
            .entry_kind(&fixture.root.join("pipe"))
            .expect("kind"),
        EntryKind::Other
    );
}
