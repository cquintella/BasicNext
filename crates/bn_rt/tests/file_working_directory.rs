// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The working directory (language/0.6/host.md, "Working directory"; bucket
//! 0.6.12, R3 and R8). The working directory belongs to the process, so this
//! binary holds a single test: no other test can run while it changes
//! directory.

#![cfg(unix)]

use bn_rt::FsPolicy;
use bn_rt::file::{get_current_directory, open_new, set_current_directory};
use bn_rt::secure_fs::RootedDir;
use bn_types::error_codes::fs;
use std::fs as stdfs;
use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::Path;

#[test]
fn the_working_directory_is_entered_without_links_and_bounds_relative_paths() {
    let base = std::env::temp_dir().join(format!("bn-cwd-{}", std::process::id()));
    let _ = stdfs::remove_dir_all(&base);
    let root = base.join("root");
    stdfs::create_dir_all(root.join("sub")).expect("create root");
    stdfs::create_dir_all(base.join("outside")).expect("create outside");
    let root = root.canonicalize().expect("canonical root");
    stdfs::write(root.join("file"), "x").expect("write");
    symlink(base.join("outside"), root.join("link")).expect("link");
    let pin = || RootedDir::new(&root).expect("pin root");
    let policy = FsPolicy::rooted(vec![pin()], vec![pin()]);
    let original = std::env::current_dir().expect("current directory");

    // The primitive: fchdir on the checked directory, never through a link.
    policy
        .change_directory(&root.join("sub"))
        .expect("enter sub");
    let entered = std::env::current_dir().expect("current directory");
    let file = policy.change_directory(&root.join("file")).unwrap_err();
    let link = policy.change_directory(&root.join("link")).unwrap_err();

    // The operation: codes, relative paths, and the read-root check.
    set_current_directory(&policy, &root).expect("enter the root");
    let at_root = get_current_directory(&policy).expect("current directory");
    let created = open_new(&policy, Path::new("sub/relative.txt")).map(drop);
    let escaping = open_new(&policy, Path::new("../escape.txt"))
        .map(drop)
        .unwrap_err();
    let entering_a_file = set_current_directory(&policy, Path::new("file")).unwrap_err();
    let entering_a_link = set_current_directory(&policy, Path::new("link")).unwrap_err();
    std::env::set_current_dir(base.join("outside")).expect("leave the root");
    let outside = get_current_directory(&policy).unwrap_err();
    std::env::set_current_dir(&original).expect("restore");

    assert_eq!(entered.canonicalize().expect("canonical"), root.join("sub"));
    assert_eq!(file.kind(), ErrorKind::NotADirectory);
    assert_eq!(link.kind(), ErrorKind::NotADirectory);
    assert_eq!(Path::new(&at_root).canonicalize().expect("canonical"), root);
    assert!(created.is_ok() && root.join("sub/relative.txt").exists());
    assert_eq!(escaping.code(), fs::POLICY_DENIED);
    assert!(!base.join("escape.txt").exists());
    assert_eq!(entering_a_file.code(), fs::IS_NOT_DIRECTORY);
    assert_eq!(entering_a_file.cause(), "the path is not a directory");
    assert_eq!(entering_a_link.code(), fs::IS_NOT_DIRECTORY);
    assert_eq!(
        entering_a_link.cause(),
        "the path is a symbolic link, which is not followed"
    );
    assert_eq!(
        (outside.code(), outside.operation()),
        (fs::POLICY_DENIED, "HOST.FileSystem.GetCurrentDirectory")
    );
    assert_eq!(outside.message(), "cannot read the current directory");
    let _ = stdfs::remove_dir_all(&base);
}
