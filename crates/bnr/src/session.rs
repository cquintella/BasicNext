// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interactive session engine adapter for `bnr`.

use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    io,
    sync::{Arc, atomic::AtomicBool},
};

use bn_cli::{diagnostics::present_diagnostic, options::Options};
use bn_frontend::{
    SessionAccumulator, SubmissionCompleteness, SubmissionKind, classify_submission,
    token::reserved_words,
};
use bn_interp::{ArcInspection, InteractiveSession};
use bn_interpret_driver::{environment::host_env, options::InterpreterOptions};
use bn_ir::SymbolId;
use bn_source::SourceFile;

#[derive(Clone, Debug)]
pub struct BenchStats {
    pub iterations: usize,
    pub min_us: u128,
    pub median_us: u128,
    pub mean_us: u128,
    pub alloc_delta: usize,
}

#[derive(Clone, Debug)]
pub enum ExecutionOutcome {
    Success {
        stdout: String,
        result_value: Option<String>,
        result_type: Option<String>,
        structured_preview: Option<String>,
    },
    Failure {
        stdout: String,
        diagnostic: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct BindingInfo {
    pub name: String,
    pub type_name: String,
    pub status: String,
}

#[derive(Debug)]
pub struct ReplSession {
    accumulator: SessionAccumulator,
    session: InteractiveSession,
    cancel_token: Arc<AtomicBool>,
}

impl ReplSession {
    /// Creates a new interactive REPL session from options.
    ///
    /// # Errors
    ///
    /// Returns configuration error string if host environment fails to initialize.
    pub fn new(options: &Options, interpreter: &InterpreterOptions) -> Result<Self, String> {
        let host = host_env(options, interpreter, Vec::new())?;
        let session = InteractiveSession::new(host);
        let cancel_token = session.cancel_flag();
        let mut accumulator = SessionAccumulator::new();
        accumulator.set_module_roots(options.module_paths.clone());
        Ok(Self {
            accumulator,
            session,
            cancel_token,
        })
    }

    #[must_use]
    pub fn cancel_token(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_token)
    }

    #[must_use]
    pub fn classify(&self, source: &str) -> SubmissionCompleteness {
        classify_submission(source)
    }

    #[must_use]
    pub fn arc_inspection(&self) -> ArcInspection {
        self.session.arc_inspection()
    }

    #[must_use]
    pub fn export_program(&self) -> String {
        self.accumulator.export_program()
    }

    #[allow(clippy::too_many_lines)]
    pub fn execute(&mut self, source: &str, options: &Options) -> ExecutionOutcome {
        let prepared_sub = match self.accumulator.compile_submission(source) {
            Ok(sub) => sub,
            Err(diag) => {
                let source_file = SourceFile::new("<session>", source);
                let formatted = present_diagnostic(&diag, &source_file, None, options);
                return ExecutionOutcome::Failure {
                    stdout: String::new(),
                    diagnostic: Some(formatted),
                };
            }
        };

        if prepared_sub.kind == SubmissionKind::Expression {
            let uses_underscore = source.contains('_')
                && {
                    let sf = SourceFile::new("check_underscore.bn", source);
                    bn_frontend::lexer::lex(&sf).is_ok_and(|toks| {
                        toks.iter().any(|t| {
                            matches!(&t.kind, bn_frontend::token::TokenKind::Identifier(name) if name == "_")
                        })
                    })
                };
            if !uses_underscore {
                self.session.remove_variable("_");
            }
        }

        let mut var_symbol_map = HashMap::new();
        if let Some(model) = prepared_sub.prepared.root_model() {
            for sym in &model.symbols {
                var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
            }
        }

        let input = io::empty();
        let mut stdout_buf = Vec::new();
        let exec_res = self.session.execute_submission(
            &prepared_sub.prepared.validated,
            &var_symbol_map,
            &mut io::BufReader::new(input),
            &mut stdout_buf,
        );

        let stdout = String::from_utf8_lossy(&stdout_buf).to_string();
        match exec_res {
            Err(diag) => {
                let source_file = SourceFile::new("<session>", source);
                let formatted = present_diagnostic(&diag, &source_file, None, options);
                ExecutionOutcome::Failure {
                    stdout,
                    diagnostic: Some(formatted),
                }
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
                self.accumulator.commit_turn(source, new_var_types);

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
                            if !stdout.is_empty() {
                                Some(stdout.trim_end_matches(['\r', '\n']).to_string())
                            } else {
                                None
                            }
                        })
                } else {
                    None
                };

                let mut structured_preview = None;
                if prepared_sub.kind == SubmissionKind::Expression
                    && let Some(ty) = result_type.as_deref()
                {
                    if let Some(val) = res.updated_variables.get("_") {
                        structured_preview = crate::format::format_structured_value(val, ty);
                    }
                    if structured_preview.is_none() && ty.contains("Json") {
                        structured_preview = self.preview_json();
                    }
                    if structured_preview.is_none() && ty.contains("DataFrame") {
                        structured_preview = self.preview_dataframe();
                    }
                }

                ExecutionOutcome::Success {
                    stdout,
                    result_value,
                    result_type,
                    structured_preview,
                }
            }
        }
    }

    fn preview_json(&mut self) -> Option<String> {
        let query_src = bn_catalog::text("cli", "bnr.query.json_stringify");
        let sub = self.accumulator.compile_submission(query_src).ok()?;
        let mut var_symbol_map = HashMap::new();
        if let Some(model) = sub.prepared.root_model() {
            for sym in &model.symbols {
                var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
            }
        }
        let input = io::empty();
        let mut stdout_buf = Vec::new();
        let _ = self.session.execute_submission(
            &sub.prepared.validated,
            &var_symbol_map,
            &mut io::BufReader::new(input),
            &mut stdout_buf,
        );
        let raw = String::from_utf8_lossy(&stdout_buf).to_string();
        crate::format::format_pretty_json(raw.trim())
    }

    fn preview_dataframe(&mut self) -> Option<String> {
        let query_src = bn_catalog::text("cli", "bnr.query.dataframe_preview");
        let sub = self.accumulator.compile_submission(query_src).ok()?;
        let mut var_symbol_map = HashMap::new();
        if let Some(model) = sub.prepared.root_model() {
            for sym in &model.symbols {
                var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
            }
        }
        let input = io::empty();
        let mut stdout_buf = Vec::new();
        let _ = self.session.execute_submission(
            &sub.prepared.validated,
            &var_symbol_map,
            &mut io::BufReader::new(input),
            &mut stdout_buf,
        );
        let output = String::from_utf8_lossy(&stdout_buf);
        let mut lines = output.lines();
        let row_count: usize = lines.next()?.parse().ok()?;
        let col_count: usize = lines.next()?.parse().ok()?;
        let mut col_names = Vec::with_capacity(col_count);
        let mut col_types = Vec::with_capacity(col_count);
        for _ in 0..col_count {
            col_names.push(lines.next()?.to_string());
            col_types.push(lines.next()?.to_string());
        }
        let preview_row_count = row_count.min(5);
        let mut rows = Vec::with_capacity(preview_row_count);
        for _ in 0..preview_row_count {
            let mut row = Vec::with_capacity(col_count);
            for _ in 0..col_count {
                row.push(lines.next().unwrap_or("").to_string());
            }
            rows.push(row);
        }
        Some(crate::format::format_dataframe_table(
            &col_names, &col_types, &rows, row_count, col_count,
        ))
    }

    /// Resolves static type of an expression in the current session context.
    ///
    /// # Errors
    ///
    /// Returns formatted diagnostic if expression is invalid.
    pub fn type_of(&mut self, expr: &str, options: &Options) -> Result<String, String> {
        let prepared_sub = self.accumulator.compile_submission(expr).map_err(|diag| {
            let source_file = SourceFile::new("<session-type>", expr);
            present_diagnostic(&diag, &source_file, None, options)
        })?;
        if prepared_sub.kind == SubmissionKind::Expression {
            let ty_name = prepared_sub
                .prepared
                .models
                .first()
                .and_then(|m| {
                    m.expressions
                        .last()
                        .map(|e| bn_frontend::semantic::display(&e.ty))
                })
                .unwrap_or_else(|| "VOID".to_string());
            return Ok(ty_name);
        }
        Ok("VOID".to_string())
    }

    /// Returns list of active variables and their types.
    #[must_use]
    pub fn bindings(&self) -> Vec<BindingInfo> {
        let _inspection = self.session.arc_inspection();
        self.accumulator
            .variables()
            .iter()
            .map(|(name, ty)| {
                let status = "ALIVE";
                BindingInfo {
                    name: name.clone(),
                    type_name: ty.clone(),
                    status: status.to_string(),
                }
            })
            .collect()
    }

    /// Resets all variables, declarations, and runtime state.
    ///
    /// # Errors
    ///
    /// Returns configuration error string if host re-initialization fails.
    pub fn reset(
        &mut self,
        options: &Options,
        interpreter: &InterpreterOptions,
    ) -> Result<(), String> {
        self.accumulator.reset();
        let host = host_env(options, interpreter, Vec::new())?;
        self.session = InteractiveSession::new(host);
        self.cancel_token = self.session.cancel_flag();
        Ok(())
    }

    /// Generates validated BN IR for a submission or the entire active session.
    pub fn ir_of(&mut self, target: &str) -> String {
        let source = if target.trim().is_empty() {
            self.accumulator.export_program()
        } else {
            target.to_string()
        };
        match self.accumulator.compile_submission(&source) {
            Ok(sub) => format!("{:?}", sub.prepared.validated),
            Err(_) => String::new(),
        }
    }

    /// Generates control flow graph summary for a submission or the active session.
    pub fn cfg_of(&mut self, target: &str) -> String {
        let source = if target.trim().is_empty() {
            self.accumulator.export_program()
        } else {
            target.to_string()
        };
        let mut output = String::new();
        if let Ok(sub) = self.accumulator.compile_submission(&source) {
            let module = sub.prepared.validated.as_module();
            if let Some(entry) = module.entry() {
                for block in &entry.blocks {
                    let _ = writeln!(output, "  block #{}:", block.id.0);
                    for instr in &block.instructions {
                        let _ = writeln!(output, "    {instr:?}");
                    }
                    let _ = writeln!(output, "    -> {:?}", block.terminator);
                }
            }
        }
        output
    }

    /// Micro-benchmarks an expression and measures execution times and ARC delta.
    ///
    /// # Errors
    ///
    /// Returns formatted diagnostic if compilation fails.
    pub fn bench(
        &mut self,
        expr: &str,
        iterations: usize,
        options: &Options,
    ) -> Result<BenchStats, String> {
        let prepared_sub = self.accumulator.compile_submission(expr).map_err(|diag| {
            let source_file = SourceFile::new("<session-bench>", expr);
            present_diagnostic(&diag, &source_file, None, options)
        })?;

        let mut var_symbol_map = HashMap::new();
        if let Some(model) = prepared_sub.prepared.root_model() {
            for sym in &model.symbols {
                var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
            }
        }

        let initial_arc = self.session.arc_inspection();
        let mut times = Vec::with_capacity(iterations);

        for _ in 0..iterations {
            let input = io::empty();
            let mut output = io::sink();
            let start = std::time::Instant::now();
            let _ = self.session.execute_submission(
                &prepared_sub.prepared.validated,
                &var_symbol_map,
                &mut io::BufReader::new(input),
                &mut output,
            );
            times.push(start.elapsed().as_micros());
        }

        let final_arc = self.session.arc_inspection();
        times.sort_unstable();
        let total_us: u128 = times.iter().sum();
        let min_us = times.first().copied().unwrap_or(0);
        let median_us = times.get(times.len() / 2).copied().unwrap_or(0);
        let mean_us = if times.is_empty() {
            0
        } else {
            total_us / (times.len() as u128)
        };
        let alloc_delta = final_arc
            .live_memory_allocations
            .saturating_sub(initial_arc.live_memory_allocations);

        Ok(BenchStats {
            iterations,
            min_us,
            median_us,
            mean_us,
            alloc_delta,
        })
    }

    /// Returns completion candidates for a given prefix.
    #[must_use]
    pub fn complete_candidates(&self, prefix: &str, options: &Options) -> Vec<String> {
        let mut candidates = Vec::new();
        for kw in reserved_words() {
            if kw.starts_with(prefix) {
                candidates.push(kw.to_string());
            }
        }
        for name in self.accumulator.variables().keys() {
            if name.starts_with(prefix) {
                candidates.push(name.clone());
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
                        candidates.push(stem.to_string());
                    }
                }
            }
        }
        candidates
    }

    /// Queries member documentation for a symbol or module.
    #[must_use]
    pub fn doc_symbol(&self, symbol: &str, options: &Options) -> Vec<String> {
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
        methods
    }
}
