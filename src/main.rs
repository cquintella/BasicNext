// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
mod cli_help;
mod process_log;
use cli_help::{help, usage};
// would this file load configurations if there is any?

use std::{
    env, fs,
    io::{self, BufRead, Read},
    path::{Path, PathBuf},
    process::ExitCode,
};

#[allow(unused_imports)]
use bn::diagnostic::{DiagId, Level};
use bn::{
    ast::Program,
    diagnostic::{Catalog, Diagnostic, WarningPolicy},
    ir::ValidatedModule,
    lexer::lex,
    llvm::{
        CompiledPolicy, Target as LlvmTarget, lower_validated_module_for_target_with_policy,
        validate_for,
    },
    module_graph::{ModuleGraph, load_with_session},
    runtime::{HostEnv, execute_validated_with_host},
    semantic::{ModuleAnalysisError, SemanticModel, analyze_modules_with_warnings},
    source::SourceFile,
    token::Token,
};
use process_log::{LogLevel, ProcessLog};
const VERSION: &str = concat!("bn ", env!("CARGO_PKG_VERSION"));

#[must_use]
fn language_error() -> ExitCode {
    ExitCode::from(1)
}

#[must_use]
fn tool_error() -> ExitCode {
    ExitCode::from(2)
}

fn render_diagnostic(
    diagnostic: &Diagnostic,
    source: &SourceFile,
    policy: &WarningPolicy,
) -> String {
    match Catalog::global_for_environment() {
        Ok(catalog) => diagnostic.render_with_catalog_and_policy(source, catalog, policy),
        Err(error) => format!(
            "{}\n\nerror: cannot load diagnostic catalog overlay: {error}",
            diagnostic.render(source)
        ),
    }
}

// can enum and structs be in another file?

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Emit {
    Tokens,
    Ast,
    TypedAst,
    Ir,
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Color {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Target {
    Native,
    Wasm32,
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // CLI flags map directly to independent policies.
struct Options {
    path: String,
    verbosity: u8,
    emit: Option<Emit>,
    output: Option<String>,
    trace: bool,
    color: Color,
    target: Target,
    filesystem: bool,
    sandbox: bool,
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    jupyter_stdin: bool,
    program_arguments: Vec<String>,
    optimization: Optimization,
    warning_policy: WarningPolicy,
    log_level: LogLevel,
    log_file: Option<String>,
    no_log: bool,
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

struct Frontend {
    graph: ModuleGraph,
    models: Vec<SemanticModel>,
    warnings: Vec<ModuleAnalysisError>,
    validated: ValidatedModule,
}

fn emit_frontend_warnings(frontend: &Frontend, source: &SourceFile, options: &Options) -> bool {
    let mut fatal = false;
    for warning in &frontend.warnings {
        let Some(id) = DiagId::from_code(warning.diagnostic.code) else {
            eprintln!(
                "{}",
                render_diagnostic(&warning.diagnostic, source, &options.warning_policy)
            );
            fatal = true;
            continue;
        };
        let level = options.warning_policy.level(id);
        let Some(module) = frontend.graph.modules.get(module_index(warning.module.0)) else {
            eprintln!("error: warning refers to a missing module");
            fatal = true;
            continue;
        };
        let rendered =
            render_diagnostic(&warning.diagnostic, &module.source, &options.warning_policy);
        if !rendered.is_empty() {
            eprintln!("{rendered}");
        }
        fatal |= level == Level::Error;
    }
    fatal
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

fn finish_process_log(log: &ProcessLog, options: &Options) -> bool {
    let Some(path) = process_log_path(options) else {
        return false;
    };
    if let Err(error) = log.write_to(&path) {
        eprintln!(
            "error[PROCESS_LOG_WRITE]: cannot write process log {}: {error}",
            path.display()
        );
        return true;
    }
    false
}

fn mirror_frontend_diagnostics(frontend: &Frontend, log: &mut ProcessLog) {
    for diagnostic in &frontend.warnings {
        log.record_warning();
        log.event(
            LogLevel::Warn,
            "diagnostic",
            "emit",
            format!(
                "code={} source={} line={} column={}",
                diagnostic.diagnostic.code,
                diagnostic.module.0,
                diagnostic.diagnostic.span.start.line,
                diagnostic.diagnostic.span.start.column
            ),
        );
    }
}

fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(command) = arguments.next() else {
        return usage();
    };
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
    let options = match parse_options(arguments) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("error: {message}");
            return usage();
        }
    };

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
            eprintln!(
                "{}",
                render_diagnostic(&diagnostic, &source, &options.warning_policy)
            );
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
        "build" => build(&source, &tokens, &options),
        _ => usage(),
    }
}

fn build(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    let mut process_log = ProcessLog::new(options.log_level);
    process_log.event(
        LogLevel::Info,
        "pipeline",
        "start",
        format!("target={:?} output={:?}", options.target, options.output),
    );
    let result = build_inner(source, tokens, options, &mut process_log);
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
    if finish_process_log(&process_log, options) {
        return tool_error();
    }
    result
}

#[allow(clippy::too_many_lines)] // Build stage events stay adjacent to their real transitions.
fn build_inner(
    source: &SourceFile,
    tokens: &[Token],
    options: &Options,
    process_log: &mut ProcessLog,
) -> ExitCode {
    process_log.event(
        LogLevel::Info,
        "frontend",
        "start",
        "load and analyze modules",
    );
    let frontend = match load_frontend(source, tokens, options) {
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
    mirror_frontend_diagnostics(&frontend, process_log);
    process_log.event(
        LogLevel::Debug,
        "config",
        "snapshot",
        format!(
            "target={:?} opt={:?} log_level={:?} no_log={}",
            options.target, options.optimization, options.log_level, options.no_log
        ),
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
    let llvm_target = if options.target == Target::Wasm32 {
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
        options.target == Target::Wasm32,
        &policy,
    ) {
        Ok(llvm) => {
            process_log.event(LogLevel::Info, "llvm_emit", "success", "LLVM emitted");
            process_log.event(LogLevel::Info, "link", "start", "write artifact");
            emit_build_output(llvm, options, process_log)
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
fn emit_build_output(llvm: String, options: &Options, process_log: &mut ProcessLog) -> ExitCode {
    let Some(output) = options.output.as_deref() else {
        return emit_output(llvm, None);
    };
    let temporary = env::temp_dir().join(format!("basicnext-llvm-{}.ll", std::process::id()));
    if let Err(error) = fs::write(&temporary, &llvm) {
        eprintln!("error: cannot write temporary LLVM IR: {error}");
        return tool_error();
    }
    let clang = match if options.target == Target::Wasm32 {
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
    let result = if options.target == Target::Wasm32 {
        process_log.event(
            LogLevel::Debug,
            "external",
            "invoke",
            format!(
                "tool=clang argv={:?}",
                [
                    options.optimization.clang_flag(),
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
                options.optimization.clang_flag(),
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
                            options.optimization.linker_flag(),
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
                        options.optimization.linker_flag(),
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
            options.optimization.clang_flag().to_string(),
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

fn run(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    let frontend = match load_frontend(source, tokens, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    if emit_frontend_warnings(&frontend, source, options) {
        return language_error();
    }
    log(options.verbosity, 1, "lowering typed BN IR");
    let module = &frontend.validated;
    if options.trace {
        log(
            options.verbosity.max(1),
            1,
            "executing IR entry point Start",
        );
    }
    if options.verbosity > 1 {
        print!("{}", tokens_text(tokens));
    }
    let executable = fs::canonicalize(&options.path)
        .map_or_else(|_| options.path.clone(), |path| path.display().to_string());
    let mut arguments = vec![executable];
    arguments.extend(options.program_arguments.iter().cloned());
    let mut host = if options.sandbox {
        match HostEnv::system(arguments.clone())
            .with_filesystem_roots(options.read_roots.clone(), options.write_roots.clone())
        {
            Ok(host) => host,
            Err(message) => {
                eprintln!("error: {message}");
                return tool_error();
            }
        }
    } else if options.filesystem {
        HostEnv::system(arguments)
    } else {
        HostEnv::system(arguments).without_filesystem()
    };
    match env::var("BN_FS_POLICY").as_deref() {
        Ok("deny") => host = host.without_filesystem(),
        Ok("read-only") => host = host.without_filesystem_writes(),
        Ok("") | Err(_) => {}
        Ok(value) => {
            eprintln!("error: invalid BN_FS_POLICY '{value}' (expected deny or read-only)");
            return tool_error();
        }
    }
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = JupyterInput {
        input: stdin.lock(),
        notify: options.jupyter_stdin,
    };
    match execute_validated_with_host(module, &mut input, &mut stdout.lock(), &host) {
        Ok(code) => ExitCode::from(code),
        Err(diagnostic) => {
            eprintln!(
                "{}",
                render_diagnostic(&diagnostic, source, &options.warning_policy)
            );
            if diagnostic.code == "EXECUTION_POLICY_DENIED" {
                tool_error()
            } else {
                language_error()
            }
        }
    }
}

struct JupyterInput<R> {
    input: R,
    notify: bool,
}

impl<R: Read> Read for JupyterInput<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.input.read(buffer)
    }
}

impl<R: BufRead> BufRead for JupyterInput<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.input.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.input.consume(amount);
    }

    fn read_line(&mut self, line: &mut String) -> io::Result<usize> {
        if self.notify {
            eprintln!("\u{001e}BN_INPUT_REQUEST");
        }
        self.input.read_line(line)
    }
}

fn check(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    if options.output.is_some() && options.emit.is_none() {
        eprintln!("error: -o requires --emit with bn check");
        return tool_error();
    }
    let frontend = match load_frontend(source, tokens, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    if emit_frontend_warnings(&frontend, source, options) {
        return language_error();
    }
    if options.verbosity > 1 {
        print!("{}", tokens_text(tokens));
    }
    if let Some(emit) = options.emit {
        let Some(semantic_model) = frontend.models.get(module_index(frontend.graph.root.0)) else {
            eprintln!("error: missing semantic model for the executable module");
            return tool_error();
        };
        let output = match emit {
            Emit::Tokens => tokens_text(tokens),
            Emit::Ast => format!("{:#?}\n", root_program(&frontend.graph)),
            Emit::TypedAst => format!(
                "{:#?}\n{semantic_model:#?}\n",
                root_program(&frontend.graph)
            ),
            Emit::Ir => format!("{:#?}\n", frontend.validated.as_module()),
        };
        if emit_output(output, options.output.as_deref()) != ExitCode::SUCCESS {
            return tool_error();
        }
    }
    if options.trace {
        log(
            options.verbosity.max(1),
            1,
            "check has no execution to trace",
        );
    }
    println!(
        "{}",
        colorize(
            &format!(
                "{}: lexical, syntax, and semantic checks passed",
                source.name
            ),
            options.color
        )
    );
    ExitCode::SUCCESS
}

fn root_program(graph: &ModuleGraph) -> &Program {
    graph
        .modules
        .iter()
        .find(|module| module.id == graph.root)
        .map(|module| &module.program)
        .expect("module graph always contains its root")
}

mod cli_frontend;
use cli_frontend::{load_frontend, parse_options};

fn tokens_text(tokens: &[Token]) -> String {
    tokens
        .iter()
        .map(|token| {
            format!(
                "{}:{}:{} {:?}",
                token.span.start.line, token.span.start.column, token.span.end.column, token.kind
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn emit_output(output: String, path: Option<&str>) -> ExitCode {
    if let Some(path) = path {
        return match fs::write(path, output) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: cannot write {path}: {error}");
                tool_error()
            }
        };
    }
    print!("{output}");
    ExitCode::SUCCESS
}

mod cli_output;
use cli_output::{colorize, log, module_index};

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
