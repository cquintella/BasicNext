//! CLI adapter over `bn_frontend::prepare`: maps common options to the
//! preparation service and presents its errors. The result is the validated
//! artifact every command (check, run, build) consumes (W1/W3).

use std::{collections::BTreeMap, path::PathBuf, process::ExitCode};

use bn_frontend::{
    frontend_session::FrontendSession,
    prepare::{PrepareError, Prepared, prepare},
};
use bn_source::SourceFile;

use crate::{
    diagnostics::emit_frontend_error,
    options::Options,
    output::{log, module_index},
};

pub type Frontend = Prepared;

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
