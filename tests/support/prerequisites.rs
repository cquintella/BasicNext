// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Stale test prerequisites fail loudly. `cargo test` rebuilds neither
//! `bni`/`bnc` under `target/` nor the runtime archives native builds link,
//! so a test could pass or fail on binaries older than the tree. The build
//! script writes a stamp of the tree it built from; a test that resolves a
//! prerequisite compares it with the stamp of the tree now, computed by the
//! same script (one algorithm). Standalone (std only), so crate tests that
//! cannot use `tests/support` include it by path.

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

pub const BUILD_SCRIPT: &str = "scripts/build-test-prereqs.sh";
pub const STAMP_FILE: &str = "test-prereqs.stamp";
pub const STALE_MESSAGE: &str =
    "test prerequisites are stale or missing: run scripts/build-test-prereqs.sh";

/// With every one of these set to an existing file, the caller chose the
/// prerequisites and no stamp is required (`scripts/test.sh` and the battery
/// set them all).
pub const OVERRIDES: [&str; 5] = [
    "BN_INTERPRETER",
    "BN_COMPILER",
    "BN_RT_LIB",
    "BN_STDLIB_LIB",
    "BN_WEB_LIB",
];

pub fn overrides_complete(lookup: impl Fn(&str) -> Option<OsString>) -> bool {
    OVERRIDES
        .iter()
        .all(|name| lookup(name).is_some_and(|path| Path::new(&path).is_file()))
}

/// `target/<profile>/` of the workspace, following `CARGO_TARGET_DIR`.
pub fn profile_dir(workspace: &Path) -> PathBuf {
    let target =
        env::var_os("CARGO_TARGET_DIR").map_or_else(|| workspace.join("target"), PathBuf::from);
    let target = if target.is_absolute() {
        target
    } else {
        workspace.join(target)
    };
    target.join(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    })
}

/// The stamp of the tree under `workspace` now, from the build script.
pub fn current_stamp(workspace: &Path) -> Result<String, String> {
    // An explicit PATH makes Windows search it before System32, whose
    // `bash.exe` is the WSL launcher, not the Git Bash that runs CI.
    let output = Command::new("bash")
        .env("PATH", env::var_os("PATH").unwrap_or_default())
        .current_dir(workspace)
        .args([BUILD_SCRIPT, "--print-stamp"])
        .output()
        .map_err(|error| format!("run {BUILD_SCRIPT} --print-stamp: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{BUILD_SCRIPT} --print-stamp failed (a source tree without git needs every \
             override: {}):\n{}",
            OVERRIDES.join(", "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// `Ok` when the stamp the last build wrote in `profile` matches the tree.
pub fn check(workspace: &Path, profile: &Path) -> Result<(), String> {
    let stamp_path = profile.join(STAMP_FILE);
    let built = fs::read_to_string(&stamp_path)
        .map_err(|error| format!("{STALE_MESSAGE} ({}: {error})", stamp_path.display()))?;
    let current = current_stamp(workspace)?;
    if built.trim() == current {
        Ok(())
    } else {
        Err(format!(
            "{STALE_MESSAGE} (built from tree {}, tree now {current})",
            built.trim()
        ))
    }
}

/// `bni` or `bnc`: its override, else the workspace build, checked fresh.
pub fn executable(workspace: &Path, name: &str) -> PathBuf {
    ensure_fresh(workspace);
    let override_name = match name {
        "bni" => "BN_INTERPRETER",
        "bnc" => "BN_COMPILER",
        _ => panic!("not a test prerequisite executable: {name}"),
    };
    env::var_os(override_name).map_or_else(
        || profile_dir(workspace).join(format!("{name}{}", env::consts::EXE_SUFFIX)),
        PathBuf::from,
    )
}

/// A runtime archive (`bn_rt`, `bn_stdlib`, `bn_web`): its override, else
/// the workspace build, checked fresh.
pub fn archive(workspace: &Path, library: &str) -> PathBuf {
    ensure_fresh(workspace);
    let override_name = match library {
        "bn_rt" => "BN_RT_LIB",
        "bn_stdlib" => "BN_STDLIB_LIB",
        "bn_web" => "BN_WEB_LIB",
        _ => panic!("not a runtime archive: {library}"),
    };
    let file_name = if cfg!(windows) {
        format!("{library}.lib")
    } else {
        format!("lib{library}.a")
    };
    env::var_os(override_name).map_or_else(|| profile_dir(workspace).join(file_name), PathBuf::from)
}

/// Checks once per test process; panics on a stale or missing build.
pub fn ensure_fresh(workspace: &Path) {
    static CHECKED: OnceLock<()> = OnceLock::new();
    CHECKED.get_or_init(|| {
        if overrides_complete(|name| env::var_os(name)) {
            return;
        }
        if let Err(message) = check(workspace, &profile_dir(workspace)) {
            panic!("{message}");
        }
    });
}
