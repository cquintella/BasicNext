//! Shared program preparation: module graph → semantic models → validated IR.
//! Returns structured results and errors; never prints. CLI, LSP and DAP
//! adapt the outcome to their own presentation.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use bn_ir::ValidatedModule;

use crate::{
    ast::Program,
    diagnostic::Diagnostic,
    frontend_session::FrontendSession,
    lowering::lower_graph_validated,
    module_graph::{self, ModuleError, ModuleGraph, ModuleRoot},
    semantic::{ModuleAnalysisError, SemanticModel, analyze_modules_with_warnings},
};

/// A program that passed language `validate` (W1): the only artifact a backend
/// may consume.
#[derive(Debug)]
pub struct Prepared {
    pub graph: ModuleGraph,
    pub models: Vec<SemanticModel>,
    pub warnings: Vec<ModuleAnalysisError>,
    pub validated: ValidatedModule,
}

impl Prepared {
    /// The executable (root) module's syntax tree.
    ///
    /// # Panics
    ///
    /// Never for a `Prepared` built by [`prepare`]: a graph always contains
    /// its root module.
    #[must_use]
    pub fn root_program(&self) -> &Program {
        self.graph
            .modules
            .iter()
            .find(|module| module.id == self.graph.root)
            .map(|module| &module.program)
            .expect("module graph always contains its root")
    }
}

/// Phase in which preparation stopped, with the diagnostic that stopped it.
#[derive(Debug)]
pub enum PrepareError {
    /// Lexical, syntax, missing-module or import-cycle error; owns its source.
    Load(ModuleError),
    /// Semantic error; `error.module` indexes `graph.modules`.
    Semantic {
        graph: Box<ModuleGraph>,
        error: ModuleAnalysisError,
    },
    /// Lowering or language-validation error over the loaded graph.
    Lower {
        graph: Box<ModuleGraph>,
        diagnostic: Box<Diagnostic>,
    },
}

impl PrepareError {
    #[must_use]
    pub fn diagnostic(&self) -> &Diagnostic {
        match self {
            Self::Load(error) => &error.diagnostic,
            Self::Semantic { error, .. } => &error.diagnostic,
            Self::Lower { diagnostic, .. } => diagnostic,
        }
    }

    /// Stable phase name used by CLI diagnostics (`parse`, `semantic`, `lower`).
    #[must_use]
    pub const fn phase(&self) -> &'static str {
        match self {
            Self::Load(_) => "parse",
            Self::Semantic { .. } => "semantic",
            Self::Lower { .. } => "lower",
        }
    }
}

/// Loads `entry` (with in-memory `overlays` and extra module roots), analyzes
/// every module and lowers to validated IR.
///
/// # Errors
///
/// Returns the first phase that failed together with its diagnostic.
pub fn prepare(
    entry: impl AsRef<Path>,
    session: &mut FrontendSession,
    overlays: &BTreeMap<PathBuf, String>,
    extras: &[ModuleRoot],
) -> Result<Prepared, PrepareError> {
    let graph = module_graph::load_with_overlays_and_paths(entry, session, overlays, extras)
        .map_err(PrepareError::Load)?;
    let analysis = match analyze_modules_with_warnings(&graph) {
        Ok(analysis) => analysis,
        Err(error) => {
            return Err(PrepareError::Semantic {
                graph: Box::new(graph),
                error,
            });
        }
    };
    let validated = match lower_graph_validated(&graph, &analysis.models) {
        Ok(validated) => validated,
        Err(diagnostic) => {
            return Err(PrepareError::Lower {
                graph: Box::new(graph),
                diagnostic: Box::new(diagnostic),
            });
        }
    };
    Ok(Prepared {
        graph,
        models: analysis.models,
        warnings: analysis.warnings,
        validated,
    })
}
