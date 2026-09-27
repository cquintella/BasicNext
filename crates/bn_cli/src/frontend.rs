//! CLI adapter over the frontend: reads and lexes the entry file, maps the
//! common options to `bn_frontend::prepare` and presents its errors. The result is the validated
//! artifact every command (check, run, build) consumes (W1/W3).

use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use bn_frontend::{
    frontend_session::FrontendSession,
    lexer::lex,
    prepare::{PrepareError, Prepared, prepare},
    token::Token,
};
use bn_source::SourceFile;

use crate::{
    diagnostics::{emit_frontend_error, render_diagnostic},
    options::Options,
    output::{language_error, log, module_index, tool_error},
};

pub type Frontend = Prepared;

/// Reads `options.path` and lexes it, reporting the -v stages. Every
/// file-oriented command starts here before `load_frontend`.
///
/// # Errors
///
/// The message or diagnostic was already printed; returns the exit code
/// (tool error for an unreadable file, language error for a lexical one).
pub fn read_source(options: &Options) -> Result<(SourceFile, Vec<Token>), ExitCode> {
    log(options.verbosity, 1, format!("reading {}", options.path));
    let text = match std::fs::read_to_string(&options.path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", options.path);
            return Err(tool_error());
        }
    };
    let source = SourceFile::new(&options.path, text);
    log(options.verbosity, 1, "lexical analysis");
    let tokens = match lex(&source) {
        Ok(tokens) => tokens,
        Err(diagnostic) => {
            eprintln!("{}", render_diagnostic(&diagnostic, &source, options));
            return Err(language_error());
        }
    };
    log(
        options.verbosity,
        1,
        format!("lexer completed: {} tokens", tokens.len()),
    );
    Ok((source, tokens))
}

/// Prepares `options.path` from disk.
///
/// # Errors
///
/// The diagnostic was already printed; returns the process exit code.
pub fn load_frontend(source: &SourceFile, options: &Options) -> Result<Frontend, ExitCode> {
    load_frontend_with_overlays(source, options, &BTreeMap::new())
}

/// Prepares `options.path`, reading `overlays` instead of the disk for the
/// paths they name.
///
/// # Errors
///
/// The diagnostic was already printed; returns the process exit code.
pub fn load_frontend_with_overlays(
    source: &SourceFile,
    options: &Options,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<Frontend, ExitCode> {
    log(options.verbosity, 1, "loading module graph");
    let mut session = FrontendSession::default();
    let prepared =
        prepare(&options.path, &mut session, overlays, &options.module_paths).map_err(|error| {
            let phase = error.phase();
            match &error {
                PrepareError::Load(error) => {
                    emit_frontend_error(&error.diagnostic, &error.source, options, phase)
                }
                PrepareError::Semantic { graph, error } => {
                    let source = graph
                        .modules
                        .get(module_index(error.module.0))
                        .map_or(source, |module| &module.source);
                    emit_frontend_error(&error.diagnostic, source, options, phase)
                }
                PrepareError::Lower { diagnostic, .. } => {
                    emit_frontend_error(diagnostic, source, options, phase)
                }
            }
        })?;
    // Stage lines are reported after the one-call preparation; on failure the
    // diagnostic is the only output, as before.
    log(
        options.verbosity,
        1,
        "syntax analysis (module graph root reused)",
    );
    log(options.verbosity, 1, "semantic analysis");
    log(options.verbosity, 1, "lowering and validating IR");
    log(
        options.verbosity,
        1,
        format!(
            "parser completed: {} top-level items",
            prepared.root_program().items.len()
        ),
    );
    Ok(prepared)
}
