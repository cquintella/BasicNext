//! Artifact production: writes the emitted LLVM IR and drives clang to
//! produce the native executable, linking `libbn_rt.a` and the platform
//! runtime libraries when HOST is used.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use bn_diag::DiagId;

use bn_cli::{
    diagnostics::tool_diagnostic,
    options::Options,
    output::{emit_output, tool_error},
    process_log::{LogLevel, ProcessLog},
};

use crate::{
    options::BuildOptions,
    toolchain::{
        configured_bn_rt_lib, configured_bn_stdlib_lib, configured_bn_web_lib, configured_clang,
    },
};

fn uses_bn_stdlib(llvm: &str) -> bool {
    [
        "@bn_rt_crypto_",
        "@bn_rt_sqlite_",
        "@bn_rt_ffi_",
        "@bn_rt_json_",
        "@bn_rt_dataframe_",
        "@bn_rt_dispatch_",
        "@bn_rt_log_",
        "@bn_rt_math_",
        "@bn_web_",
    ]
    .iter()
    .any(|prefix| llvm.contains(prefix))
}

/// Flags for every native executable. On Windows the emitted IR carries no
/// target triple (clang would warn), links against the dynamic CRT that
/// `bn_rt.lib` is built for, and gets `printf` from
/// `legacy_stdio_definitions` (the UCRT defines it inline in its headers).
pub(crate) fn native_program_link_args_for_os(target_os: &str) -> &'static [&'static str] {
    if target_os == "windows" {
        &[
            "-Wno-override-module",
            "-fms-runtime-lib=dll",
            "-llegacy_stdio_definitions",
        ]
    } else {
        &[]
    }
}

pub(crate) fn native_program_link_args() -> &'static [&'static str] {
    native_program_link_args_for_os(std::env::consts::OS)
}

/// Link-time removal of runtime code the program never reaches (bucket
/// 0.6.5 E, D2: a program links only what it uses, so the runtime archive can
/// hold `BNWeb` without growing other programs). The MSVC linker already
/// applies `/OPT:REF` by default.
pub(crate) fn dead_code_link_args_for_os(target_os: &str) -> &'static [&'static str] {
    if target_os == "macos" {
        &["-Wl,-dead_strip"]
    } else if target_os == "windows" {
        &[]
    } else {
        &["-Wl,--gc-sections"]
    }
}

pub(crate) fn dead_code_link_args() -> &'static [&'static str] {
    dead_code_link_args_for_os(std::env::consts::OS)
}

/// System libraries the `bn_rt` static library needs, as reported by
/// `rustc --print native-static-libs` for each target.
pub(crate) fn native_runtime_link_args_for_os(target_os: &str) -> &'static [&'static str] {
    if target_os == "linux" {
        &["-lm"]
    } else if target_os == "windows" {
        &[
            "-lbcrypt",
            "-ladvapi32",
            "-lkernel32",
            "-lntdll",
            "-luserenv",
            "-lws2_32",
            "-ldbghelp",
        ]
    } else {
        &[]
    }
}

pub(crate) fn native_runtime_link_args() -> &'static [&'static str] {
    native_runtime_link_args_for_os(std::env::consts::OS)
}

/// Writes `bytes` to a file that must not exist yet (`create_new`: no
/// overwrite, no symlink followed).
pub(crate) fn write_new(path: &Path, bytes: &str) -> io::Result<()> {
    use std::io::Write as _;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes.as_bytes())
}

/// The private directory a native build keeps its LLVM IR and object file in
/// (known issue K24; Carlos, 2026-10-09: beside the source being compiled).
/// Created exclusively (`mkdir` fails on an existing name, a planted
/// symlink included), mode 0700 on Unix so nobody else can place a link
/// inside it, and removed with its contents when dropped, on every path.
pub(crate) struct BuildDir(PathBuf);

impl BuildDir {
    /// Creates `.<stem>.bnbuild-<pid>-<nanos>-<n>/` beside `source`.
    pub(crate) fn beside(source: &Path) -> io::Result<Self> {
        let parent = source
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let stem = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("a");
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.subsec_nanos());
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt as _;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        for attempt in 0..16 {
            let path = parent.join(format!(
                ".{stem}.bnbuild-{}-{nanos}-{attempt}",
                std::process::id()
            ));
            match builder.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free build directory name",
        ))
    }

    pub(crate) fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for BuildDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The file the build writes: `-o`, or the entry's stem (`.exe` on Windows);
/// for `--emit llvm`, the `-o` file if any. The one answer the artifact and
/// its companion process log both use.
#[must_use]
pub(crate) fn product_path(options: &Options) -> Option<std::path::PathBuf> {
    if let Some(path) = options.output.as_deref() {
        return Some(std::path::PathBuf::from(path));
    }
    if options.emit.is_some() {
        return None;
    }
    let stem = std::path::Path::new(&options.path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("a");
    Some(std::path::PathBuf::from(if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.to_string()
    }))
}

#[allow(clippy::too_many_lines)] // External tool command construction stays auditable here.
pub(crate) fn emit_build_output(
    llvm: String,
    options: &Options,
    build_options: BuildOptions,
    process_log: &mut ProcessLog,
) -> ExitCode {
    if options.emit == Some(bn_cli::options::Emit::Llvm) {
        return emit_output(llvm, options.output.as_deref());
    }
    let Some(product) = product_path(options) else {
        return tool_error();
    };
    let output = product.to_string_lossy();
    let output = output.as_ref();
    // Beside the source (Carlos, 2026-10-09); a read-only source tree falls
    // back to the system temporary directory, where the same exclusive,
    // private creation is equally safe.
    let source = Path::new(&options.path);
    let build_dir = match BuildDir::beside(source).or_else(|_| {
        BuildDir::beside(&std::env::temp_dir().join(source.file_name().unwrap_or_default()))
    }) {
        Ok(build_dir) => build_dir,
        Err(error) => {
            eprintln!("error: cannot create a private build directory: {error}");
            return tool_error();
        }
    };
    let temporary = build_dir.join("module.ll");
    if let Err(error) = write_new(&temporary, &llvm) {
        eprintln!("error: cannot write temporary LLVM IR: {error}");
        return tool_error();
    }
    let clang = match configured_clang() {
        Ok(clang) => clang,
        Err(message) => {
            eprintln!(
                "{}",
                tool_diagnostic(DiagId::CONFIG_INVALID, message, &options.diagnostic_catalog)
            );
            return tool_error();
        }
    };
    let failed_tool = "clang";
    // `-g` keeps the debug sections the module metadata describes; on macOS
    // it also makes clang run dsymutil, which writes `<output>.dSYM`.
    let debug_flag = build_options.debug.then_some("-g");
    let cpu_flag = match build_options.cpu {
        crate::options::CpuTarget::Generic => None,
        crate::options::CpuTarget::Native => {
            if cfg!(target_arch = "x86_64") {
                Some("-march=native")
            } else {
                Some("-mcpu=native")
            }
        }
    };
    let result = {
        let mut command = std::process::Command::new(clang);
        let mut command_args = vec![
            build_options.optimization.clang_flag().to_string(),
            temporary.to_string_lossy().into_owned(),
        ];
        command_args.extend(native_program_link_args().iter().map(ToString::to_string));
        command_args.extend(debug_flag.map(ToString::to_string));
        command_args.extend(cpu_flag.map(ToString::to_string));
        let mut static_libs = Vec::new();
        if llvm.contains("@bn_web_") {
            let bn_web = match configured_bn_web_lib() {
                Ok(path) => path,
                Err(message) => {
                    eprintln!(
                        "{}",
                        tool_diagnostic(
                            DiagId::BUILD_TOOLCHAIN_UNAVAILABLE,
                            message,
                            &options.diagnostic_catalog,
                        )
                    );
                    return tool_error();
                }
            };
            static_libs.push(bn_web.display().to_string());
        }
        if llvm.contains("@bn_rt_") || uses_bn_stdlib(&llvm) {
            let runtime_archive = if uses_bn_stdlib(&llvm) {
                configured_bn_stdlib_lib()
            } else {
                configured_bn_rt_lib()
            };
            let bn_rt = match runtime_archive {
                Ok(path) => path,
                Err(message) => {
                    eprintln!(
                        "{}",
                        tool_diagnostic(
                            DiagId::BUILD_TOOLCHAIN_UNAVAILABLE,
                            message,
                            &options.diagnostic_catalog,
                        )
                    );
                    return tool_error();
                }
            };
            static_libs.push(bn_rt.display().to_string());
        }

        if !static_libs.is_empty() {
            #[cfg(target_os = "linux")]
            let use_group = static_libs.len() > 1;
            #[cfg(not(target_os = "linux"))]
            let use_group = false;

            if use_group {
                command_args.push("-Wl,--start-group".into());
            }
            command_args.extend(static_libs);
            if use_group {
                command_args.push("-Wl,--end-group".into());
            }

            command_args.extend(native_runtime_link_args().iter().map(ToString::to_string));
            // A `-g` build keeps the whole runtime: the debugger calls `bn_rt`
            // functions the program never reaches (`bn_rt_arc_dump`).
            if debug_flag.is_none() {
                command_args.extend(dead_code_link_args().iter().map(ToString::to_string));
            }
            #[cfg(target_os = "linux")]
            if llvm.contains("@bn_rt_ffi_") {
                command_args.push("-ldl".to_string());
            }
        }
        if llvm.contains("@bn_web_") {
            #[cfg(target_os = "macos")]
            {
                command_args.extend([
                    "-framework".into(),
                    "Security".into(),
                    "-framework".into(),
                    "CoreFoundation".into(),
                    "-framework".into(),
                    "SystemConfiguration".into(),
                ]);
            }
            #[cfg(target_os = "linux")]
            {
                command_args.extend(["-lpthread".into(), "-ldl".into()]);
            }
            #[cfg(windows)]
            {
                command_args.extend(["-lsecur32".into(), "-lcrypt32".into()]);
            }
        }
        command_args.extend(["-o".into(), output.into()]);
        process_log.event(
            LogLevel::Debug,
            "external",
            "invoke",
            format!("tool=clang argv={command_args:?}"),
        );
        command.args(&command_args).output()
    };
    match result {
        Ok(result) if result.status.success() => ExitCode::SUCCESS,
        Ok(result) => {
            eprintln!(
                "{}",
                tool_diagnostic(
                    DiagId::BUILD_EMISSION_FAILED,
                    String::from_utf8_lossy(&result.stderr).trim(),
                    &options.diagnostic_catalog,
                )
            );
            tool_error()
        }
        Err(error) => {
            eprintln!(
                "{}",
                tool_diagnostic(
                    DiagId::BUILD_TOOLCHAIN_UNAVAILABLE,
                    format!("cannot execute {failed_tool}: {error}"),
                    &options.diagnostic_catalog,
                )
            );
            tool_error()
        }
    }
}
