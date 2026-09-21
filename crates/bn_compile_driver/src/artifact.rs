//! Artifact production: writes the emitted LLVM IR and drives clang /
//! wasm-ld to produce the native executable or Wasm module, linking
//! `libbn_rt.a` and the platform runtime libraries when HOST is used.

use std::{env, fs, process::ExitCode};

use bn_cli::{
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

pub(crate) fn native_runtime_link_args() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &["-lm"]
    }

    #[cfg(not(target_os = "linux"))]
    {
        &[]
    }
}

#[allow(clippy::too_many_lines)] // External tool command construction stays auditable here.
pub fn emit_build_output(
    llvm: String,
    options: &Options,
    build_options: BuildOptions,
    process_log: &mut ProcessLog,
) -> ExitCode {
    let Some(output) = options.output.as_deref() else {
        return emit_output(llvm, None);
    };
    let temporary = env::temp_dir().join(format!("basicnext-llvm-{}.ll", std::process::id()));
    if let Err(error) = fs::write(&temporary, &llvm) {
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
            eprintln!("error[CONFIG_INVALID]: {message}");
            return tool_error();
        }
    };
    let object = temporary.with_extension("o");
    let mut failed_tool = "clang";
    let result = if build_options.target == Target::Wasm32 {
        process_log.event(
            LogLevel::Debug,
            "external",
            "invoke",
            format!(
                "tool=clang argv={:?}",
                [
                    build_options.optimization.clang_flag(),
                    "--target=wasm32-unknown-unknown",
                    "-Wno-override-module",
                    "-c",
                    temporary.to_string_lossy().as_ref(),
                    "-o",
                    object.to_string_lossy().as_ref(),
                ]
            ),
        );
        let compiled = std::process::Command::new(clang)
            .args([
                build_options.optimization.clang_flag(),
                "--target=wasm32-unknown-unknown",
                "-Wno-override-module",
                "-c",
                temporary.to_string_lossy().as_ref(),
                "-o",
                object.to_string_lossy().as_ref(),
            ])
            .output();
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
        if llvm.contains("@bn_rt_") {
            let bn_rt = match configured_bn_rt_lib() {
                Ok(path) => path,
                Err(message) => {
                    eprintln!("error[BUILD_TOOLCHAIN_UNAVAILABLE]: {message}");
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
    let _ = fs::remove_file(temporary);
    let _ = fs::remove_file(object);
    match result {
        Ok(result) if result.status.success() => ExitCode::SUCCESS,
        Ok(result) => {
            eprintln!(
                "error[BUILD_EMISSION_FAILED]: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            );
            tool_error()
        }
        Err(error) => {
            eprintln!("error[BUILD_TOOLCHAIN_UNAVAILABLE]: cannot execute {failed_tool}: {error}");
            tool_error()
        }
    }
}
