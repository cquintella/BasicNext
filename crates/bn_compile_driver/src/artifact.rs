//! Artifact production: writes the emitted LLVM IR and drives clang /
//! wasm-ld to produce the native executable or Wasm module, linking
//! `libbn_rt.a` and the platform runtime libraries when HOST is used.

use std::{env, fs, path::PathBuf, process::ExitCode};

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

pub(crate) struct TempFileGuard(pub(crate) PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
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
    let default_output;
    let output = if let Some(path) = options.output.as_deref() {
        path
    } else {
        let stem = std::path::Path::new(&options.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("a");
        default_output = if build_options.target == Target::Wasm32 {
            format!("{stem}.wasm")
        } else if cfg!(windows) {
            format!("{stem}.exe")
        } else {
            stem.to_string()
        };
        &default_output
    };
    let temporary = env::temp_dir().join(format!("basicnext-llvm-{}.ll", std::process::id()));
    if let Err(error) = fs::write(&temporary, &llvm) {
        eprintln!("error: cannot write temporary LLVM IR: {error}");
        return tool_error();
    }
    let _temporary_guard = TempFileGuard(temporary.clone());
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
    let object = temporary.with_extension("o");
    let _object_guard = TempFileGuard(object.clone());
    let mut failed_tool = "clang";
    // `-g` keeps the debug sections the module metadata describes; on macOS
    // it also makes clang run dsymutil, which writes `<output>.dSYM`.
    let debug_flag = build_options.debug.then_some("-g");
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
