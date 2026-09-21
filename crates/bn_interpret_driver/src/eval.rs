//! The `eval` command: snippet/program modes, `Start` promotion, the
//! `FUNCTION Start` wrapper with source-coordinate mapping for diagnostics,
//! and the JSON v1 error envelopes. Execution is shared with `run`.

use std::{
    collections::BTreeMap,
    env,
    io::{self, Read},
    path::PathBuf,
    process::ExitCode,
};

use bn_cli::{
    diagnostics::{diagnostic_json, render_diagnostic},
    frontend::load_frontend_with_overlays,
    options::{EvalMapping, OptionExtension, OutputFormat, parse_options},
    output::{language_error, tool_error},
};
use bn_diag::{DiagId, Diagnostic, DiagnosticValue, Label, LabelStyle};
use bn_frontend::{
    ast::{DeclarationKind, Item},
    lexer::lex,
};
use bn_source::SourceFile;

use crate::run::run_loaded;

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

fn eval_top_level_start_span(source_text: &str) -> Option<bn_source::Span> {
    let source = SourceFile::new("<eval-classify>", source_text);
    let Ok(tokens) = lex(&source) else {
        return None;
    };
    let Ok(program) = bn_frontend::parser::parse(&tokens) else {
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

pub(crate) fn eval_promotion_diagnostic(source_text: &str, span: bn_source::Span) -> Diagnostic {
    let source = SourceFile::new("<eval>", source_text);
    let span = bn_source::Span {
        start: bn_source::Position {
            source_id: source.source_id,
            revision: source.revision,
            ..span.start
        },
        end: bn_source::Position {
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

pub(crate) fn eval_json_error(
    code: &str,
    message: impl Into<String>,
    phase: &str,
    exit_code: u8,
) -> ExitCode {
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
pub fn eval(command_arguments: Vec<String>, extension: &mut dyn OptionExtension) -> ExitCode {
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
    let mut options = match parse_options(arguments.into_iter(), extension) {
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
            bn_frontend::lexer::lex(&expression_source)
                .ok()
                .and_then(|tokens| bn_frontend::parser::parse_expression(&tokens).ok())
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
