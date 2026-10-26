// Basic Next interactive session symbol accumulator (Cluster A.2)
// Accumulates module declarations and variables across REPL / interactive turns
// while compiling each submission into validated BN IR before execution (W1–W5).

use std::{collections::BTreeMap, path::PathBuf};

use bn_diag::Diagnostic;
use bn_source::SourceFile;

use crate::{
    ast::Expression,
    frontend_session::FrontendSession,
    lexer::lex,
    module_graph::ModuleRoot,
    parser::parse_expression,
    prepare::{Prepared, prepare},
    session::trim_newlines,
    token::{Symbol, Token, TokenKind},
};

/// Mode of compilation of a single interactive turn.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubmissionKind {
    Expression,
    Statement,
    Declaration,
    Import,
    Empty,
}

/// Artifact prepared by the session accumulator ready for interpretation.
#[derive(Debug)]
pub struct PreparedSubmission {
    pub kind: SubmissionKind,
    pub prepared: Prepared,
    pub result_expression: Option<Expression>,
}

/// State of variable and symbol bindings across turns in an interactive session.
#[derive(Debug, Default)]
pub struct SessionAccumulator {
    frontend_session: FrontendSession,
    module_roots: Vec<ModuleRoot>,
    imports: Vec<String>,
    constants: Vec<String>,
    declarations: Vec<String>,
    variables: BTreeMap<String, String>,
    history: Vec<String>,
    turn: usize,
}

impl SessionAccumulator {
    /// Creates a fresh session accumulator with no prior declarations.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets explicit module search roots for resolving imported modules.
    pub fn set_module_roots(&mut self, roots: Vec<ModuleRoot>) {
        self.module_roots = roots;
    }

    /// Current turn sequence counter (starts at 0).
    #[must_use]
    pub fn turn_count(&self) -> usize {
        self.turn
    }

    /// Returns the live variable bindings in the session (name -> type).
    #[must_use]
    pub fn variables(&self) -> &BTreeMap<String, String> {
        &self.variables
    }

    /// Returns accumulated history of committed submissions.
    #[must_use]
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Returns accumulated imports across session turns.
    #[must_use]
    pub fn imports(&self) -> &[String] {
        &self.imports
    }

    /// Returns accumulated constants across session turns.
    #[must_use]
    pub fn constants(&self) -> &[String] {
        &self.constants
    }

    /// Returns accumulated declarations across session turns.
    #[must_use]
    pub fn declarations(&self) -> &[String] {
        &self.declarations
    }

    /// Resets all accumulated variables, declarations, and history.
    pub fn reset(&mut self) {
        self.imports.clear();
        self.constants.clear();
        self.declarations.clear();
        self.variables.clear();
        self.history.clear();
        self.turn = 0;
        self.frontend_session = FrontendSession::default();
    }

    /// Commits the successful result of an executed turn into session history and active bindings.
    pub fn commit_turn(&mut self, source: &str, new_variables: BTreeMap<String, String>) {
        let trimmed = source.trim();
        if trimmed.starts_with("IMPORT ") {
            if !self.imports.iter().any(|i| i == trimmed) {
                self.imports.push(trimmed.to_string());
            }
        } else if trimmed.starts_with("CONST ") {
            self.constants.push(trimmed.to_string());
        } else if trimmed.starts_with("FUNCTION ")
            || trimmed.starts_with("CLASS ")
            || trimmed.starts_with("STRUCT ")
            || trimmed.starts_with("INTERFACE ")
            || trimmed.starts_with("ENUM ")
        {
            if let Some(decl_name) = extract_decl_name(trimmed) {
                self.declarations
                    .retain(|d| extract_decl_name(d).as_deref() != Some(&decl_name));
            }
            self.declarations.push(trimmed.to_string());
        }
        for (name, ty) in new_variables {
            self.variables.insert(name, ty);
        }
        self.history.push(source.to_string());
        self.turn += 1;
    }

    /// Removes a variable from the accumulated session symbols.
    pub fn remove_variable(&mut self, name: &str) {
        self.variables.remove(name);
    }

    /// Compiles a submission into a fully validated `Prepared` artifact, classifying its submission kind.
    ///
    /// # Errors
    ///
    /// Returns a diagnostic if the submission fails parsing, semantic analysis, or IR validation.
    #[allow(clippy::too_many_lines)]
    pub fn compile_submission(
        &mut self,
        submission_source: &str,
    ) -> Result<PreparedSubmission, Diagnostic> {
        let trimmed = submission_source.trim();
        if trimmed.is_empty() {
            let synthetic = self.synthesize_program("");
            return self.prepare_synthetic(synthetic, SubmissionKind::Empty, None);
        }

        if trimmed.starts_with("IMPORT ") {
            let synthetic = self.synthesize_program_with_decl(trimmed, "");
            return self.prepare_synthetic(synthetic, SubmissionKind::Import, None);
        }

        let is_const = trimmed.starts_with("CONST ");
        let is_decl = trimmed.starts_with("FUNCTION ")
            || trimmed.starts_with("CLASS ")
            || trimmed.starts_with("STRUCT ")
            || trimmed.starts_with("INTERFACE ")
            || trimmed.starts_with("ENUM ");

        if is_const {
            let synthetic = self.synthesize_program_with_decl(trimmed, "");
            return self.prepare_synthetic(synthetic, SubmissionKind::Declaration, None);
        }

        if is_decl {
            let synthetic = self.synthesize_program_with_decl(trimmed, "");
            return self.prepare_synthetic(synthetic, SubmissionKind::Declaration, None);
        }

        // Standalone expression check
        let source_file = SourceFile::new("repl_expr.bn", submission_source);
        if let Ok(tokens) = lex(&source_file) {
            let expr_tokens = trim_newlines(&tokens);
            if !is_assignment_statement(expr_tokens)
                && let Ok(expr) = parse_expression(expr_tokens)
                && !trimmed.starts_with("LET ")
                && !trimmed.starts_with("PRINT ")
                && !trimmed.starts_with("IF ")
                && !trimmed.starts_with("WHILE ")
                && !trimmed.starts_with("FOR ")
                && !trimmed.starts_with("REPEAT ")
                && !trimmed.starts_with("DELETE ")
                && !trimmed.starts_with("RELEASE ")
            {
                let uses_underscore = expr_tokens
                    .iter()
                    .any(|t| matches!(&t.kind, TokenKind::Identifier(name) if name == "_"));

                let probe_body = format!("    PRINT ({trimmed})\n");
                let synthetic_probe = self.synthesize_program(&probe_body);
                let expr_offset = synthetic_probe.rfind(trimmed).unwrap_or(0);
                if let Ok(prepared_probe) =
                    self.prepare_synthetic(synthetic_probe, SubmissionKind::Expression, None)
                {
                    let ty_str = prepared_probe.prepared.root_model().and_then(|m| {
                        m.expressions
                            .iter()
                            .filter(|e| e.span.start.offset >= expr_offset)
                            .max_by_key(|e| e.span.end.offset.saturating_sub(e.span.start.offset))
                            .map(|e| crate::semantic::display(&e.ty))
                    });

                    if let Some(ty) = ty_str {
                        if ty == "VOID" {
                            let mut start_body = String::new();
                            for line in submission_source.lines() {
                                start_body.push_str("    ");
                                start_body.push_str(line);
                                start_body.push('\n');
                            }
                            let synthetic = self.synthesize_program(&start_body);
                            return self.prepare_synthetic(
                                synthetic,
                                SubmissionKind::Statement,
                                None,
                            );
                        }
                        let mut final_body = String::new();
                        if uses_underscore
                            && self.variables.get("_").map(String::as_str) == Some(&ty)
                        {
                            final_body.push_str("    _ = (");
                            final_body.push_str(trimmed);
                            final_body.push_str(")\n");
                        } else if !uses_underscore {
                            push_variable_decl(&mut final_body, "_", &ty);
                            final_body.pop();
                            final_body.push_str(" = (");
                            final_body.push_str(trimmed);
                            final_body.push_str(")\n");
                        } else {
                            final_body.push_str("    PRINT (");
                            final_body.push_str(trimmed);
                            final_body.push_str(")\n");
                        }

                        let synthetic_final = if uses_underscore {
                            self.synthesize_program(&final_body)
                        } else {
                            self.synthesize_program_excluding(Some("_"), "", &final_body)
                        };

                        if let Ok(mut prepared_final) = self.prepare_synthetic(
                            synthetic_final,
                            SubmissionKind::Expression,
                            None,
                        ) {
                            prepared_final.result_expression = Some(expr);
                            return Ok(prepared_final);
                        }
                    }
                    return Ok(PreparedSubmission {
                        kind: SubmissionKind::Expression,
                        prepared: prepared_probe.prepared,
                        result_expression: Some(expr),
                    });
                }
            }
        }

        // Otherwise compile as a sequence of statements inside Start
        let mut start_body = String::new();
        for line in submission_source.lines() {
            start_body.push_str("    ");
            start_body.push_str(line);
            start_body.push('\n');
        }
        let synthetic = self.synthesize_program(&start_body);
        self.prepare_synthetic(synthetic, SubmissionKind::Statement, None)
    }

    fn synthesize_program(&self, start_statements: &str) -> String {
        self.synthesize_program_excluding(None, "", start_statements)
    }

    fn synthesize_program_with_decl(&self, extra_decl: &str, start_statements: &str) -> String {
        self.synthesize_program_excluding(None, extra_decl, start_statements)
    }

    fn synthesize_program_excluding(
        &self,
        excluded: Option<&str>,
        extra_decl: &str,
        start_statements: &str,
    ) -> String {
        let mut buffer = String::new();
        for import in &self.imports {
            buffer.push_str(import);
            buffer.push('\n');
        }
        for constant in &self.constants {
            buffer.push_str(constant);
            buffer.push('\n');
        }
        for decl in &self.declarations {
            buffer.push_str(decl);
            buffer.push('\n');
        }
        if !extra_decl.is_empty() {
            buffer.push_str(extra_decl);
            buffer.push('\n');
        }
        push_start_header(&mut buffer);
        for (name, ty) in &self.variables {
            if Some(name.as_str()) != excluded {
                push_variable_decl(&mut buffer, name, ty);
            }
        }
        buffer.push_str(start_statements);
        push_end_function(&mut buffer);
        buffer
    }

    fn prepare_synthetic(
        &mut self,
        synthetic_source: String,
        kind: SubmissionKind,
        result_expression: Option<Expression>,
    ) -> Result<PreparedSubmission, Diagnostic> {
        let entry_path = PathBuf::from("session.bn");
        let mut overlays = BTreeMap::new();
        overlays.insert(entry_path.clone(), synthetic_source);

        let prepared = prepare(
            &entry_path,
            &mut self.frontend_session,
            &overlays,
            &self.module_roots,
        )
        .map_err(crate::prepare::PrepareError::into_diagnostic)?;

        Ok(PreparedSubmission {
            kind,
            prepared,
            result_expression,
        })
    }

    /// Reconstructs a full Basic Next source file representing the accumulated session.
    #[must_use]
    pub fn export_program(&self) -> String {
        let mut buffer = String::new();
        for import in &self.imports {
            buffer.push_str(import);
            buffer.push('\n');
        }
        if !self.imports.is_empty() {
            buffer.push('\n');
        }
        for constant in &self.constants {
            buffer.push_str(constant);
            buffer.push('\n');
        }
        for decl in &self.declarations {
            buffer.push_str(decl);
            buffer.push('\n');
            buffer.push('\n');
        }
        let has_user_start = self
            .declarations
            .iter()
            .any(|d| extract_decl_name(d).as_deref() == Some("START"));
        if !has_user_start {
            push_export_start_header(&mut buffer);
            for turn_code in &self.history {
                let trimmed = turn_code.trim();
                if trimmed.starts_with("IMPORT ")
                    || trimmed.starts_with("CONST ")
                    || trimmed.starts_with("FUNCTION ")
                    || trimmed.starts_with("CLASS ")
                    || trimmed.starts_with("STRUCT ")
                    || trimmed.starts_with("INTERFACE ")
                    || trimmed.starts_with("ENUM ")
                {
                    continue;
                }
                for line in turn_code.lines() {
                    buffer.push_str("    ");
                    buffer.push_str(line);
                    buffer.push('\n');
                }
            }
            push_end_function(&mut buffer);
        }
        buffer
    }
}

fn is_assignment_statement(tokens: &[Token]) -> bool {
    match tokens.first().map(|t| &t.kind) {
        Some(TokenKind::Identifier(_)) => {}
        Some(TokenKind::Keyword(kw)) if kw != "LET" && kw != "CONST" => {}
        _ => return false,
    }
    let mut depth = 0;
    for token in tokens {
        match &token.kind {
            TokenKind::Symbol(Symbol::LeftParen | Symbol::LeftBracket) => depth += 1,
            TokenKind::Symbol(Symbol::RightParen | Symbol::RightBracket) if depth > 0 => depth -= 1,
            TokenKind::Symbol(
                Symbol::Assign
                | Symbol::PlusAssign
                | Symbol::MinusAssign
                | Symbol::StarAssign
                | Symbol::SlashAssign
                | Symbol::PercentAssign
                | Symbol::PowerAssign,
            ) if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

fn push_start_header(buffer: &mut String) {
    buffer.push_str("FUNCTION");
    buffer.push(' ');
    buffer.push_str(bn_ir::names::SESSION_ENTRY);
    buffer.push_str("()");
    buffer.push_str(" AS");
    buffer.push_str(" VOID\n");
}

fn push_export_start_header(buffer: &mut String) {
    buffer.push_str("FUNCTION");
    buffer.push_str(" Start()");
    buffer.push_str(" AS");
    buffer.push_str(" VOID\n");
}

fn push_end_function(buffer: &mut String) {
    buffer.push_str("END");
    buffer.push_str(" FUNCTION\n");
}

fn push_variable_decl(buffer: &mut String, name: &str, ty: &str) {
    buffer.push_str("    LET ");
    buffer.push_str(name);
    buffer.push_str(" AS ");
    buffer.push_str(ty);
    buffer.push('\n');
}

fn extract_decl_name(source: &str) -> Option<String> {
    let mut words = source.split_whitespace();
    let kind = words.next()?;
    if kind == "FUNCTION"
        || kind == "CLASS"
        || kind == "STRUCT"
        || kind == "INTERFACE"
        || kind == "ENUM"
    {
        let name_part = words.next()?;
        let name = name_part.split('(').next()?;
        return Some(name.to_ascii_uppercase());
    }
    None
}
