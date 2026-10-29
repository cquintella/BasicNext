// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The 0.6.12 `HOST.FileSystem` operations of `bn_rt::file` (language/0.6/
//! host.md, "Directories, private files, copy and move"; bucket 0.6.12,
//! FS-CORE-2): every row of gate G-3 and every field of the `Error`, the
//! same for both backends. Paths are absolute: the working directory is
//! tested alone in `file_working_directory.rs`.

#![cfg(unix)]

use bn_rt::FsPolicy;
use bn_rt::file::{
    FileError, copy, create_directory, is_directory, move_entry, open_new, remove_directory,
};
use bn_rt::secure_fs::RootedDir;
use bn_types::error_codes::fs;
use std::fs as stdfs;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};

const PRIVATE_FILE: u32 = 0o600;
const PERMISSION_BITS: u32 = 0o777;
const SPECIAL_AND_SHARED: u32 = 0o4755;

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    outside: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("bn-file-entries-{name}-{}", std::process::id()));
        let _ = stdfs::remove_dir_all(&base);
        stdfs::create_dir_all(base.join("root")).expect("create root");
        stdfs::create_dir_all(base.join("outside")).expect("create outside");
        Self {
            root: base.join("root").canonicalize().expect("canonical root"),
            outside: base
                .join("outside")
                .canonicalize()
                .expect("canonical outside"),
            base,
        }
    }

    fn at(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn rooted(&self) -> FsPolicy {
        let pin = || RootedDir::new(&self.root).expect("pin root");
        FsPolicy::rooted(vec![pin()], vec![pin()])
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = stdfs::remove_dir_all(&self.base);
    }
}

fn code<T>(result: Result<T, FileError>) -> i32 {
    result.err().map_or(0, |error| error.code())
}

#[test]
fn every_undefined_case_of_gate_g3_has_its_code() {
    let fixture = Fixture::new("g3");
    let policy = fixture.rooted();
    stdfs::write(fixture.at("file"), "data").expect("write");
    stdfs::create_dir(fixture.at("directory")).expect("directory");
    stdfs::write(fixture.at("directory/inner"), "x").expect("write");
    symlink(fixture.at("file"), fixture.at("link")).expect("link");
    let at = |name: &str| fixture.at(name);
    let remove = |name| code(remove_directory(&policy, &at(name)));
    let create = |name| code(create_directory(&policy, &at(name)));
    let open = |name| code(open_new(&policy, &at(name)));
    let copied = |from, to| code(copy(&policy, &at(from), &at(to)));
    let moved = |from, to| code(move_entry(&policy, &at(from), &at(to)));

    let fifo = std::process::Command::new("mkfifo")
        .arg(fixture.at("pipe"))
        .status()
        .expect("run mkfifo");
    assert!(fifo.success());
    let cases = [
        ("remove a file", remove("file"), fs::IS_NOT_DIRECTORY),
        ("remove a link", remove("link"), fs::IS_NOT_DIRECTORY),
        (
            "remove with entries",
            remove("directory"),
            fs::DIRECTORY_NOT_EMPTY,
        ),
        ("remove missing", remove("missing"), fs::NOT_FOUND),
        (
            "create, missing parent",
            create("none/inner"),
            fs::NOT_FOUND,
        ),
        (
            "open new, missing parent",
            open("none/inner"),
            fs::NOT_FOUND,
        ),
        ("copy a link", copied("link", "copy"), fs::INVALID_ARGUMENT),
        ("copy a FIFO", copied("pipe", "copy"), fs::INVALID_ARGUMENT),
        (
            "copy a directory",
            copied("directory", "copy"),
            fs::IS_DIRECTORY,
        ),
        ("copy missing", copied("missing", "copy"), fs::NOT_FOUND),
        (
            "copy onto a file",
            copied("file", "directory/inner"),
            fs::ALREADY_EXISTS,
        ),
        (
            "copy onto a link",
            copied("file", "link"),
            fs::ALREADY_EXISTS,
        ),
        (
            "move onto a directory",
            moved("file", "directory"),
            fs::ALREADY_EXISTS,
        ),
        (
            "move onto a link",
            moved("file", "link"),
            fs::ALREADY_EXISTS,
        ),
        ("move missing", moved("missing", "x"), fs::NOT_FOUND),
    ];
    for (case, actual, expected) in cases {
        assert_eq!(actual, expected, "{case}");
    }
    assert!(!fixture.at("copy").exists());
    assert_eq!(
        stdfs::read_to_string(fixture.at("file")).expect("read"),
        "data"
    );
    assert_eq!(
        stdfs::read_link(fixture.at("link")).expect("link"),
        fixture.at("file")
    );
}

#[test]
fn a_policy_root_is_readable_and_never_written() {
    let fixture = Fixture::new("root");
    let policy = fixture.rooted();
    let root = &fixture.root;
    assert!(is_directory(&policy, root).expect("root is a directory"));
    assert_eq!(code(create_directory(&policy, root)), fs::ALREADY_EXISTS);
    let removed = remove_directory(&policy, root).unwrap_err();
    assert_eq!(removed.code(), fs::POLICY_DENIED);
    assert_eq!(
        removed.cause(),
        "the policy root itself is not a write target"
    );
    stdfs::write(fixture.at("file"), "x").expect("write");
    assert_eq!(
        code(move_entry(&policy, root, &fixture.at("x"))),
        fs::POLICY_DENIED
    );
    assert_eq!(
        code(move_entry(&policy, &fixture.at("file"), root)),
        fs::POLICY_DENIED
    );
    assert!(root.is_dir() && fixture.at("file").exists());
}

#[test]
fn a_link_is_followed_by_no_operation() {
    let fixture = Fixture::new("links");
    let policy = fixture.rooted();
    stdfs::create_dir(fixture.at("directory")).expect("directory");
    symlink(fixture.at("directory"), fixture.at("to-directory")).expect("link");
    symlink(fixture.outside.join("target"), fixture.at("dangling")).expect("link");
    symlink(&fixture.outside, fixture.at("to-outside")).expect("link");

    assert!(!is_directory(&policy, &fixture.at("to-directory")).expect("kind"));
    let removed = remove_directory(&policy, &fixture.at("to-directory")).unwrap_err();
    assert_eq!(
        removed.cause(),
        "the path is a symbolic link, which is not followed"
    );
    for result in [
        create_directory(&policy, &fixture.at("dangling")),
        open_new(&policy, &fixture.at("dangling")).map(drop),
    ] {
        assert_eq!(code(result), fs::ALREADY_EXISTS);
    }
    let through = open_new(&policy, &fixture.at("to-outside/new"))
        .map(drop)
        .unwrap_err();
    assert_eq!(through.code(), fs::POLICY_DENIED);
    move_entry(&policy, &fixture.at("dangling"), &fixture.at("renamed")).expect("rename the link");
    assert!(
        stdfs::symlink_metadata(fixture.at("renamed"))
            .expect("link")
            .is_symlink()
    );
    assert!(fixture.at("directory").is_dir());
    assert_eq!(stdfs::read_dir(&fixture.outside).expect("read").count(), 0);
}

#[test]
fn copy_keeps_only_the_bytes_and_move_keeps_the_entry() {
    let fixture = Fixture::new("copy");
    let policy = fixture.rooted();
    let source = fixture.at("source");
    stdfs::write(&source, "payload").expect("write");
    stdfs::set_permissions(&source, stdfs::Permissions::from_mode(SPECIAL_AND_SHARED))
        .expect("chmod");

    copy(&policy, &source, &fixture.at("copy")).expect("copy");
    assert_eq!(
        stdfs::read_to_string(fixture.at("copy")).expect("read"),
        "payload"
    );
    let mode = stdfs::metadata(fixture.at("copy"))
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & PERMISSION_BITS, PRIVATE_FILE);
    assert_eq!(mode & !PERMISSION_BITS & 0o7000, 0);

    stdfs::write(fixture.at("old"), "old").expect("write");
    move_entry(&policy, &source, &fixture.at("old")).expect("replace a file");
    assert_eq!(
        stdfs::read_to_string(fixture.at("old")).expect("read"),
        "payload"
    );
    let moved = stdfs::metadata(fixture.at("old"))
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(moved & 0o7777, SPECIAL_AND_SHARED);
    assert!(!source.exists());
}

#[test]
fn errors_name_the_operation_and_the_paths() {
    let fixture = Fixture::new("errors");
    let policy = fixture.rooted();
    stdfs::write(fixture.at("a"), "x").expect("write");
    let created = create_directory(&policy, &fixture.at("a")).unwrap_err();
    assert_eq!(created.operation(), "HOST.FileSystem.CreateDirectory");
    assert_eq!(
        created.message(),
        format!(
            "cannot create the directory \"{}\"",
            fixture.at("a").display()
        )
    );
    assert_eq!(
        created.cause(),
        "the path already exists, and an existing entry is never replaced or changed"
    );
    let copied = copy(&policy, &fixture.at("a"), &fixture.at("a")).unwrap_err();
    assert_eq!(copied.operation(), "HOST.FileSystem.Copy");
    assert_eq!(
        copied.message(),
        format!(
            "cannot copy \"{}\" to \"{}\"",
            fixture.at("a").display(),
            fixture.at("a").display()
        )
    );
    let outside = fixture.base.join("away");
    let moved = move_entry(&policy, &fixture.at("a"), &outside).unwrap_err();
    assert_eq!(
        (moved.code(), moved.operation()),
        (fs::POLICY_DENIED, "HOST.FileSystem.Move")
    );
    assert!(!outside.exists() && fixture.at("a").exists());
    let denied = open_new(&FsPolicy::denied(), &fixture.at("b"))
        .map(drop)
        .unwrap_err();
    assert_eq!(denied.code(), fs::POLICY_DENIED);
    assert!(!Path::new(&fixture.at("b")).exists());
}
