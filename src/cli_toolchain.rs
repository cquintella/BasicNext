use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub(crate) fn configured_wasm_ld() -> String {
    if let Ok(command) = env::var("BN_WASM_LD") {
        return command;
    }
    if let Ok(Some(command)) = toolchain_value("wasm-ld") {
        return command;
    }
    if command_succeeds("wasm-ld", &["--version"]) {
        return "wasm-ld".into();
    }
    if let Some(command) = brew_bin("lld@20", "wasm-ld").or_else(|| brew_bin("lld", "wasm-ld")) {
        return command;
    }
    "wasm-ld".into()
}

pub(crate) fn configured_wasm_clang() -> Result<String, String> {
    if let Ok(command) = env::var("BN_WASM_CLANG") {
        return Ok(command);
    }
    if let Some(command) = toolchain_value("wasm-clang")? {
        return Ok(command);
    }
    let default = configured_clang()?;
    if clang_has_wasm32(&default) {
        return Ok(default);
    }
    if let Some(command) = brew_bin("llvm", "clang")
        && clang_has_wasm32(&command)
    {
        return Ok(command);
    }
    if let Some(parent) = Path::new(&configured_wasm_ld()).parent() {
        let sibling = parent.join("clang");
        if sibling.is_file()
            && let Some(command) = sibling.to_str()
            && clang_has_wasm32(command)
        {
            return Ok(command.into());
        }
    }
    Ok(default)
}

pub(crate) fn configured_clang() -> Result<String, String> {
    Ok(toolchain_value("clang")?.unwrap_or_else(|| "clang".into()))
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
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(file_name));
        if let Some(parent) = dir.parent() {
            candidates.push(parent.join(file_name));
            candidates.push(parent.join("lib").join(file_name));
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        candidates.push(manifest.join("target").join(profile).join(file_name));
    }
    if let Ok(dir) = env::var("CARGO_TARGET_DIR") {
        for profile in ["debug", "release"] {
            candidates.push(PathBuf::from(&dir).join(profile).join(file_name));
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "libbn_rt.a not found; build the bn_rt crate (cargo build -p bn_rt)".into())
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

pub(crate) fn brew_bin(formula: &str, binary: &str) -> Option<String> {
    let output = std::process::Command::new("brew")
        .args(["--prefix", formula])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let command = Path::new(String::from_utf8_lossy(&output.stdout).trim())
        .join("bin")
        .join(binary);
    command
        .is_file()
        .then(|| command.to_string_lossy().into_owned())
}

pub(crate) fn clang_has_wasm32(clang: &str) -> bool {
    let Ok(output) = std::process::Command::new(clang)
        .arg("-print-targets")
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().next() == Some("wasm32"))
}

pub(crate) fn command_succeeds(command: &str, args: &[&str]) -> bool {
    std::process::Command::new(command)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}
