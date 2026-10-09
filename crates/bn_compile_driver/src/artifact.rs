//! Artifact production: writes the emitted LLVM IR and drives clang /
//! wasm-ld to produce the native executable or Wasm module, linking
//! `libbn_rt.a` and the platform runtime libraries when HOST is used.

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
    options::{BuildOptions, Target},
    toolchain::{
        configured_bn_rt_lib, configured_clang, configured_wasm_clang, configured_wasm_ld,
    },
};

/// Flags for every native executable. On Windows the emitted IR carries no
/// target triple (clang would warn), links against the dynamic CRT that
/// `bn_rt.lib` is built for, and gets `printf` from
/// `legacy_stdio_definitions` (the UCRT defines it inline in its headers).
pub(crate) fn native_program_link_args() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &[
            "-Wno-override-module",
            "-fms-runtime-lib=dll",
            "-llegacy_stdio_definitions",
        ]
    }

    #[cfg(not(windows))]
    {
        &[]
    }
}

/// System libraries the `bn_rt` static library needs, as reported by
/// `rustc --print native-static-libs` for each target.
pub(crate) fn native_runtime_link_args() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &["-lm"]
    }

    #[cfg(windows)]
    {
        &[
            "-lbcrypt",
            "-ladvapi32",
            "-lkernel32",
            "-lntdll",
            "-luserenv",
            "-lws2_32",
            "-ldbghelp",
        ]
    }

    #[cfg(not(any(target_os = "linux", windows)))]
    {
        &[]
    }
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
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
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

/// The file the build writes: `-o`, or the entry's stem (`.exe` on Windows,
/// `.wasm` for wasm32); for `--emit llvm`, the `-o` file if any. The one
/// answer the artifact and its companion process log both use.
#[must_use]
pub fn product_path(options: &Options, build_options: BuildOptions) -> Option<std::path::PathBuf> {
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
    Some(std::path::PathBuf::from(
        if build_options.target == Target::Wasm32 {
            format!("{stem}.wasm")
        } else if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_string()
        },
    ))
}

#[allow(clippy::too_many_lines)] // External tool command construction stays auditable here.
pub fn emit_build_output(
    llvm: String,
    options: &Options,
    build_options: BuildOptions,
    process_log: &mut ProcessLog,
) -> ExitCode {
    if options.emit == Some(bn_cli::options::Emit::Llvm) {
        return emit_output(llvm, options.output.as_deref());
    }
    let Some(product) = product_path(options, build_options) else {
        return tool_error();
    };
    let output = product.to_string_lossy();
    let output = output.as_ref();
    if build_options.target == Target::Wasm32
        && build_options.cpu == crate::options::CpuTarget::Native
    {
        eprintln!(
            "{}",
            tool_diagnostic(
                DiagId::CONFIG_INVALID,
                "--cpu native is not supported when targeting wasm32",
                &options.diagnostic_catalog,
            )
        );
        return tool_error();
    }
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
    let clang = match if build_options.target == Target::Wasm32 {
        configured_wasm_clang()
    } else {
        configured_clang()
    } {
        Ok(clang) => clang,
        Err(message) => {
            eprintln!(
                "{}",
                tool_diagnostic(DiagId::CONFIG_INVALID, message, &options.diagnostic_catalog)
            );
            return tool_error();
        }
    };
    let object = build_dir.join("module.o");
    let mut failed_tool = "clang";
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
    let result = if build_options.target == Target::Wasm32 {
        let clang_args = [
            build_options.optimization.clang_flag(),
            "--target=wasm32-unknown-unknown",
            "-Wno-override-module",
            "-c",
            temporary.to_string_lossy().as_ref(),
            "-o",
            object.to_string_lossy().as_ref(),
        ]
        .into_iter()
        .map(ToString::to_string)
        .chain(debug_flag.map(ToString::to_string))
        .collect::<Vec<_>>();
        process_log.event(
            LogLevel::Debug,
            "external",
            "invoke",
            format!("tool=clang argv={clang_args:?}"),
        );
        let compiled = std::process::Command::new(clang).args(&clang_args).output();
        match compiled {
            Ok(compiled) if compiled.status.success() => {
                failed_tool = "wasm-ld";
                process_log.event(
                    LogLevel::Debug,
                    "external",
                    "invoke",
                    format!(
                        "tool=wasm-ld argv={:?}",
                        [
                            build_options.optimization.linker_flag(),
                            "--no-entry",
                            "--export=main",
                            "--export=__heap_base",
                            "--allow-undefined",
                            object.to_string_lossy().as_ref(),
                            "-o",
                            output,
                        ]
                    ),
                );
                std::process::Command::new(configured_wasm_ld())
                    .args([
                        build_options.optimization.linker_flag(),
                        "--no-entry",
                        "--export=main",
                        "--export=__heap_base",
                        "--allow-undefined",
                        object.to_string_lossy().as_ref(),
                        "-o",
                        output,
                    ])
                    .output()
            }
            compiled => compiled,
        }
    } else {
        let mut command = std::process::Command::new(clang);
        let mut command_args = vec![
            build_options.optimization.clang_flag().to_string(),
            temporary.to_string_lossy().into_owned(),
        ];
        command_args.extend(native_program_link_args().iter().map(ToString::to_string));
        command_args.extend(debug_flag.map(ToString::to_string));
        command_args.extend(cpu_flag.map(ToString::to_string));
        if llvm.contains("@bn_rt_") {
            let bn_rt = match configured_bn_rt_lib() {
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
            command_args.push(bn_rt.display().to_string());
            command_args.extend(native_runtime_link_args().iter().map(ToString::to_string));
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
