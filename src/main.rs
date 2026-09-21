// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bn`: the combined driver (eval | check | lex | run | build | lsp | dap).
//! Common CLI services come from `bn_cli`, interpretation from
//! `bn_interpret_driver`; this file keeps build (compilation) until its
//! driver is extracted (bucket 0.6.0, activity 1.3).
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use bn::{
    lexer::lex,
    llvm::{
        CompiledPolicy, Target as LlvmTarget, lower_validated_module_for_target_with_policy,
        validate_for,
    },
    source::SourceFile,
};
use bn_cli::{
    check::check,
    diagnostics::{emit_frontend_warnings, render_diagnostic},
    frontend::load_frontend,
    help::COMMON_OPTIONS,
    options::{OptionExtension, Options, OutputFormat, parse_options},
    output::{emit_output, language_error, log, tokens_text, tool_error},
    process_log::{LogLevel, ProcessLog},
};
use bn_interpret_driver::{eval::eval, run::run};
const VERSION: &str = concat!("bn ", env!("CARGO_PKG_VERSION"));

/// Compile-only flags (`--target`, `--opt`); accepted by every `bn` command
/// for compatibility, consumed by `build`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BuildOptions {
    target: Target,
    optimization: Optimization,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            target: Target::Native,
            optimization: Optimization::Level(2),
        }
    }
}

impl OptionExtension for BuildOptions {
    fn accept(
        &mut self,
        argument: &str,
        rest: &mut dyn Iterator<Item = String>,
    ) -> Result<bool, String> {
        match argument {
            "--opt" => {
                self.optimization = match rest.next().as_deref() {
                    Some("none") => Optimization::None,
                    Some("1") => Optimization::Level(1),
                    Some("2") => Optimization::Level(2),
                    Some("3") => Optimization::Level(3),
                    Some("s") => Optimization::Size,
                    _ => return Err("--opt expects none, 1, 2, 3, or s".into()),
                };
            }
            "--target" => {
                self.target = match rest.next().as_deref() {
                    Some("native") => Target::Native,
                    Some("wasm32") => Target::Wasm32,
                    _ => return Err("--target expects native or wasm32".into()),
                };
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

fn help() -> ExitCode {
    println!(
        "\
{VERSION}
usage: bn <eval|check|lex|run|build|lsp|dap> [options] <file.bn> [-- program-args]

commands:
  eval    evaluate one source fragment (SOURCE or --stdin) through the interpreter
  check   validate lexer, parser, and semantics
  lex     print the token stream
  run     execute FUNCTION Start through typed BN IR
  build   compile the supported typed BN IR subset with LLVM
  lsp     serve Language Server Protocol over stdio
  dap     serve Debug Adapter Protocol over stdio

options:
  --mode snippet|program       select eval fragment mode (eval only)
  --format text|json           select eval result format (eval only)
  --target native|wasm32     select the build target (build only)
  --opt none|1|2|3|s         optimization level for native/Wasm builds (default 2)
{COMMON_OPTIONS}
 `bn eval` accepts SOURCE or --stdin; extra program arguments follow --.
 For file-oriented commands, HOST.Args[0] is the source path. Extra program arguments follow --.
See also: man bn
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    bn_cli::help::usage(
        "usage: bn <eval|check|lex|run|build|lsp|dap> [options] <file.bn>\ntry: bn --help",
    )
}

fn native_runtime_link_args() -> &'static [&'static str] {
    #[cfg(target_os = "linux")]
    {
        &["-lm"]
    }

    #[cfg(not(target_os = "linux"))]
    {
        &[]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Native,
    Wasm32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Optimization {
    None,
    Level(u8),
    Size,
}

impl Optimization {
    fn clang_flag(self) -> &'static str {
        match self {
            Self::None => "-O0",
            Self::Level(level) => match level {
                1 => "-O1",
                3 => "-O3",
                _ => "-O2",
            },
            Self::Size => "-Oz",
        }
    }

    fn linker_flag(self) -> &'static str {
        match self {
            Self::None => "-O0",
            Self::Level(level) => match level {
                1 => "-O1",
                3 => "-O3",
                _ => "-O2",
            },
            Self::Size => "-O2",
        }
    }
}

fn process_log_path(options: &Options) -> Option<PathBuf> {
    if options.no_log {
        return None;
    }
    options.log_file.as_ref().map_or_else(
        || {
            options
                .output
                .as_ref()
                .map(|output| Path::new(output).with_extension("log"))
        },
        |path| Some(PathBuf::from(path)),
    )
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage();
    };
    if command == "eval" {
        return eval(arguments.collect(), &mut BuildOptions::default());
    }
    match command.as_str() {
        "-h" | "--help" => return help(),
        "-V" | "--version" => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        _ => {} // if there is any extra arg, do nothing
    }

    if command == "lsp" {
        // Language Server Protocol
        return match bn::lsp::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error[LSP]: {message}");
                tool_error()
            }
        };
    }

    if command == "dap" {
        // Debug Adapter Protocol
        return match bn::dap::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error[DAP]: {message}");
                tool_error()
            }
        };
    }
    let mut build_options = BuildOptions::default();
    let options = match parse_options(arguments, &mut build_options) {
        Ok(options) => options,
        Err(message) => {
            if let Some(message) = message.strip_prefix("CONFIG_INVALID: ") {
                eprintln!("error[CONFIG_INVALID]: {message}");
            } else {
                eprintln!("error: {message}");
            }
            return usage();
        }
    };
    if options.output_format != OutputFormat::Text {
        eprintln!("error: --format is available only with bn eval");
        return tool_error();
    }

    log(options.verbosity, 1, format!("reading {}", options.path));
    let text = match fs::read_to_string(&options.path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", options.path);
            return tool_error();
        }
    };
    let source = SourceFile::new(&options.path, text);
    log(options.verbosity, 1, "lexical analysis");
    let tokens = match lex(&source) {
        Ok(tokens) => tokens,
        Err(diagnostic) => {
            eprintln!("{}", render_diagnostic(&diagnostic, &source, &options));
            return language_error();
        }
    };
    log(
        options.verbosity,
        1,
        format!("lexer completed: {} tokens", tokens.len()),
    );
    match command.as_str() {
        "lex" => emit_output(tokens_text(&tokens), options.output.as_deref()),
        "check" => check(&source, &tokens, &options),
        "run" => run(&source, &tokens, &options),
        "build" => build(&source, &options, build_options),
        _ => usage(),
    }
}

fn build(source: &SourceFile, options: &Options, build_options: BuildOptions) -> ExitCode {
    let mut process_log = ProcessLog::new(options.log_level);
    process_log.event(
        LogLevel::Info,
        "pipeline",
        "start",
        format!(
            "target={:?} output={:?}",
            build_options.target, options.output
        ),
    );
    let result = build_inner(source, options, build_options, &mut process_log);
    process_log.event(
        if result == ExitCode::SUCCESS {
            LogLevel::Warn
        } else {
            LogLevel::Error
        },
        "diagnostic",
        "summary",
        format!(
            "errors={} warnings={} exit={result:?}",
            usize::from(result != ExitCode::SUCCESS),
            process_log.warning_count()
        ),
    );
    process_log.event(
        if result == ExitCode::SUCCESS {
            LogLevel::Info
        } else {
            LogLevel::Error
        },
        "pipeline",
        "end",
        format!("exit={result:?}"),
    );
    if process_log.finish(process_log_path(options).as_deref()) {
        return tool_error();
    }
    result
}

#[allow(clippy::too_many_lines)] // Build stage events stay adjacent to their real transitions.
fn build_inner(
    source: &SourceFile,
    options: &Options,
    build_options: BuildOptions,
    process_log: &mut ProcessLog,
) -> ExitCode {
    process_log.event(
        LogLevel::Info,
        "frontend",
        "start",
        "load and analyze modules",
    );
    let frontend = match load_frontend(source, options) {
        Ok(frontend) => frontend,
        Err(code) => {
            process_log.event(
                LogLevel::Error,
                "frontend",
                "fail",
                format!("exit={code:?}"),
            );
            return code;
        }
    };
    process_log.event(
        LogLevel::Info,
        "frontend",
        "success",
        "semantic analysis complete",
    );
    process_log.mirror_frontend_diagnostics(&frontend);
    let roots = frontend
        .graph
        .roots
        .iter()
        .map(|root| format!("{} ({})", root.path.display(), root.provenance.label()))
        .collect::<Vec<_>>();
    process_log.event(
        LogLevel::Debug,
        "config",
        "snapshot",
        format!(
            "target={:?} opt={:?} log_level={:?} no_log={} bn_home={} module_roots={roots:?}",
            build_options.target,
            build_options.optimization,
            options.log_level,
            options.no_log,
            std::env::var_os("BN_HOME").is_some(),
        ),
    );
    let resolved = frontend
        .graph
        .modules
        .iter()
        .filter(|module| module.id != frontend.graph.root)
        .map(|module| {
            let origin = module.root.as_ref().map_or_else(
                || "unresolved".to_string(),
                |root| format!("{} ({})", root.path.display(), root.provenance.label()),
            );
            format!("{} <- {origin}", module.source.name)
        })
        .collect::<Vec<_>>();
    process_log.event(
        LogLevel::Debug,
        "frontend",
        "module-roots",
        format!("resolved={resolved:?}"),
    );
    process_log.event(
        LogLevel::Debug,
        "frontend",
        "modules",
        format!(
            "count={} names={:?}",
            frontend.graph.modules.len(),
            frontend
                .graph
                .modules
                .iter()
                .map(|module| module.source.name.as_str())
                .collect::<Vec<_>>()
        ),
    );
    if emit_frontend_warnings(&frontend, source, options) {
        process_log.event(
            LogLevel::Error,
            "frontend",
            "fail",
            "warning promoted to error",
        );
        return language_error();
    }
    process_log.event(LogLevel::Info, "lower", "start", "lower and validate IR");
    let module = &frontend.validated;
    process_log.event(LogLevel::Info, "lower", "success", "validated IR ready");
    let llvm_target = if build_options.target == Target::Wasm32 {
        LlvmTarget::Wasm32
    } else {
        LlvmTarget::Native
    };
    if let Err(error) = validate_for(module, llvm_target) {
        process_log.event(
            LogLevel::Error,
            "validate_for",
            "fail",
            format!(
                "code={} source={} line={} column={}",
                error.code,
                error.span.start.source_id.0,
                error.span.start.line,
                error.span.start.column
            ),
        );
        eprintln!("error[{}]: {}", error.code, error.message);
        return tool_error();
    }
    process_log.event(
        LogLevel::Info,
        "validate_for",
        "success",
        "target support accepted",
    );
    process_log.event(LogLevel::Info, "llvm_emit", "start", "emit target LLVM");
    let policy = CompiledPolicy {
        sandboxed: options.sandbox,
        read_roots: options
            .read_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        write_roots: options
            .write_roots
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
    };
    let result = match lower_validated_module_for_target_with_policy(
        module,
        build_options.target == Target::Wasm32,
        &policy,
    ) {
        Ok(llvm) => {
            process_log.event(LogLevel::Info, "llvm_emit", "success", "LLVM emitted");
            process_log.event(LogLevel::Info, "link", "start", "write artifact");
            emit_build_output(llvm, options, build_options, process_log)
        }
        Err(message) => {
            process_log.event(
                LogLevel::Error,
                "llvm_emit",
                "fail",
                format!("error={message}"),
            );
            eprintln!("error[{message}]");
            tool_error()
        }
    };
    process_log.event(
        if result == ExitCode::SUCCESS {
            LogLevel::Info
        } else {
            LogLevel::Error
        },
        "link",
        if result == ExitCode::SUCCESS {
            "success"
        } else {
            "fail"
        },
        format!("exit={result:?}"),
    );
    result
}

#[allow(clippy::too_many_lines)] // External tool command construction stays auditable here.
fn emit_build_output(
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

mod cli_toolchain;
use cli_toolchain::{
    configured_bn_rt_lib, configured_clang, configured_wasm_clang, configured_wasm_ld,
};

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
