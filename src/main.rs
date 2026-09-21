// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
//! `bn`: the combined driver (eval | check | lex | run | build | lsp | dap).
//! Common CLI services come from `bn_cli`; this file keeps eval/run
//! (interpretation) and build (compilation) until their drivers are extracted
//! (bucket 0.6.0, activities 1.2a and 1.3).
use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, Read},
    path::{Path, PathBuf},
    process::ExitCode,
};

use bn::diagnostic::{DiagId, Level};
use bn::{
    ast::{DeclarationKind, Item},
    diagnostic::{Diagnostic, DiagnosticValue, Label, LabelStyle},
    lexer::lex,
    llvm::{
        CompiledPolicy, Target as LlvmTarget, lower_validated_module_for_target_with_policy,
        validate_for,
    },
    runtime::{HostEnv, HostEnvDefaults, execute_validated_with_host},
    source::SourceFile,
    token::Token,
};
use bn_cli::{
    check::check,
    diagnostics::{diagnostic_json, emit_frontend_warnings, render_diagnostic},
    frontend::{Frontend, load_frontend, load_frontend_with_overlays},
    help::COMMON_OPTIONS,
    options::{EvalMapping, OptionExtension, Options, OutputFormat, parse_options},
    output::{emit_output, language_error, log, module_index, tokens_text, tool_error},
    process_log::{LogLevel, ProcessLog},
};
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
enum EvalMode {
    Snippet,
    Program,
}

fn eval_source(args: Vec<String>) -> Result<(String, Vec<String>, EvalMode), String> {
    let mut source = None;
    let mut options = Vec::new();
    let mut program_arguments = Vec::new();
    let mut stdin_source = false;
    let mut mode = EvalMode::Snippet;
    let mut after_separator = false;
    let mut arguments = args.into_iter();
    while let Some(argument) = arguments.next() {
        if after_separator {
            program_arguments.push(argument);
        } else if argument == "--" {
            after_separator = true;
        } else if argument == "--stdin" {
            if stdin_source {
                return Err("duplicate --stdin".into());
            }
            stdin_source = true;
        } else if argument == "--session" {
            return Err("--session is not available in 0.5.1".into());
        } else if argument == "--format" {
            options.push(argument);
            options.push(
                arguments
                    .next()
                    .ok_or_else(|| "--format expects text or json".to_string())?,
            );
        } else if argument == "--mode" {
            mode = match arguments.next().as_deref() {
                Some("snippet") => EvalMode::Snippet,
                Some("program") => EvalMode::Program,
                _ => return Err("--mode expects snippet or program".into()),
            };
        } else if matches!(
            argument.as_str(),
            "--warnings"
                | "--allow"
                | "--deny"
                | "--warn"
                | "--config"
                | "--target"
                | "--opt"
                | "--color"
                | "--emit"
                | "-o"
                | "--output"
                | "--log-level"
                | "--log-file"
                | "--read-root"
                | "--write-root"
                | "--module-path"
        ) {
            options.push(argument);
            options.push(
                arguments
                    .next()
                    .ok_or_else(|| "option expects a value".to_string())?,
            );
        } else if matches!(
            argument.as_str(),
            "-v" | "-vv"
                | "--verbose"
                | "--trace"
                | "--no-filesystem"
                | "--sandbox"
                | "--jupyter-stdin"
                | "--no-log"
        ) {
            options.push(argument);
        } else if source.is_none() {
            source = Some(argument);
        } else {
            options.push(argument);
        }
    }
    if stdin_source == source.is_some() {
        return Err("bn eval requires exactly one source form: SOURCE or --stdin".into());
    }
    let text = if stdin_source {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| format!("cannot read eval source: {error}"))?;
        text
    } else {
        source.expect("source form checked above")
    };
    if !program_arguments.is_empty() {
        options.push("--".into());
        options.extend(program_arguments);
    }
    Ok((text, options, mode))
}

fn eval_top_level_start_span(source_text: &str) -> Option<bn::source::Span> {
    let source = SourceFile::new("<eval-classify>", source_text);
    let Ok(tokens) = lex(&source) else {
        return None;
    };
    let Ok(program) = bn::parser::parse(&tokens) else {
        return None;
    };
    program.items.iter().find_map(|item| match item {
        Item::Declaration {
            kind: DeclarationKind::Function,
            name,
            span,
            ..
        } if name == "Start" => Some(*span),
        _ => None,
    })
}

fn eval_promotion_diagnostic(source_text: &str, span: bn::source::Span) -> Diagnostic {
    let source = SourceFile::new("<eval>", source_text);
    let span = bn::source::Span {
        start: bn::source::Position {
            source_id: source.source_id,
            revision: source.revision,
            ..span.start
        },
        end: bn::source::Position {
            source_id: source.source_id,
            revision: source.revision,
            ..span.end
        },
    };
    Diagnostic::structured(
        DiagId::EVAL_START_PROMOTED,
        vec![(
            "message".into(),
            DiagnosticValue::from(
                "top-level FUNCTION Start was detected; evaluating as a complete program",
            ),
        )],
        vec![Label {
            span,
            style: LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("promotion diagnostic schema is registered")
}

fn eval_line_partition(line: &str, in_block_comment: &mut bool) -> (bool, bool) {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut string = false;
    let mut visible = String::new();
    while index < bytes.len() {
        if *in_block_comment {
            if bytes[index..].starts_with(b"*/") {
                *in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if string {
            visible.push(bytes[index] as char);
            if bytes[index] == b'\\' {
                if let Some(next) = bytes.get(index + 1) {
                    visible.push(*next as char);
                }
                index = (index + 2).min(bytes.len());
            } else if bytes[index] == b'"' {
                string = false;
                index += 1;
            } else {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"//") {
            break;
        }
        if bytes[index..].starts_with(b"/*") {
            *in_block_comment = true;
            index += 2;
        } else if bytes[index] == b'"' {
            string = true;
            visible.push('"');
            index += 1;
        } else {
            visible.push(bytes[index] as char);
            index += 1;
        }
    }
    let trimmed = visible.trim_start();
    (trimmed.is_empty(), trimmed.starts_with("IMPORT "))
}

fn eval_json_error(code: &str, message: impl Into<String>, phase: &str, exit_code: u8) -> ExitCode {
    println!(
        "{}",
        serde_json::json!({
            "schema_version": 1, "ok": false, "exit_code": exit_code,
            "stdout": "", "stderr": "",
            "diagnostics": [{"code": code, "severity": "error", "phase": phase,
                "title": "Invalid eval configuration", "message": message.into(),
                "labels": [], "causes": [], "help": serde_json::Value::Null}],
        })
    );
    ExitCode::from(exit_code)
}

#[allow(clippy::too_many_lines)]
fn eval(command_arguments: Vec<String>) -> ExitCode {
    let json_requested = command_arguments
        .windows(2)
        .any(|window| window[0] == "--format" && window[1] == "json");
    let (snippet, mut arguments, mode) = match eval_source(command_arguments) {
        Ok(value) => value,
        Err(message) => {
            if json_requested {
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1,
                        "ok": false,
                        "exit_code": 2,
                        "stdout": "",
                        "stderr": "",
                        "diagnostics": [{
                            "code": "CONFIG_INVALID",
                            "severity": "error",
                            "phase": "cli",
                            "title": "Invalid eval options",
                            "message": message,
                            "labels": [],
                            "causes": [],
                            "help": serde_json::Value::Null,
                        }],
                    })
                );
            } else {
                eprintln!("error: {message}");
            }
            return tool_error();
        }
    };
    let promotion_span = (mode == EvalMode::Snippet)
        .then(|| eval_top_level_start_span(&snippet))
        .flatten();
    let has_start = promotion_span.is_some();
    let virtual_path = env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(if mode == EvalMode::Program || has_start {
            ".bn-eval-program.bn"
        } else {
            ".bn-eval-input.bn"
        });
    let separator = arguments.iter().position(|argument| argument == "--");
    if let Some(index) = separator {
        arguments.insert(index, virtual_path.display().to_string());
    } else {
        arguments.push(virtual_path.display().to_string());
    }
    let mut options = match parse_options(arguments.into_iter(), &mut BuildOptions::default()) {
        Ok(options) => options,
        Err(message) => {
            if json_requested {
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1,
                        "ok": false,
                        "exit_code": 2,
                        "stdout": "",
                        "stderr": "",
                        "diagnostics": [{
                            "code": "CONFIG_INVALID",
                            "severity": "error",
                            "phase": "cli",
                            "title": "Invalid eval options",
                            "message": message,
                            "labels": [],
                            "causes": [],
                            "help": serde_json::Value::Null,
                        }],
                    })
                );
            } else {
                eprintln!("error: {message}");
            }
            return tool_error();
        }
    };
    if options.output_format == OutputFormat::Json {
        // JSON v1 owns both channels; verbosity/token tracing are incompatible
        // with the one-document stdout contract and are therefore suppressed.
        options.verbosity = 0;
        options.trace = false;
        options.jupyter_stdin = false;
        options.no_log = true;
    }
    options.eval_promotion_warning = has_start && mode == EvalMode::Snippet;
    options.eval_promotion_span = promotion_span;
    let mut expression_wrapped = false;
    let wrapped = if mode == EvalMode::Program || has_start {
        snippet.clone()
    } else {
        let mut imports = Vec::new();
        let mut body = Vec::new();
        let mut body_started = false;
        let mut in_block_comment = false;
        for line in snippet.split_inclusive('\n') {
            let (trivia, is_import) = eval_line_partition(line, &mut in_block_comment);
            if !body_started && (trivia || is_import) {
                imports.push(line);
            } else {
                body_started = true;
                body.push(line);
            }
        }
        let body_text = body.concat();
        let expression_candidate = body_text.trim_end_matches(['\r', '\n']);
        let expression = {
            let expression_source = SourceFile::new("<eval-expression>", expression_candidate);
            bn::lexer::lex(&expression_source)
                .ok()
                .and_then(|tokens| bn::parser::parse_expression(&tokens).ok())
                .is_some()
                && !expression_candidate.contains(['\r', '\n'])
        };
        let body = if expression {
            expression_wrapped = true;
            format!("PRINT {body_text}")
        } else {
            body_text.clone()
        };
        let mut wrapped = imports.concat();
        wrapped.push_str("FUNCTION Start() AS VOID\n");
        wrapped.push_str(&body);
        if !body.ends_with('\n') {
            wrapped.push('\n');
        }
        wrapped.push_str("END FUNCTION\n");
        wrapped
    };
    let (insertion_offset, inserted_length, insertion_line) =
        if mode == EvalMode::Program || has_start {
            (0, 0, 1)
        } else {
            let marker = "FUNCTION Start() AS VOID\n";
            let insertion_offset = wrapped.find(marker).unwrap_or(0);
            (
                insertion_offset,
                marker.len(),
                wrapped[..insertion_offset]
                    .bytes()
                    .filter(|byte| *byte == b'\n')
                    .count()
                    + 1,
            )
        };
    options.eval_mapping = Some(EvalMapping {
        source_name: "<eval>".into(),
        insertion_offset,
        inserted_length,
        insertion_line,
        expression_prefix: expression_wrapped
            .then_some((insertion_offset + inserted_length, "PRINT ".len())),
    });
    options.eval_source_text = Some(snippet.clone());
    let source = SourceFile::new(&options.path, wrapped.clone());
    let tokens = match lex(&source) {
        Ok(tokens) => tokens,
        Err(diagnostic) => {
            if options.output_format == OutputFormat::Json {
                let envelope = serde_json::json!({
                    "schema_version": 1,
                    "ok": false,
                    "exit_code": 1,
                    "stdout": "",
                    "stderr": "",
                    "diagnostics": [diagnostic_json(&diagnostic, &source, &options, "lexical")],
                });
                println!("{envelope}");
            } else {
                eprintln!("{}", render_diagnostic(&diagnostic, &source, &options));
            }
            return language_error();
        }
    };
    let mut overlays = BTreeMap::new();
    overlays.insert(PathBuf::from(&options.path), wrapped);
    let frontend = match load_frontend_with_overlays(&source, &options, &overlays) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    run_loaded(&source, &tokens, &options, &frontend)
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
        return eval(arguments.collect());
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

fn run(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    let frontend = match load_frontend(source, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    run_loaded(source, tokens, options, &frontend)
}

#[allow(clippy::too_many_lines)]
fn run_loaded(
    source: &SourceFile,
    tokens: &[Token],
    options: &Options,
    frontend: &Frontend,
) -> ExitCode {
    if emit_frontend_warnings(frontend, source, options) {
        if options.output_format == OutputFormat::Json {
            let diagnostics = frontend
                .warnings
                .iter()
                .filter_map(|warning| {
                    let module = frontend.graph.modules.get(module_index(warning.module.0))?;
                    Some(diagnostic_json(
                        &warning.diagnostic,
                        &module.source,
                        options,
                        "semantic",
                    ))
                })
                .collect::<Vec<_>>();
            println!(
                "{}",
                serde_json::json!({
                    "schema_version": 1,
                    "ok": false,
                    "exit_code": 1,
                    "stdout": "",
                    "stderr": "",
                    "diagnostics": diagnostics,
                })
            );
        }
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
    if options.eval_promotion_warning {
        let promotion = eval_promotion_diagnostic(
            options.eval_source_text.as_deref().unwrap_or(""),
            options
                .eval_promotion_span
                .expect("promotion warning has a source span"),
        );
        let level = options.warning_policy.level(DiagId::EVAL_START_PROMOTED);
        if level == Level::Error {
            if options.output_format == OutputFormat::Json {
                println!(
                    "{}",
                    serde_json::json!({
                        "schema_version": 1, "ok": false, "exit_code": 1,
                        "stdout": "", "stderr": "",
                        "diagnostics": [diagnostic_json(&promotion, source, options, "cli")],
                    })
                );
            } else {
                eprintln!("{}", render_diagnostic(&promotion, source, options));
            }
            return language_error();
        }
        if level == Level::Warn && options.output_format == OutputFormat::Text {
            eprintln!("{}", render_diagnostic(&promotion, source, options));
        }
    }
    let mut host = if options.sandbox {
        match HostEnv::system(arguments.clone())
            .with_default_providers()
            .with_filesystem_roots(options.read_roots.clone(), options.write_roots.clone())
        {
            Ok(host) => host,
            Err(message) => {
                if options.output_format == OutputFormat::Json {
                    return eval_json_error("CONFIG_INVALID", message, "config", 2);
                }
                eprintln!("error: {message}");
                return tool_error();
            }
        }
    } else if options.filesystem {
        HostEnv::system(arguments).with_default_providers()
    } else {
        HostEnv::system(arguments)
            .with_default_providers()
            .without_filesystem()
    };
    // One environment read, one parser (`bn_rt::Policy::narrow_from_env`):
    // the same four inputs a compiled artifact honours in `bn_rt_policy_init`.
    host = match host.narrowed_by_env(|name| env::var(name).ok()) {
        Ok(host) => host,
        Err(error) => {
            let message = error.to_string();
            if options.output_format == OutputFormat::Json {
                return eval_json_error("CONFIG_INVALID", message, "config", 2);
            }
            eprintln!("error: {message}");
            return tool_error();
        }
    };
    let stdin = io::stdin();
    if options.output_format == OutputFormat::Json {
        let mut output = Vec::new();
        let mut input = JupyterInput {
            input: stdin.lock(),
            notify: options.jupyter_stdin,
        };
        let result = execute_validated_with_host(module, &mut input, &mut output, &host);
        let mut diagnostics = frontend
            .warnings
            .iter()
            .filter_map(|warning| {
                let id = DiagId::from_code(warning.diagnostic.code)?;
                if options.warning_policy.level(id) == Level::Allow {
                    return None;
                }
                let module = frontend.graph.modules.get(module_index(warning.module.0))?;
                Some(diagnostic_json(
                    &warning.diagnostic,
                    &module.source,
                    options,
                    "semantic",
                ))
            })
            .collect::<Vec<_>>();
        if options.eval_promotion_warning {
            let promotion = eval_promotion_diagnostic(
                options.eval_source_text.as_deref().unwrap_or(""),
                options
                    .eval_promotion_span
                    .expect("promotion warning has a source span"),
            );
            if options.warning_policy.level(DiagId::EVAL_START_PROMOTED) != Level::Allow {
                diagnostics.insert(0, diagnostic_json(&promotion, source, options, "cli"));
            }
        }
        let (ok, exit_code) = match result {
            Ok(code) => (code == 0, code),
            Err(diagnostic) => {
                diagnostics.push(diagnostic_json(&diagnostic, source, options, "runtime"));
                (
                    false,
                    if diagnostic.code == "EXECUTION_POLICY_DENIED" {
                        2
                    } else {
                        1
                    },
                )
            }
        };
        let envelope = serde_json::json!({
            "schema_version": 1,
            "ok": ok,
            "exit_code": exit_code,
            "stdout": String::from_utf8_lossy(&output),
            "stderr": "",
            "diagnostics": diagnostics,
        });
        println!("{envelope}");
        ExitCode::from(exit_code)
    } else {
        let stdout = io::stdout();
        let mut input = JupyterInput {
            input: stdin.lock(),
            notify: options.jupyter_stdin,
        };
        match execute_validated_with_host(module, &mut input, &mut stdout.lock(), &host) {
            Ok(code) => ExitCode::from(code),
            Err(diagnostic) => {
                eprintln!("{}", render_diagnostic(&diagnostic, source, options));
                if diagnostic.code == "EXECUTION_POLICY_DENIED" {
                    tool_error()
                } else {
                    language_error()
                }
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

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
