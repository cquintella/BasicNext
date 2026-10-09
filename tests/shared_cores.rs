// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The one-core gate (`tests/check-shared-cores.sh`) against trees made for
//! it: rule (f) fails when an ARC mechanism appears outside `bn_rt::arc`
//! (proposal `arc-shared-core-0.6.5`), rule (g) when a `HOST.Env` provider
//! reads the environment outside `bn_host_env`; each passes without it. The gate needs
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

/// Rule (g): a `HOST.Env` provider that reads the environment itself, instead
/// of through `bn_host_env`, fails the gate; `bn_host_env` may read it.
#[test]
fn the_shared_cores_gate_rejects_a_second_env_reader() {
    let root = std::env::temp_dir().join(format!("bn-shared-env-{}", std::process::id()));
    let core = (
        "crates/bn_host_env/src/lib.rs",
        "// HOST.Env.Get\nfn get(name: &str) { std::env::var_os(name); }\n",
    );
    tree(&root, Some(core));
    let clean = gate(&root);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    for (path, copy) in [
        (
            "crates/bn_rt/src/env.rs",
            "// HOST.Env.Get\nfn get(name: &str) { std::env::var_os(name); }\n",
        ),
        (
            "crates/bn_interpret_driver/src/hosts/env.rs",
            "// HOST.Env.Has\nfn has(name: &str) -> bool { std::env::var(name).is_ok() }\n",
        ),
    ] {
        tree(&root, Some((path, copy)));
        let output = gate(&root);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{path}: the gate passed");
        assert!(
            stderr.contains(&format!(
                "HOST.Env reads the environment outside bn_host_env: {path}"
            )),
            "{path}: {stderr}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

/// Rule (h): `BNDispatch` queue worker pool and state outside `bn_core_dispatch`
/// fail the gate.
#[test]
fn the_shared_cores_gate_rejects_a_second_dispatch_worker_pool() {
    let root = std::env::temp_dir().join(format!("bn-shared-dispatch-{}", std::process::id()));
    let core = (
        "crates/bn_core_dispatch/src/lib.rs",
        "struct QueueInner { worker_handles: Vec<()> }\n",
    );
    tree(&root, Some(core));
    let clean = gate(&root);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    for (path, copy) in [
        (
            "crates/bn_lib_dispatch/src/dispatch.rs",
            "struct QueueInner { worker_handles: Vec<()> }\n",
        ),
        (
            "crates/bn_rt/src/dispatch_abi.rs",
            "struct QueueInner { worker_handles: Vec<()> }\n",
        ),
    ] {
        tree(&root, Some((path, copy)));
        let output = gate(&root);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{path}: the gate passed");
        assert!(
            stderr.contains(&format!(
                "BNDispatch worker pool or queue state outside bn_core_dispatch: {path}"
            )),
            "{path}: {stderr}"
        );
    }
    let _ = fs::remove_dir_all(&root);
}

/// Rule (i): civil temporal arithmetic outside `bn_core_text` fails the gate.
#[test]
fn the_shared_cores_gate_rejects_duplicated_civil_calendar_arithmetic() {
    let root = std::env::temp_dir().join(format!("bn-shared-civil-{}", std::process::id()));
    let core = (
        "crates/bn_core_text/src/civil.rs",
        "pub fn days_from_civil(y: i32, m: u32, d: u32) -> Option<i32> { None }\n",
    );
    tree(&root, Some(core));
    let clean = gate(&root);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    let duplicate = (
        "crates/bn_rt/src/civil.rs",
        "pub fn days_from_civil(y: i32, m: u32, d: u32) -> Option<i32> { None }\n",
    );
    tree(&root, Some(duplicate));
    let output = gate(&root);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the gate passed on duplicate civil"
    );
    assert!(
        stderr
            .contains("civil calendar conversions outside bn_core_text: crates/bn_rt/src/civil.rs"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Rule (j): statistical reductions outside `bn_core_math` fail the gate.
#[test]
fn the_shared_cores_gate_rejects_duplicated_statistical_reductions() {
    let root = std::env::temp_dir().join(format!("bn-shared-math-{}", std::process::id()));
    let core = (
        "crates/bn_core_math/src/reduce.rs",
        "pub fn reduce_f64(name: &str, values: &[f64]) -> () { () }\n",
    );
    tree(&root, Some(core));
    let clean = gate(&root);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    let duplicate = (
        "crates/bn_lib_math/src/reduce.rs",
        "pub fn reduce_f64(name: &str, values: &[f64]) -> () { () }\n",
    );
    tree(&root, Some(duplicate));
    let output = gate(&root);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the gate passed on duplicate reduce"
    );
    assert!(
        stderr.contains(
            "statistical reduction core outside bn_core_math: crates/bn_lib_math/src/reduce.rs"
        ),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Rule (k): log dispatch logic outside `bn_core_log` fails the gate.
#[test]
fn the_shared_cores_gate_rejects_log_dispatch_outside_bn_core_log() {
    let root = std::env::temp_dir().join(format!("bn-shared-log-{}", std::process::id()));
    let core = (
        "crates/bn_core_log/src/lib.rs",
        "pub fn dispatch_log() -> () { () }\n",
    );
    tree(&root, Some(core));
    let clean = gate(&root);
    assert!(
        clean.status.success(),
        "{}",
        String::from_utf8_lossy(&clean.stderr)
    );
    let duplicate = (
        "crates/bn_lib_log/src/lib.rs",
        "pub fn dispatch_log() -> () { () }\n",
    );
    tree(&root, Some(duplicate));
    let output = gate(&root);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the gate passed on duplicate log dispatch"
    );
    assert!(
        stderr.contains("log dispatch logic outside bn_core_log: crates/bn_lib_log/src/lib.rs"),
        "{stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}
