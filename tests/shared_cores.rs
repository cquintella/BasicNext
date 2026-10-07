// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The one-core gate (`tests/check-shared-cores.sh`) against trees made for
//! it: rule (f) fails when an ARC mechanism appears outside `bn_rt::arc`
//! (proposal `arc-shared-core-0.6.5`), and passes without it. The gate needs
//! ripgrep and fails closed; this test never skips it.

use std::{fs, path::Path, process::Command};

/// A tree that passes the other rules of the gate (one policy static).
fn tree(root: &Path, extra: Option<(&str, &str)>) {
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(root.join("crates/bn_rt/src")).expect("create tree");
    fs::create_dir_all(root.join("src")).expect("create tree");
    fs::write(
        root.join("crates/bn_rt/src/policy.rs"),
        "static POLICY: u8 = 0;\n",
    )
    .expect("write policy");
    fs::write(
        root.join("crates/bn_rt/src/arc.rs"),
        "pub struct ArcCore { strong_count: u64 }\n",
    )
    .expect("write core");
    if let Some((path, text)) = extra {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("create directory");
        fs::write(path, text).expect("write copy");
    }
}

fn gate(root: &Path) -> std::process::Output {
    // An explicit PATH makes Windows search it before System32, whose
    // `bash.exe` is the WSL launcher, not the Git Bash that runs CI.
    Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .arg("tests/check-shared-cores.sh")
        .arg("--root")
        .arg(root)
        .output()
        .expect("run the shared-cores gate")
}

#[test]
fn the_shared_cores_gate_rejects_a_second_arc_mechanism() {
    let root = std::env::temp_dir().join(format!("bn-shared-cores-{}", std::process::id()));
    tree(&root, None);
    let clean = gate(&root);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    for (path, copy) in [
        (
            "crates/bn_interp/src/heap.rs",
            "struct Slot { strong_count: usize }\n",
        ),
        (
            "crates/bn_llvm/src/llvm/arc.rs",
            "const WEAK: &str = \"@bn_arc_weak_objects\";\n",
        ),
        (
            "crates/bn_interp/src/core.rs",
            "fn bump(slot: &mut Slot) { slot.strong += 1; }\n",
        ),
    ] {
        tree(&root, Some((path, copy)));
        let output = gate(&root);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{path}: the gate passed");
        assert!(
            stderr.contains(&format!("ARC mechanism outside bn_rt::arc: {path}")),
            "{path}: {stderr}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}
