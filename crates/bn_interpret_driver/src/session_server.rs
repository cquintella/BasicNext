// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Persistent interactive session NDJSON protocol server (bni eval --session).
//! Handles multi-turn incremental interpretation, submission classification,
//! and compiler/ARC introspection queries over stdio.

use std::{
    collections::{BTreeMap, HashMap},
    io::{self, BufRead, Write},
    process::ExitCode,
};

use bn_cli::{diagnostics::diagnostic_json, options::Options, output::tool_error};
use bn_frontend::{
    SessionAccumulator, SubmissionKind,
    session::{ClassifiedTokenKind, SubmissionCompleteness, classify_submission},
    token::reserved_words,
};
use bn_interp::InteractiveSession;
use bn_ir::SymbolId;
use bn_source::SourceFile;
use bn_value::Value;
use serde_json::{Value as JsonValue, json};

fn classified_kind_str(kind: ClassifiedTokenKind) -> &'static str {
    match kind {
        ClassifiedTokenKind::Keyword => "keyword",
        ClassifiedTokenKind::Identifier => "identifier",
        ClassifiedTokenKind::Number => "number",
        ClassifiedTokenKind::String => "string",
        ClassifiedTokenKind::Comment => "comment",
        ClassifiedTokenKind::Symbol => "symbol",
        ClassifiedTokenKind::Operator => "operator",
        ClassifiedTokenKind::Special => "special",
        ClassifiedTokenKind::Newline => "newline",
        ClassifiedTokenKind::Whitespace => "whitespace",
        ClassifiedTokenKind::Other => "other",
    }
}

fn value_to_summary_json(val: &Value) -> JsonValue {
    match val {
        Value::Integer(i, _) => json!(i),
        Value::Float(f, _) => json!(f),
        Value::Boolean(b) => json!(b),
        Value::String(s) => json!(s.as_ref()),
        Value::Vector(v) => {
            json!({
                "kind": "vector",
                "length": v.len(),
                "elements": v.iter().take(10).map(value_to_summary_json).collect::<Vec<_>>()
            })
        }
        Value::Record { record } => {
            json!({
                "kind": "struct",
                "type_name": record.type_name().as_ref(),
                "length": record.len(),
                "fields": record.iter().take(10).map(value_to_summary_json).collect::<Vec<_>>()
            })
        }
        Value::DataFrame(h) => json!({"kind": "dataframe", "handle": h}),
        Value::Json(h) => json!({"kind": "json", "handle": h}),
        Value::Object { class, handle } => json!({
            "kind": "object",
            "class": class.as_ref(),
            "slot": handle.slot
        }),
        Value::Pointer { handle, element } => json!({
            "kind": "pointer",
            "slot": handle.slot,
            "element": element
        }),
        Value::Null => json!(null),
        Value::Date(d) => json!({"kind": "date", "days": d}),
        Value::Time(t) => json!({"kind": "time", "micros": t}),
        Value::TimeZone(tz) => json!({"kind": "timezone", "zone": tz.as_ref()}),
        _ => json!(format!("{val:?}")),
    }
}

fn handle_classify(req: &JsonValue, options: &Options) -> JsonValue {
    let source = req["source"].as_str().unwrap_or("");
    match classify_submission(source) {
        SubmissionCompleteness::Complete { tokens } => {
            let tokens_json = tokens
                .into_iter()
                .map(|t| {
                    json!({
                        "kind": classified_kind_str(t.kind),
                        "text": t.text,
                        "offset": t.offset,
                        "length": t.length,
                        "line": t.line,
                        "column": t.column,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "ok": true,
                "status": "complete",
                "tokens": tokens_json,
            })
        }
        SubmissionCompleteness::Incomplete {
            tokens,
            open_constructs,
        } => {
            let tokens_json = tokens
                .into_iter()
                .map(|t| {
                    json!({
                        "kind": classified_kind_str(t.kind),
                        "text": t.text,
                        "offset": t.offset,
                        "length": t.length,
                        "line": t.line,
                        "column": t.column,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "ok": true,
                "status": "incomplete",
                "tokens": tokens_json,
                "open_constructs": open_constructs,
            })
        }
        SubmissionCompleteness::Invalid { tokens, diagnostic } => {
            let source_file = SourceFile::new("<session-classify>", source);
            let tokens_json = tokens
                .into_iter()
                .map(|t| {
                    json!({
                        "kind": classified_kind_str(t.kind),
                        "text": t.text,
                        "offset": t.offset,
                        "length": t.length,
                        "line": t.line,
                        "column": t.column,
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "ok": false,
                "status": "invalid",
                "tokens": tokens_json,
                "diagnostics": [diagnostic_json(&diagnostic, &source_file, options, "syntax", None)],
            })
        }
    }
}

#[allow(clippy::too_many_lines)]
fn handle_submit(
    req: &JsonValue,
    options: &Options,
    accumulator: &mut SessionAccumulator,
    session: &mut InteractiveSession,
) -> JsonValue {
    let source = req["source"].as_str().unwrap_or("");
    let prepared_sub = match accumulator.compile_submission(source) {
        Ok(sub) => sub,
        Err(diag) => {
            let source_file = SourceFile::new("<session>", source);
            return json!({
                "ok": false,
                "exit_code": 1,
                "stdout": "",
                "stderr": "",
                "result_value": null,
                "result_type": null,
                "structured_data": null,
                "diagnostics": [diagnostic_json(&diag, &source_file, options, "semantic", None)],
            });
        }
    };

    let mut var_symbol_map = HashMap::new();
    if let Some(model) = prepared_sub.prepared.root_model() {
        for sym in &model.symbols {
            var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
        }
    }

    let input = io::empty();
    let mut stdout_buf = Vec::new();
    let exec_res = session.execute_submission(
        &prepared_sub.prepared.validated,
        &var_symbol_map,
        &mut io::BufReader::new(input),
        &mut stdout_buf,
    );

    match exec_res {
        Err(diag) => {
            let source_file = SourceFile::new("<session>", source);
            json!({
                "ok": false,
                "exit_code": 1,
                "stdout": String::from_utf8_lossy(&stdout_buf).to_string(),
                "stderr": "",
                "result_value": null,
                "result_type": null,
                "structured_data": null,
                "diagnostics": [diagnostic_json(&diag, &source_file, options, "runtime", None)],
            })
        }
        Ok(res) => {
            let mut new_var_types = BTreeMap::new();
            if let Some(model) = prepared_sub.prepared.root_model() {
                for name in res.updated_variables.keys() {
                    if let Some(sym) = model.symbols.iter().rev().find(|s| &s.name == name) {
                        new_var_types.insert(name.clone(), sym.type_name.clone());
                    }
                }
            }
            accumulator.commit_turn(source, new_var_types);

            let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
            let result_type = if prepared_sub.kind == SubmissionKind::Expression {
                prepared_sub.prepared.root_model().and_then(|m| {
                    m.expressions
                        .last()
                        .map(|e| bn_frontend::semantic::display(&e.ty))
                })
            } else {
                None
            };
            let result_value = if prepared_sub.kind == SubmissionKind::Expression {
                res.updated_variables
                    .get("_")
                    .map(bn_interp::render_pub)
                    .or_else(|| {
                        if stdout.is_empty() {
                            None
                        } else {
                            Some(stdout.trim_end_matches(['\r', '\n']).to_string())
                        }
                    })
            } else {
                None
            };
            let structured_data = res
                .returned_value
                .as_ref()
                .map(value_to_summary_json)
                .or_else(|| {
                    if res.updated_variables.len() == 1 {
                        res.updated_variables
                            .values()
                            .next()
                            .map(value_to_summary_json)
                    } else {
                        None
                    }
                });

            json!({
                "ok": true,
                "exit_code": res.exit_code,
                "stdout": stdout,
                "stderr": "",
                "result_value": result_value,
                "result_type": result_type,
                "structured_data": structured_data,
                "diagnostics": [],
            })
        }
    }
}

fn handle_type(
    req: &JsonValue,
    options: &Options,
    accumulator: &mut SessionAccumulator,
) -> JsonValue {
    let source = req["source"].as_str().unwrap_or("");
    match accumulator.compile_submission(source) {
        Ok(prepared_sub) => {
            let ty_name = if prepared_sub.kind == SubmissionKind::Expression {
                prepared_sub
                    .prepared
                    .models
                    .first()
                    .and_then(|m| {
                        m.expressions
                            .last()
                            .map(|e| bn_frontend::semantic::display(&e.ty))
                    })
                    .unwrap_or_else(|| "VOID".to_string())
            } else {
                "VOID".to_string()
            };
            json!({
                "ok": true,
                "type": ty_name,
            })
        }
        Err(diag) => {
            let source_file = SourceFile::new("<session-type>", source);
            json!({
                "ok": false,
                "diagnostics": [diagnostic_json(&diag, &source_file, options, "semantic", None)],
            })
        }
    }
}

fn handle_bindings(accumulator: &SessionAccumulator, session: &InteractiveSession) -> JsonValue {
    let mut variables = Vec::new();
    for (name, ty) in accumulator.variables() {
        let val_str = session
            .variables()
            .get(name)
            .map(|v| format!("{v:?}"))
            .unwrap_or_default();
        variables.push(json!({
            "name": name,
            "type": ty,
            "value": val_str,
        }));
    }
    json!({
        "ok": true,
        "variables": variables,
        "constants": accumulator.constants(),
        "declarations": accumulator.declarations(),
        "imports": accumulator.imports(),
    })
}

fn handle_policy(options: &Options, interpreter: &crate::options::InterpreterOptions) -> JsonValue {
    json!({
        "ok": true,
        "policy": {
            "sandbox": options.sandbox,
            "no_filesystem": !interpreter.filesystem,
            "read_roots": options.read_roots.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "write_roots": options.write_roots.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "module_paths": options.module_paths.iter().map(|m| m.path.display().to_string()).collect::<Vec<_>>(),
            "ffi_paths": interpreter.ffi_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        }
    })
}

fn handle_reset(
    accumulator: &mut SessionAccumulator,
    session: &mut InteractiveSession,
) -> JsonValue {
    accumulator.reset();
    session.reset();
    json!({"ok": true})
}

fn handle_ir(req: &JsonValue, accumulator: &mut SessionAccumulator) -> JsonValue {
    let source = req["source"].as_str().unwrap_or("");
    let ir_str = if source.trim().is_empty() {
        let exported = accumulator.export_program();
        match accumulator.compile_submission(&exported) {
            Ok(sub) => format!("{:?}", sub.prepared.validated),
            Err(_) => String::new(),
        }
    } else {
        match accumulator.compile_submission(source) {
            Ok(sub) => format!("{:?}", sub.prepared.validated),
            Err(_) => String::new(),
        }
    };
    json!({
        "ok": true,
        "ir": ir_str,
    })
}

fn handle_cfg(req: &JsonValue, accumulator: &mut SessionAccumulator) -> JsonValue {
    let source = req["source"].as_str().unwrap_or("");
    let compiled = if source.trim().is_empty() {
        let exported = accumulator.export_program();
        accumulator.compile_submission(&exported).ok()
    } else {
        accumulator.compile_submission(source).ok()
    };

    let mut blocks_json = Vec::new();
    if let Some(sub) = compiled {
        let module = sub.prepared.validated.as_module();
        if let Some(entry) = module.entry() {
            for block in &entry.blocks {
                let instrs: Vec<String> = block
                    .instructions
                    .iter()
                    .map(|i| format!("{i:?}"))
                    .collect();
                blocks_json.push(json!({
                    "id": block.id.0,
                    "instructions": instrs,
                    "terminator": format!("{:?}", block.terminator),
                }));
            }
        }
    }
    json!({
        "ok": true,
        "blocks": blocks_json,
    })
}

fn handle_arc(session: &InteractiveSession) -> JsonValue {
    let inspection = session.arc_inspection();
    let objects_json: Vec<JsonValue> = inspection
        .live_objects
        .iter()
        .map(|obj| {
            json!({
                "id": obj.id.bits(),
                "class": &obj.class,
                "strong": obj.strong,
                "status": if obj.strong > 0 { "ALIVE" } else { "RELEASED" },
            })
        })
        .collect();

    json!({
        "ok": true,
        "total_live_objects": inspection.total_live_objects,
        "live_memory_allocations": inspection.live_memory_allocations,
        "live_instances": inspection.live_instances,
        "objects": objects_json,
    })
}

fn handle_bench(
    req: &JsonValue,
    options: &Options,
    accumulator: &mut SessionAccumulator,
    session: &mut InteractiveSession,
) -> JsonValue {
    let source = req["source"].as_str().unwrap_or("");
    let iterations = req["iterations"].as_u64().unwrap_or(100).clamp(1, 10_000) as usize;
    let prepared_sub = match accumulator.compile_submission(source) {
        Ok(sub) => sub,
        Err(diag) => {
            let source_file = SourceFile::new("<session-bench>", source);
            return json!({
                "ok": false,
                "diagnostics": [diagnostic_json(&diag, &source_file, options, "semantic", None)],
            });
        }
    };

    let mut var_symbol_map = HashMap::new();
    if let Some(model) = prepared_sub.prepared.root_model() {
        for sym in &model.symbols {
            var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
        }
    }

    let initial_arc = session.arc_inspection();
    let mut times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let input = io::empty();
        let mut output = io::sink();
        let start = std::time::Instant::now();
        let _ = session.execute_submission(
            &prepared_sub.prepared.validated,
            &var_symbol_map,
            &mut io::BufReader::new(input),
            &mut output,
        );
        times.push(start.elapsed().as_nanos());
    }

    let final_arc = session.arc_inspection();
    times.sort_unstable();
    let total_ns: u128 = times.iter().sum();
    let min_ns = times.first().copied().unwrap_or(0);
    let median_ns = times.get(times.len() / 2).copied().unwrap_or(0);
    let mean_ns = if times.is_empty() {
        0
    } else {
        total_ns / (times.len() as u128)
    };

    json!({
        "ok": true,
        "iterations": iterations,
        "min_ns": min_ns,
        "median_ns": median_ns,
        "mean_ns": mean_ns,
        "total_ns": total_ns,
        "arc_allocations_delta": final_arc.live_memory_allocations.saturating_sub(initial_arc.live_memory_allocations),
        "live_objects_delta": final_arc.total_live_objects.saturating_sub(initial_arc.total_live_objects),
    })
}

fn handle_complete(
    req: &JsonValue,
    options: &Options,
    accumulator: &SessionAccumulator,
) -> JsonValue {
    let prefix = req["prefix"].as_str().unwrap_or("");
    let mut candidates = Vec::new();

    for kw in reserved_words() {
        if kw.starts_with(prefix) {
            candidates.push(json!({
                "name": kw,
                "kind": "keyword",
                "signature": kw,
            }));
        }
    }

    for (name, ty) in accumulator.variables() {
        if name.starts_with(prefix) {
            candidates.push(json!({
                "name": name,
                "kind": "variable",
                "signature": format!("{name} AS {ty}"),
            }));
        }
    }

    for root in &options.module_paths {
        if let Ok(entries) = std::fs::read_dir(&root.path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("bn")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && stem.starts_with(prefix)
                {
                    candidates.push(json!({
                        "name": stem,
                        "kind": "module",
                        "signature": stem,
                    }));
                }
            }
        }
    }

    json!({
        "ok": true,
        "candidates": candidates,
    })
}

fn handle_doc(req: &JsonValue, options: &Options) -> JsonValue {
    let symbol = req["symbol"].as_str().unwrap_or("");
    let mapped = if symbol.ends_with(".Fs") {
        symbol.replace(".Fs", ".FileSystem")
    } else if symbol == "Fs" {
        symbol.replace("Fs", "FileSystem")
    } else {
        symbol.to_string()
    };

    let mut methods = bn_frontend::host_member_names(&mapped);

    if methods.is_empty() {
        let module_filename = format!("{symbol}.bn");
        for root in &options.module_paths {
            let path = root.path.join(&module_filename);
            if path.is_file() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("EXPORT ") || trimmed.starts_with("PUBLIC ") {
                            methods.push(trimmed.to_string());
                        }
                    }
                }
                break;
            }
        }
    }

    json!({
        "ok": true,
        "symbol": symbol,
        "methods": methods,
    })
}

fn handle_export(accumulator: &SessionAccumulator) -> JsonValue {
    json!({
        "ok": true,
        "source": accumulator.export_program(),
    })
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub(crate) fn run_session_server(
    options: &Options,
    interpreter: &crate::options::InterpreterOptions,
) -> ExitCode {
    let host = match crate::environment::host_env(options, interpreter, Vec::new()) {
        Ok(host) => host,
        Err(message) => {
            let config_invalid = bn_diag::DiagId::CONFIG_INVALID.code();
            let error_json = json!({
                "schema_version": 1,
                "ok": false,
                "exit_code": 2,
                "diagnostics": [{
                    "code": config_invalid,
                    "severity": "error",
                    "phase": "config",
                    "message": message,
                }],
            });
            println!("{error_json}");
            return tool_error();
        }
    };

    let mut session = InteractiveSession::new(host);
    let mut accumulator = SessionAccumulator::new();
    accumulator.set_module_roots(options.module_paths.clone());

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(req) = serde_json::from_str::<JsonValue>(trimmed) else {
            let err_msg = bn_catalog::text("cli", "msg.session_invalid_json");
            let config_invalid = bn_diag::DiagId::CONFIG_INVALID.code();
            println!(
                "{}",
                json!({
                    "ok": false,
                    "diagnostics": [{
                        "code": config_invalid,
                        "severity": "error",
                        "phase": "protocol",
                        "message": err_msg,
                    }]
                })
            );
            let _ = io::stdout().flush();
            continue;
        };

        let Some(action) = req["action"].as_str() else {
            let err_msg = bn_catalog::text("cli", "msg.session_action_missing");
            let config_invalid = bn_diag::DiagId::CONFIG_INVALID.code();
            println!(
                "{}",
                json!({
                    "ok": false,
                    "diagnostics": [{
                        "code": config_invalid,
                        "severity": "error",
                        "phase": "protocol",
                        "message": err_msg,
                    }]
                })
            );
            let _ = io::stdout().flush();
            continue;
        };

        if action == "quit" {
            println!("{}", json!({"ok": true}));
            let _ = io::stdout().flush();
            break;
        }

        let resp = match action {
            "classify" => handle_classify(&req, options),
            "submit" => handle_submit(&req, options, &mut accumulator, &mut session),
            "type" => handle_type(&req, options, &mut accumulator),
            "bindings" => handle_bindings(&accumulator, &session),
            "policy" => handle_policy(options, interpreter),
            "reset" => handle_reset(&mut accumulator, &mut session),
            "ir" => handle_ir(&req, &mut accumulator),
            "cfg" => handle_cfg(&req, &mut accumulator),
            "arc" => handle_arc(&session),
            "bench" => handle_bench(&req, options, &mut accumulator, &mut session),
            "complete" => handle_complete(&req, options, &accumulator),
            "doc" => handle_doc(&req, options),
            "export" => handle_export(&accumulator),
            _ => {
                let err_msg = bn_catalog::text("cli", "msg.session_unknown_action");
                let config_invalid = bn_diag::DiagId::CONFIG_INVALID.code();
                json!({
                    "ok": false,
                    "diagnostics": [{
                        "code": config_invalid,
                        "severity": "error",
                        "phase": "protocol",
                        "message": err_msg,
                    }]
                })
            }
        };

        println!("{resp}");
        let _ = io::stdout().flush();
    }

    ExitCode::SUCCESS
}
