// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    env, fmt, fs,
    io::{self, BufRead, IsTerminal, Read},
    path::Path,
    process::ExitCode,
};

use bn::{
    ast::Program,
    ir::{Constant, Instruction, Module as IrModule, lower_graph},
    lexer::lex,
    llvm::lower_module,
    module_graph::{ModuleGraph, load},
    parser::parse_named,
    runtime::{HostEnv, execute_with_host},
    semantic::{SemanticModel, analyze_modules},
    source::SourceFile,
    token::Token,
};

const VERSION: &str = "bn 0.2.0";

#[must_use]
fn language_error() -> ExitCode {
    ExitCode::from(1)
}

#[must_use]
fn tool_error() -> ExitCode {
    ExitCode::from(2)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Emit {
    Tokens,
    Ast,
    TypedAst,
    Ir,
}

#[derive(Clone, Copy, Debug)]
enum Color {
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
struct Options {
    path: String,
    verbosity: u8,
    emit: Option<Emit>,
    output: Option<String>,
    trace: bool,
    color: Color,
    target: Target,
    filesystem: bool,
    jupyter_stdin: bool,
    program_arguments: Vec<String>,
}

struct Frontend {
    graph: ModuleGraph,
    program: Program,
    models: Vec<SemanticModel>,
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
        _ => {}
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
            eprintln!("{}", diagnostic.render(&source));
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
    let frontend = match load_frontend(source, tokens, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    let module = match lower_graph(&frontend.graph, &frontend.models) {
        Ok(module) => module,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            return language_error();
        }
    };
    if options.target == Target::Wasm32 && requires_unavailable_wasm_capability(&module) {
        eprintln!(
            "error[BUILD_CAPABILITY_UNAVAILABLE]: target wasm32 does not provide HOST.FileSystem or HOST.Console"
        );
        return language_error();
    }
    match lower_module(&module) {
        Ok(llvm) => emit_build_output(llvm, options),
        Err(message) => {
            eprintln!("error[{message}]");
            tool_error()
        }
    }
}

fn requires_unavailable_wasm_capability(module: &IrModule) -> bool {
    module.filesystem_import.is_some()
        || module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
            .any(|instruction| {
                matches!(
                    instruction,
                    Instruction::Constant {
                        value: Constant::HostConsole,
                        ..
                    } | Instruction::ClearScreen { .. }
                        | Instruction::Beep { .. }
                )
            })
}

fn emit_build_output(llvm: String, options: &Options) -> ExitCode {
    let Some(output) = options.output.as_deref() else {
        return emit_output(llvm, None);
    };
    let temporary = env::temp_dir().join(format!("basicnext-llvm-{}.ll", std::process::id()));
    if let Err(error) = fs::write(&temporary, llvm) {
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
        let compiled = std::process::Command::new(clang)
            .args([
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
                std::process::Command::new(configured_wasm_ld())
                    .args([
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
        std::process::Command::new(clang)
            .args([temporary.to_string_lossy().as_ref(), "-o", output])
            .output()
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

fn configured_wasm_ld() -> String {
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

fn configured_wasm_clang() -> Result<String, String> {
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

fn configured_clang() -> Result<String, String> {
    Ok(toolchain_value("clang")?.unwrap_or_else(|| "clang".into()))
}

fn toolchain_value(key: &str) -> Result<Option<String>, String> {
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

fn brew_bin(formula: &str, binary: &str) -> Option<String> {
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

fn clang_has_wasm32(clang: &str) -> bool {
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

fn command_succeeds(command: &str, args: &[&str]) -> bool {
    std::process::Command::new(command)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn run(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    let frontend = match load_frontend(source, tokens, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    log(options.verbosity, 1, "lowering typed BN IR");
    let module = match lower_graph(&frontend.graph, &frontend.models) {
        Ok(module) => module,
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            return language_error();
        }
    };
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
    let host = if options.filesystem {
        HostEnv::system(arguments)
    } else {
        HostEnv::system(arguments).without_filesystem()
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = JupyterInput {
        input: stdin.lock(),
        notify: options.jupyter_stdin,
    };
    match execute_with_host(&module, &mut input, &mut stdout.lock(), &host) {
        Ok(code) => ExitCode::from(code),
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render(source));
            language_error()
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
            Emit::Ast => format!("{:#?}\n", frontend.program),
            Emit::TypedAst => format!("{:#?}\n{semantic_model:#?}\n", frontend.program),
            Emit::Ir => match lower_graph(&frontend.graph, &frontend.models) {
                Ok(module) => format!("{module:#?}\n"),
                Err(diagnostic) => {
                    eprintln!("{}", diagnostic.render(source));
                    return language_error();
                }
            },
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

fn load_frontend(
    source: &SourceFile,
    tokens: &[Token],
    options: &Options,
) -> Result<Frontend, ExitCode> {
    log(options.verbosity, 1, "loading module graph");
    let graph = load(&options.path).map_err(|error| {
        eprintln!("{}", error.diagnostic.render(&error.source));
        language_error()
    })?;
    log(options.verbosity, 1, "syntax analysis");
    let program = parse_named(tokens, &source.name).map_err(|diagnostic| {
        eprintln!("{}", diagnostic.render(source));
        language_error()
    })?;
    log(options.verbosity, 1, "semantic analysis");
    let models = match analyze_modules(&graph) {
        Ok(models) => models,
        Err(error) => {
            let rendered = graph.modules.get(module_index(error.module.0)).map_or_else(
                || error.diagnostic.render(source),
                |module| error.diagnostic.render(&module.source),
            );
            eprintln!("{rendered}");
            return Err(language_error());
        }
    };
    log(
        options.verbosity,
        1,
        format!("parser completed: {} top-level items", program.items.len()),
    );
    Ok(Frontend {
        graph,
        program,
        models,
    })
}

fn parse_options(mut arguments: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut path = None;
    let mut verbosity = 0u8;
    let mut emit = None;
    let mut output = None;
    let mut trace = false;
    let mut color = Color::Auto;
    let mut target = Target::Native;
    let mut filesystem = true;
    let mut jupyter_stdin = false;
    let mut program_arguments = Vec::new();
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--" => {
                program_arguments.extend(arguments);
                break;
            }
            "-h" | "--help" => return Err("help is available as bn --help".into()),
            "-v" | "--verbose" => verbosity = verbosity.saturating_add(1).min(2),
            "-vv" => verbosity = 2,
            "--trace" => trace = true,
            "--no-filesystem" => filesystem = false,
            "--jupyter-stdin" => jupyter_stdin = true,
            "--target" => {
                target = match arguments.next().as_deref() {
                    Some("native") => Target::Native,
                    Some("wasm32") => Target::Wasm32,
                    _ => return Err("--target expects native or wasm32".into()),
                };
            }
            "--emit" => {
                if emit.is_some() {
                    return Err("--emit was specified more than once".into());
                }
                emit = Some(match arguments.next().as_deref() {
                    Some("tokens") => Emit::Tokens,
                    Some("ast") => Emit::Ast,
                    Some("typed-ast") => Emit::TypedAst,
                    Some("ir") => Emit::Ir,
                    _ => return Err("--emit expects tokens, ast, typed-ast, or ir".into()),
                });
            }
            "-o" | "--output" => {
                if output.is_some() {
                    return Err("output file was specified more than once".into());
                }
                output = Some(
                    arguments
                        .next()
                        .filter(|value| !value.starts_with('-'))
                        .ok_or_else(|| "-o expects an output file path".to_string())?,
                );
            }
            "--color" => {
                color = match arguments.next().as_deref() {
                    Some("auto") => Color::Auto,
                    Some("always") => Color::Always,
                    Some("never") => Color::Never,
                    _ => return Err("--color expects auto, always, or never".into()),
                }
            }
            _ if path.is_none() && !argument.starts_with('-') => path = Some(argument),
            _ => return Err(format!("unknown or repeated option '{argument}'")),
        }
    }
    path.map(|path| Options {
        path,
        verbosity,
        emit,
        output,
        trace,
        color,
        target,
        filesystem,
        jupyter_stdin,
        program_arguments,
    })
    .ok_or_else(|| "missing source file".into())
}

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

fn color_enabled(color: Color) -> bool {
    matches!(color, Color::Always) || (matches!(color, Color::Auto) && io::stdout().is_terminal())
}

fn colorize(text: &str, color: Color) -> String {
    if color_enabled(color) {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.into()
    }
}

fn log(verbosity: u8, level: u8, message: impl fmt::Display) {
    if verbosity >= level {
        eprintln!("[bn] {message}");
    }
}

fn module_index(id: u32) -> usize {
    usize::try_from(id).unwrap_or(usize::MAX)
}

fn help() -> ExitCode {
    println!(
        "\
{VERSION}
usage: bn <check|lex|run|build> [options] <file.bn> [-- program-args]

commands:
  check   validate lexer, parser, and semantics
  lex     print the token stream
  run     execute FUNCTION Start through typed BN IR
  build   compile the supported typed BN IR subset with LLVM

options:
  -v, --verbose              show pipeline stages (repeat for tokens: -v -v)
  -vv                        show stages and tokens
  --emit tokens|ast|typed-ast|ir
                             print a frontend artifact (use with check or lex)
  -o, --output <file>        write an emitted artifact to <file>
  --trace                    report the bn run execution entry point
  --target native|wasm32     select the build target (build only)
  --no-filesystem            deny HOST.FileSystem imports (run only)
  --color auto|always|never  control ANSI color on status messages
  -V, --version              print version
  -h, --help                 print this help

HOST.Args[0] is the source path. Extra program arguments follow --.
See also: man bn
"
    );
    ExitCode::SUCCESS
}

fn usage() -> ExitCode {
    eprintln!("usage: bn <check|lex|run|build> [options] <file.bn>\ntry: bn --help");
    tool_error()
}

#[cfg(test)]
mod tests {
    use super::{Color, clang_has_wasm32, colorize, configured_clang, parse_options};

    fn opts(args: &[&str]) -> super::Options {
        parse_options(args.iter().map(|argument| (*argument).to_string())).expect("parse options")
    }

    #[test]
    fn stacked_verbose_flags_accumulate() {
        assert_eq!(opts(&["-v", "a.bn"]).verbosity, 1);
        assert_eq!(opts(&["-v", "-v", "a.bn"]).verbosity, 2);
        assert_eq!(opts(&["--verbose", "a.bn"]).verbosity, 1);
        assert_eq!(opts(&["-vv", "a.bn"]).verbosity, 2);
    }

    #[test]
    fn repeated_emit_is_an_error() {
        let error = parse_options(
            ["--emit", "ir", "--emit", "ast", "a.bn"]
                .iter()
                .map(|argument| (*argument).to_string()),
        )
        .expect_err("repeated --emit");
        assert!(error.contains("--emit"));
    }

    #[test]
    fn success_color_can_be_disabled() {
        assert_eq!(colorize("ok", Color::Never), "ok");
        assert!(colorize("ok", Color::Always).contains("ok"));
    }

    #[test]
    fn compiler_configuration_selects_clang() {
        assert_eq!(configured_clang().expect("read configuration"), "clang");
    }

    #[test]
    fn apple_clang_is_not_a_wasm32_compiler() {
        if configured_clang().ok().as_deref() == Some("clang") {
            assert!(
                !clang_has_wasm32("clang")
                    || std::process::Command::new("clang")
                        .arg("--version")
                        .output()
                        .is_ok_and(|output| !String::from_utf8_lossy(&output.stdout)
                            .contains("Apple clang")),
                "PATH clang is Apple clang and must not be used for wasm32"
            );
        }
    }
}
