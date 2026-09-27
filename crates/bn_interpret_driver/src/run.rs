//! The `run` command over an already prepared program, and the execution
//! I/O it shares with `eval`: text or JSON v1 envelope, Jupyter input
//! notifications, HOST environment composition and exit classification.

use std::{
    fs,
    io::{self, BufRead, Read},
    process::ExitCode,
};

use bn_cli::{
    diagnostics::{diagnostic_json, emit_frontend_warnings, render_diagnostic},
    frontend::{Frontend, load_frontend},
    options::{Options, OutputFormat},
    output::{language_error, log, module_index, tokens_text, tool_error},
};
use bn_diag::{DiagId, Level};
use bn_frontend::token::Token;
use bn_interp::execute_validated_with_host;
use bn_source::SourceFile;

use crate::eval::{eval_json_error, eval_promotion_diagnostic};

/// Prepares `options.path` and executes it.
#[must_use]
pub fn run(source: &SourceFile, tokens: &[Token], options: &Options) -> ExitCode {
    let frontend = match load_frontend(source, options) {
        Ok(frontend) => frontend,
        Err(code) => return code,
    };
    run_loaded(source, tokens, options, &frontend)
}

/// Executes an already prepared program (text or JSON v1 output).
///
/// # Panics
///
/// When `options.eval_promotion_warning` is set without
/// `eval_promotion_span`; `eval` always sets both together.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn run_loaded(
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
    let host = match crate::environment::host_env(options, arguments) {
        Ok(host) => host,
        Err(message) => {
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

pub struct JupyterInput<R> {
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
