// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Interactive session engine adapter for `bnr`.

use std::{
    collections::{BTreeMap, HashMap},
    io,
    sync::{Arc, atomic::AtomicBool},
};

use bn_cli::{diagnostics::present_diagnostic, options::Options};
use bn_frontend::{
    SessionAccumulator, SubmissionCompleteness, SubmissionKind, classify_submission,
};
use bn_interp::InteractiveSession;
use bn_interpret_driver::{environment::host_env, options::InterpreterOptions};
use bn_ir::SymbolId;
use bn_source::SourceFile;

#[derive(Clone, Debug)]
pub enum ExecutionOutcome {
    Success {
        stdout: String,
        result_value: Option<String>,
        result_type: Option<String>,
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

        let mut var_symbol_map = HashMap::new();
        if let Some(model) = prepared_sub.prepared.models.first() {
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
                if let Some(model) = prepared_sub.prepared.models.first() {
                    for name in res.updated_variables.keys() {
                        if let Some(sym) = model.symbols.iter().rev().find(|s| &s.name == name) {
                            new_var_types.insert(name.clone(), sym.type_name.clone());
                        }
                    }
                }
                self.accumulator.commit_turn(source, new_var_types);

                let result_type = if prepared_sub.kind == SubmissionKind::Expression {
                    prepared_sub.prepared.models.first().and_then(|m| {
                        m.expressions
                            .last()
                            .map(|e| bn_frontend::semantic::display(&e.ty))
                    })
                } else {
                    None
                };
                let result_value =
                    if prepared_sub.kind == SubmissionKind::Expression && !stdout.is_empty() {
                        Some(stdout.trim_end_matches(['\r', '\n']).to_string())
                    } else {
                        None
                    };

                ExecutionOutcome::Success {
                    stdout,
                    result_value,
                    result_type,
                }
            }
        }
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
}
