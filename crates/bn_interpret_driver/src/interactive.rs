// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Shared interactive turn execution coordinator.
//!
//! Coordinates compilation, execution, variable tracking, and turn commitment
//! for multi-turn interactive environments (such as the Jupyter session server
//! in `bni` and the interactive terminal REPL in `bnr`).

use std::collections::{BTreeMap, HashMap};
use std::io::{self, BufReader, Read, Write};

use bn_diag::Diagnostic;
use bn_frontend::{SessionAccumulator, SubmissionKind};
use bn_interp::session::InteractiveSession;
use bn_ir::SymbolId;
use bn_source::SourceFile;
use bn_value::Value;

/// Result of a successfully executed interactive turn.
#[derive(Clone, Debug)]
pub struct TurnSuccess {
    /// Process exit code returned by the turn execution (0 for normal completion).
    pub exit_code: u8,
    /// Standard output captured during the turn execution.
    pub stdout: String,
    /// Rendered string representation of the expression result or printed output, if any.
    pub result_value: Option<String>,
    /// Display name of the expression result type, if the submission was an expression.
    pub result_type: Option<String>,
    /// Raw evaluated value returned by the execution, if available.
    pub returned_value: Option<Value>,
    /// Variables updated or introduced during this turn.
    pub updated_variables: BTreeMap<String, Value>,
    /// Classification of the submission (e.g. Expression, Statement, Declaration).
    pub submission_kind: SubmissionKind,
}

/// Outcome of attempting to execute an interactive turn.
#[derive(Debug)]
pub enum TurnOutcome {
    /// Turn compiled and executed to completion.
    Success(TurnSuccess),
    /// Turn failed at compilation (parsing, semantic check, IR lowering).
    CompilationFailure(Diagnostic),
    /// Turn compiled successfully but failed during evaluation.
    RuntimeFailure {
        /// Standard output produced before the runtime error occurred.
        stdout: String,
        /// The runtime diagnostic describing the failure.
        diagnostic: Diagnostic,
    },
}

/// Unified interactive turn execution coordinator.
#[derive(Debug)]
pub struct InteractiveCoordinator;

impl InteractiveCoordinator {
    /// Executes a single interactive turn with empty input and internal stdout capture.
    pub fn execute_turn(
        accumulator: &mut SessionAccumulator,
        session: &mut InteractiveSession,
        source: &str,
    ) -> TurnOutcome {
        Self::execute_turn_with_io(accumulator, session, source, io::empty(), Vec::new())
    }

    /// Executes a single interactive turn using the provided input reader and output writer.
    pub fn execute_turn_with_io<R: Read, W: Write>(
        accumulator: &mut SessionAccumulator,
        session: &mut InteractiveSession,
        source: &str,
        input: R,
        mut output: W,
    ) -> TurnOutcome {
        let prepared_sub = match accumulator.compile_submission(source) {
            Ok(sub) => sub,
            Err(diag) => return TurnOutcome::CompilationFailure(diag),
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
                session.remove_variable("_");
            }
        }

        let mut var_symbol_map = HashMap::new();
        if let Some(model) = prepared_sub.prepared.root_model() {
            for sym in &model.symbols {
                var_symbol_map.insert(sym.name.clone(), SymbolId::from_raw(sym.id.value()));
            }
        }

        let mut stdout_buf = Vec::new();
        let exec_res = session.execute_submission(
            &prepared_sub.prepared.validated,
            &var_symbol_map,
            &mut BufReader::new(input),
            &mut stdout_buf,
        );

        let _ = output.write_all(&stdout_buf);
        let stdout = String::from_utf8_lossy(&stdout_buf).to_string();

        match exec_res {
            Err(diag) => TurnOutcome::RuntimeFailure {
                stdout,
                diagnostic: diag,
            },
            Ok(res) => {
                let mut new_var_types = BTreeMap::new();
                if let Some(model) = prepared_sub.prepared.root_model() {
                    for name in res.updated_variables.keys() {
                        if let Some(sym) = model.symbols.iter().rev().find(|s| &s.name == name) {
                            new_var_types.insert(name.clone(), sym.type_name.clone());
                        }
                    }
                    for sym in &model.symbols {
                        if !res.updated_variables.contains_key(&sym.name) {
                            accumulator.remove_variable(&sym.name);
                        }
                    }
                }
                accumulator.commit_turn(source, new_var_types);

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

                TurnOutcome::Success(TurnSuccess {
                    exit_code: res.exit_code,
                    stdout,
                    result_value,
                    result_type,
                    returned_value: res.returned_value,
                    updated_variables: res.updated_variables,
                    submission_kind: prepared_sub.kind,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bn_interp::HostEnv;

    fn test_environment() -> (SessionAccumulator, InteractiveSession) {
        let host = HostEnv::sandbox(Vec::new());
        let session = InteractiveSession::new(host);
        let accumulator = SessionAccumulator::new();
        (accumulator, session)
    }

    #[test]
    fn coordinator_expression_turn_success() {
        let (mut accumulator, mut session) = test_environment();

        // Turn 1: variable declaration
        let out1 = InteractiveCoordinator::execute_turn(
            &mut accumulator,
            &mut session,
            "LET x AS INTEGER = 10",
        );
        match out1 {
            TurnOutcome::Success(res) => {
                assert_eq!(res.exit_code, 0);
                assert_eq!(res.submission_kind, SubmissionKind::Statement);
                assert_eq!(res.result_type, None);
            }
            other => panic!("expected success, got {other:?}"),
        }

        // Turn 2: expression using variable
        let out2 = InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "x + 5");
        match out2 {
            TurnOutcome::Success(res) => {
                assert_eq!(res.exit_code, 0);
                assert_eq!(res.submission_kind, SubmissionKind::Expression);
                assert_eq!(res.result_value.as_deref(), Some("15"));
                assert_eq!(res.result_type.as_deref(), Some("INT32"));
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn coordinator_compilation_failure_leaves_accumulator_intact() {
        let (mut accumulator, mut session) = test_environment();

        let out1 = InteractiveCoordinator::execute_turn(
            &mut accumulator,
            &mut session,
            "LET val AS INTEGER = 42",
        );
        assert!(matches!(out1, TurnOutcome::Success(_)));

        // Invalid turn: syntax error
        let out2 = InteractiveCoordinator::execute_turn(
            &mut accumulator,
            &mut session,
            "LET broken AS INTEGER =",
        );
        assert!(matches!(out2, TurnOutcome::CompilationFailure(_)));

        // Subsequent turn can still read prior valid variable
        let out3 = InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "val * 2");
        match out3 {
            TurnOutcome::Success(res) => {
                assert_eq!(res.result_value.as_deref(), Some("84"));
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn coordinator_runtime_failure_is_reported() {
        let (mut accumulator, mut session) = test_environment();

        let out = InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "10 DIV 0");
        match out {
            TurnOutcome::RuntimeFailure {
                stdout: _,
                diagnostic,
            } => {
                assert_eq!(diagnostic.code, "DIVISION_BY_ZERO");
            }
            other => panic!("expected runtime failure, got {other:?}"),
        }
    }

    #[test]
    fn coordinator_captures_stdout_from_print() {
        let (mut accumulator, mut session) = test_environment();

        let out = InteractiveCoordinator::execute_turn(
            &mut accumulator,
            &mut session,
            "PRINT \"hello coordinator\"",
        );
        match out {
            TurnOutcome::Success(res) => {
                assert_eq!(res.stdout.trim(), "hello coordinator");
                assert_eq!(res.submission_kind, SubmissionKind::Statement);
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn coordinator_expression_resets_underscore_unless_referenced() {
        let (mut accumulator, mut session) = test_environment();

        // Turn 1: 10 + 20 -> _ = 30
        let out1 = InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "10 + 20");
        match out1 {
            TurnOutcome::Success(res) => {
                assert_eq!(res.result_value.as_deref(), Some("30"));
            }
            other => panic!("expected success, got {other:?}"),
        }

        // Turn 2: references _ -> _ + 5 = 35
        let out2 = InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "_ + 5");
        match out2 {
            TurnOutcome::Success(res) => {
                assert_eq!(res.result_value.as_deref(), Some("35"));
            }
            other => panic!("expected success, got {other:?}"),
        }
    }

    #[test]
    fn coordinator_reassignment_and_new_bindings() {
        let (mut accumulator, mut session) = test_environment();

        let out1 = InteractiveCoordinator::execute_turn(
            &mut accumulator,
            &mut session,
            "LET val AS INT32 = 42",
        );
        assert!(matches!(out1, TurnOutcome::Success(_)));
        assert_eq!(
            accumulator.variables().get("val"),
            Some(&"INT32".to_string())
        );

        // Reassign existing variable
        let out2 =
            InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "val = 100");
        assert!(matches!(out2, TurnOutcome::Success(_)));

        // Read updated variable
        let out3 = InteractiveCoordinator::execute_turn(&mut accumulator, &mut session, "val + 1");
        match out3 {
            TurnOutcome::Success(res) => {
                assert_eq!(res.result_value.as_deref(), Some("101"));
            }
            other => panic!("expected success, got {other:?}"),
        }

        // Introduce second variable
        let out4 = InteractiveCoordinator::execute_turn(
            &mut accumulator,
            &mut session,
            "LET greeting AS STRING = \"hello\"",
        );
        assert!(matches!(out4, TurnOutcome::Success(_)));
        assert_eq!(
            accumulator.variables().get("greeting"),
            Some(&"STRING".to_string())
        );
        assert!(accumulator.variables().contains_key("val"));
        assert!(accumulator.variables().contains_key("greeting"));
        assert!(accumulator.variables().contains_key("_"));
    }
}
