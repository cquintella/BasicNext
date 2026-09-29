// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::super::{OpenMode, RootedDir};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "bn-secure-fs-win-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// A directory junction, which needs no privilege (unlike a symlink).
fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .expect("run mklink");
    assert!(status.status.success(), "mklink /J failed: {status:?}");
}

#[test]
fn files_inside_the_root_write_read_and_remove() {
    let base = fixture("cycle");
    std::fs::create_dir(base.join("sub")).unwrap();
    let root = RootedDir::new(&base).unwrap();
    let path = base.join("sub").join("f.txt");
    root.open(&path, OpenMode::Write)
        .unwrap()
        .write_all(b"long text")
        .unwrap();
    // Write truncates, Append appends.
    root.open(&path, OpenMode::Write)
        .unwrap()
        .write_all(b"ab")
        .unwrap();
    root.open(&path, OpenMode::Append)
        .unwrap()
        .write_all(b"c")
        .unwrap();
    let mut text = String::new();
    root.open(&path, OpenMode::Read)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert_eq!(text, "abc");
    root.remove_file(&path).unwrap();
    assert!(!path.exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn junctions_and_parent_traversal_cannot_leave_the_root() {
    let base = fixture("escape");
    let outside = fixture("outside");
    std::fs::write(outside.join("secret.txt"), "secret").unwrap();
    junction(&base.join("jn"), &outside);
    let root = RootedDir::new(&base).unwrap();
    let through = base.join("jn").join("secret.txt");
    for mode in [OpenMode::Read, OpenMode::Write, OpenMode::Append] {
        let error = root.open(&through, mode).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    }
    assert!(root.remove_file(&through).is_err());
    // The junction itself is not a file to open or remove.
    assert!(root.open(&base.join("jn"), OpenMode::Read).is_err());
    assert!(root.remove_file(&base.join("jn")).is_err());
    assert!(
        root.open(&base.join("..").join("x"), OpenMode::Read)
            .is_err()
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("secret.txt")).unwrap(),
        "secret"
    );
    let _ = std::fs::remove_dir(base.join("jn"));
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&outside);
}
