// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.
mod cli_help;
use cli_help::{help, usage};
// would this file load configurations if there is any?

use std::{
    env, fs,
    io::{self, BufRead, Read},
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

const VERSION: &str = "bn 0.4.2";

#[must_use]
fn language_error() -> ExitCode {
    ExitCode::from(1)
}

#[must_use]
fn tool_error() -> ExitCode {
    ExitCode::from(2)
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
            "error[BUILD_CAPABILITY_UNAVAILABLE]: target wasm32 does not provide HOST.FileSystem, HOST.Console, HOST.Net, BNLog, or BNWeb"
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
        || module.network_import.is_some()
        || module.bnlog_import.is_some()
        || module.bnweb_import.is_some()
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

mod cli_toolchain;
use cli_toolchain::{configured_clang, configured_wasm_clang, configured_wasm_ld};

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
