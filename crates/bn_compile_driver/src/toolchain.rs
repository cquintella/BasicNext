//! Discovery of the external compiler toolchain: clang and `libbn_rt.a`,
//! from `BN_*` environment variables, the toolchain configuration file and
//! PATH.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub(crate) fn configured_clang() -> Result<String, String> {
    Ok(toolchain_value("clang")?.unwrap_or_else(|| "clang".into()))
}

fn find_static_lib(file_name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(file_name));
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join(file_name));
            candidates.push(parent.join("lib").join(file_name));
            // Optional layout: keep BN-owned libs under share/bn/lib (arch-specific
            // .a still belongs in lib/ for FHS; this path is a fallback).
            candidates.push(parent.join("share").join("bn").join("lib").join(file_name));
        }
    }
    // Workspace target directory (this crate lives two levels below the root).
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for profile in ["debug", "release"] {
        candidates.push(workspace.join("target").join(profile).join(file_name));
    }
    if let Ok(dir) = env::var(bn_catalog::text("cli", "msg.cargo_target_dir")) {
        for profile in ["debug", "release"] {
            candidates.push(PathBuf::from(&dir).join(profile).join(file_name));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

pub(crate) fn configured_bn_rt_lib() -> Result<PathBuf, String> {
    if let Ok(path) = env::var("BN_RT_LIB") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!("BN_RT_LIB is not a file: {}", path.display()));
    }
    let file_name = if cfg!(windows) {
        "bn_rt.lib"
    } else {
        "libbn_rt.a"
    };
    find_static_lib(file_name).ok_or_else(|| "libbn_rt.a not found; install it under $PREFIX/lib (scripts/install.sh) or build the bn_rt crate (cargo build -p bn_rt) / set BN_RT_LIB".into())
}

pub(crate) fn configured_bn_stdlib_lib() -> Result<PathBuf, String> {
    if let Ok(path) = env::var(bn_catalog::text("cli", "msg.bn_stdlib_lib")) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(bn_catalog::fill(
            "cli",
            "msg.bn_stdlib_lib_not_a_file",
            &[("path", &path.display())],
        ));
    }
    let file_name = if cfg!(windows) {
        "bn_stdlib.lib"
    } else {
        "libbn_stdlib.a"
    };
    find_static_lib(file_name)
        .ok_or_else(|| bn_catalog::text("cli", "msg.libbn_stdlib_a_not_found_build_the_bn").into())
}

pub(crate) fn configured_bn_web_lib() -> Result<PathBuf, String> {
    if let Ok(path) = env::var(bn_catalog::text("cli", "msg.bn_web_lib")) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(bn_catalog::fill(
            "cli",
            "msg.bn_web_lib_not_a_file",
            &[("path", &path.display())],
        ));
    }
    let file_name = if cfg!(windows) {
        "bn_web.lib"
    } else {
        "libbn_web.a"
    };
    find_static_lib(file_name)
        .ok_or_else(|| bn_catalog::text("cli", "msg.libbn_web_a_not_found_build_the_bn").into())
}

pub(crate) fn toolchain_value(key: &str) -> Result<Option<String>, String> {
    let Ok(config) = fs::read_to_string("config.toml") else {
        return Ok(None);
    };
    let prefix = format!("{key} = ");
    let mut in_toolchain = false;
    for line in config.lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_toolchain = line == "[toolchain]";
            continue;
        }
        if !in_toolchain {
            continue;
        }
        let Some(value) = line.strip_prefix(&prefix) else {
            continue;
        };
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        else {
            return Err(format!("toolchain.{key} must be a quoted command"));
        };
        if value.is_empty() {
            return Err(format!("toolchain.{key} must not be empty"));
        }
        return Ok(Some(value.into()));
    }
    Ok(None)
}
