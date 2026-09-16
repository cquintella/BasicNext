// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    collections::{BTreeMap, HashMap},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    ast::{Item, Program},
    diagnostic::Diagnostic,
    frontend_session::FrontendSession,
    lexer::lex,
    parser::parse_named,
    source::{Position, SourceFile, Span},
};

pub use crate::types::ModuleId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardModule {
    BNData,
    BNMath,
    BNLog,
    BNWeb,
    BNJson,
    BNDispatch,
}

/// Where an effective import root came from. Logged per root and per resolved
/// module so a stray ancestor `modules/bn` can never hijack resolution silently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RootProvenance {
    /// Directory of the entry file, and its `modules/` compatibility root.
    EntryDir,
    /// Stdlib named explicitly through `BN_HOME` (no ancestor search).
    BnHome,
    /// Stdlib found by walking the entry file's ancestors for `modules/bn`.
    EntryAncestor,
    /// Stdlib found by walking the process working directory's ancestors.
    Cwd,
    /// Stdlib found relative to the `bn` executable (installed layout).
    ExeInstall,
    /// No stdlib found; the default `<entry dir>/modules/bn` placeholder.
    StdlibDefault,
    /// Extra root from the `module-path` config array.
    Config,
    /// Extra root from a `--module-path` flag.
    CliFlag,
}

impl RootProvenance {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::EntryDir => "entry-dir",
            Self::BnHome => "BN_HOME",
            Self::EntryAncestor => "entry-ancestor",
            Self::Cwd => "cwd-ancestor",
            Self::ExeInstall => "exe-install",
            Self::StdlibDefault => "stdlib-default",
            Self::Config => "config",
            Self::CliFlag => "cli-flag",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleRoot {
    pub path: PathBuf,
    pub provenance: RootProvenance,
}

#[derive(Debug)]
pub struct LoadedModule {
    pub id: ModuleId,
    pub path: PathBuf,
    pub source: SourceFile,
    pub program: Program,
    pub imports: Vec<ModuleId>,
    pub standard_module: Option<StandardModule>,
    /// The import root that resolved this module; `None` for the entry file
    /// and for an unresolved import that fell back to the entry directory.
    pub root: Option<ModuleRoot>,
}

#[derive(Debug)]
pub struct ModuleGraph {
    pub root: ModuleId,
    pub modules: Vec<LoadedModule>,
    pub roots: Vec<ModuleRoot>,
}

#[derive(Debug)]
pub struct ModuleError {
    pub source: Box<SourceFile>,
    pub diagnostic: Box<Diagnostic>,
}

/// Loads the executable module and its non-HOST imports beneath its directory.
///
/// # Errors
///
/// Returns the source file that owns a lexical, syntax, missing-module, or
/// import-cycle diagnostic.
pub fn load(entry: impl AsRef<Path>) -> Result<ModuleGraph, ModuleError> {
    let mut session = FrontendSession::default();
    load_with_session(entry, &mut session)
}

/// Loads a module graph while recording every source snapshot in the supplied
/// frontend session.
///
/// The session owns the revision boundary used by frontend clients. The
/// resulting source files and all spans produced from them carry the same
/// `SourceId` and session revision.
///
/// # Errors
///
/// Returns the source file that owns a lexical, syntax, missing-module, or
/// import-cycle diagnostic.
pub fn load_with_session(
    entry: impl AsRef<Path>,
    session: &mut FrontendSession,
) -> Result<ModuleGraph, ModuleError> {
    load_with_overlays(entry, session, &BTreeMap::new())
}

/// Loads a module graph using in-memory source overlays before consulting disk.
///
/// Overlay paths are normalized in the same way as the entry path. This is the
/// handoff used by IDE clients for unsaved buffers.
///
/// # Errors
///
/// Returns the source file that owns a lexical, syntax, missing-module, or
/// import-cycle diagnostic.
pub fn load_with_overlays(
    entry: impl AsRef<Path>,
    session: &mut FrontendSession,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<ModuleGraph, ModuleError> {
    load_with_overlays_and_paths(entry, session, overlays, &[])
}

/// Loads a module graph with an ordered list of additional import roots.
///
/// # Errors
///
/// Returns the source and diagnostic for a lexical, syntax, missing-module, or
/// import-cycle failure.
pub fn load_with_overlays_and_paths(
    entry: impl AsRef<Path>,
    session: &mut FrontendSession,
    overlays: &BTreeMap<PathBuf, String>,
    extras: &[ModuleRoot],
) -> Result<ModuleGraph, ModuleError> {
    let entry = normalize(entry.as_ref());
    let root_directory = entry.parent().map_or_else(PathBuf::new, PathBuf::from);
    let standard = discover_standard_root(&root_directory, std::env::var_os("BN_HOME"));
    let roots = effective_module_paths(&root_directory, &standard, extras);
    let mut loader = Loader {
        root_directory,
        states: HashMap::new(),
        modules: Vec::new(),
        session,
        overlays,
        roots: &roots,
    };
    let root = loader.visit(&entry, None, None)?;
    Ok(ModuleGraph {
        root,
        modules: loader.modules,
        roots,
    })
}

/// Locates the standard-library `modules/bn` root and records where it came
/// from. An explicit `BN_HOME` is authoritative: it is used even when no
/// stdlib exists beneath it (imports then fail at that path), so an ancestor
/// `modules/bn` can never silently win over a configured home.
fn discover_standard_root(root_directory: &Path, bn_home: Option<OsString>) -> ModuleRoot {
    if let Some(home) = bn_home.filter(|value| !value.is_empty()) {
        let home = PathBuf::from(home);
        let path = [home.join("share/bn/modules/bn"), home.join("modules/bn")]
            .into_iter()
            .find(|directory| directory.is_dir())
            .unwrap_or_else(|| home.join("modules/bn"));
        return ModuleRoot {
            path,
            provenance: RootProvenance::BnHome,
        };
    }
    let ancestor_stdlib = |start: &Path| {
        start
            .ancestors()
            .map(|directory| directory.join("modules/bn"))
            .find(|directory| directory.is_dir())
    };
    if let Some(path) = ancestor_stdlib(root_directory) {
        return ModuleRoot {
            path,
            provenance: RootProvenance::EntryAncestor,
        };
    }
    if let Some(path) = std::env::current_dir()
        .ok()
        .and_then(|working_directory| ancestor_stdlib(&working_directory))
    {
        return ModuleRoot {
            path,
            provenance: RootProvenance::Cwd,
        };
    }
    // Installed layout (FHS): the binary lives in <prefix>/bin and the
    // arch-independent stdlib lives in <prefix>/share/bn/modules/bn, matching
    // the diagnostics-catalog convention. `lib/bn/modules/bn` and a portable
    // `modules/bn` beside the executable are also accepted.
    if let Some(path) = std::env::current_exe().ok().and_then(|executable| {
        executable
            .parent()?
            .ancestors()
            .flat_map(|directory| {
                [
                    directory.join("share/bn/modules/bn"),
                    directory.join("lib/bn/modules/bn"),
                    directory.join("modules/bn"),
                ]
            })
            .find(|directory| directory.is_dir())
    }) {
        return ModuleRoot {
            path,
            provenance: RootProvenance::ExeInstall,
        };
    }
    ModuleRoot {
        path: root_directory.join("modules/bn"),
        provenance: RootProvenance::StdlibDefault,
    }
}

/// Computes the ordered import roots. Earlier roots win when the same module
/// exists in multiple roots; a repeated path keeps its first provenance.
#[must_use]
pub fn effective_module_paths(
    root_directory: &Path,
    standard: &ModuleRoot,
    extras: &[ModuleRoot],
) -> Vec<ModuleRoot> {
    let mut roots: Vec<ModuleRoot> = Vec::new();
    for root in [
        ModuleRoot {
            path: root_directory.to_path_buf(),
            provenance: RootProvenance::EntryDir,
        },
        ModuleRoot {
            path: root_directory.join("modules"),
            provenance: RootProvenance::EntryDir,
        },
        standard.clone(),
    ]
    .into_iter()
    .chain(extras.iter().cloned())
    {
        let path = normalize(&root.path);
        if !roots.iter().any(|existing| existing.path == path) {
            roots.push(ModuleRoot {
                path,
                provenance: root.provenance,
            });
        }
    }
    roots
}

enum State {
    Visiting,
    Loaded(ModuleId),
}

struct Loader<'a> {
    root_directory: PathBuf,
    states: HashMap<PathBuf, State>,
    modules: Vec<LoadedModule>,
    session: &'a mut FrontendSession,
    overlays: &'a BTreeMap<PathBuf, String>,
    roots: &'a [ModuleRoot],
}

impl Loader<'_> {
    fn visit(
        &mut self,
        path: &Path,
        importer: Option<(&SourceFile, Span)>,
        root: Option<ModuleRoot>,
    ) -> Result<ModuleId, ModuleError> {
        let path = normalize(path);
        if let Some(state) = self.states.get(&path) {
            return match state {
                State::Loaded(id) => Ok(*id),
                State::Visiting => {
                    let (source, span) = importer.expect("root module cannot form an import cycle");
                    Err(module_error(
                        source,
                        "IMPORT_CYCLE",
                        format!("import cycle includes {}", path.display()),
                        span,
                    ))
                }
            };
        }
        let source = read_source(&path, importer, self.session, self.overlays)?;
        let tokens = lex(&source).map_err(|diagnostic| ModuleError {
            source: Box::new(source.clone()),
            diagnostic: Box::new(diagnostic),
        })?;
        let program = parse_named(&tokens, &source.name).map_err(|diagnostic| ModuleError {
            source: Box::new(source.clone()),
            diagnostic: Box::new(diagnostic),
        })?;
        self.states.insert(path.clone(), State::Visiting);

        let mut imports = Vec::new();
        for item in &program.items {
            let Item::Import {
                path: import, span, ..
            } = item
            else {
                continue;
            };
            if import.first().is_some_and(|part| part == "HOST") {
                continue;
            }
            let (imported_path, winning_root) = self.import_path(import);
            imports.push(self.visit(&imported_path, Some((&source, *span)), winning_root)?);
        }

        let id = ModuleId(u32::try_from(self.modules.len()).map_err(|_| {
            module_error(
                &source,
                "MODULE_LIMIT",
                "module graph exceeds the portable module limit",
                default_span(),
            )
        })?);
        self.states.insert(path.clone(), State::Loaded(id));
        let standard_module = standard_module(&path);
        self.modules.push(LoadedModule {
            id,
            path,
            source,
            program,
            imports,
            standard_module,
            root,
        });
        Ok(id)
    }

    fn import_path(&self, parts: &[String]) -> (PathBuf, Option<ModuleRoot>) {
        let mut fallback = self.root_directory.clone();
        for part in parts {
            fallback.push(part);
        }
        fallback.set_extension("bn");
        for root in self.roots {
            let mut candidate = root.path.clone();
            for part in parts {
                candidate.push(part);
            }
            candidate.set_extension("bn");
            if exact_file_exists(&candidate) {
                return (candidate, Some(root.clone()));
            }
        }
        (fallback, None)
    }
}

fn exact_file_exists(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(name) = path.file_name() else {
        return false;
    };
    fs::read_dir(parent)
        .is_ok_and(|entries| entries.flatten().any(|entry| entry.file_name() == name))
}

fn standard_module(path: &Path) -> Option<StandardModule> {
    let in_bn = path
        .parent()
        .is_some_and(|directory| directory.ends_with("modules/bn"));
    match path.file_name().and_then(|name| name.to_str()) {
        Some("BNData.bn") if in_bn => Some(StandardModule::BNData),
        Some("BNMath.bn") if in_bn => Some(StandardModule::BNMath),
        Some("BNLog.bn") if in_bn => Some(StandardModule::BNLog),
        Some("BNWeb.bn") if in_bn => Some(StandardModule::BNWeb),
        Some("BNJson.bn") if in_bn => Some(StandardModule::BNJson),
        Some("BNDispatch.bn") if in_bn => Some(StandardModule::BNDispatch),
        _ => None,
    }
}

fn read_source(
    path: &Path,
    importer: Option<(&SourceFile, Span)>,
    session: &mut FrontendSession,
    overlays: &BTreeMap<PathBuf, String>,
) -> Result<SourceFile, ModuleError> {
    let name = path.display().to_string();
    let text = overlays
        .get(path)
        .cloned()
        .map_or_else(|| fs::read_to_string(path), Ok);
    match text {
        Ok(text) => {
            let identity = SourceFile::new(name.clone(), "").source_id;
            let snapshot = session.upsert_if_changed(Some(identity), text.clone());
            Ok(SourceFile {
                name,
                text,
                source_id: snapshot.source,
                revision: snapshot.revision,
            })
        }
        Err(error) => {
            let (source, span) = importer.map_or_else(
                || (SourceFile::new(name.clone(), ""), default_span()),
                |(source, span)| {
                    (
                        SourceFile::new(source.name.clone(), source.text.clone()),
                        span,
                    )
                },
            );
            Err(ModuleError {
                source: Box::new(source),
                diagnostic: Box::new(
                    Diagnostic::structured(
                        crate::diagnostic::DiagId::MODULE_NOT_FOUND,
                        vec![("path".into(), format!("{name}: {error}").into())],
                        vec![crate::diagnostic::Label {
                            span,
                            style: crate::diagnostic::LabelStyle::Primary,
                            text: None,
                        }],
                    )
                    .expect("module-not-found diagnostic schema is registered"),
                ),
            })
        }
    }
}

fn normalize(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
        }
    })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{
        BTreeMap, HashMap, Item, Loader, ModuleRoot, PathBuf, RootProvenance,
        discover_standard_root, effective_module_paths, load_with_session,
    };
    use crate::frontend_session::FrontendSession;

    #[test]
    fn loader_reuses_a_module_across_equivalent_path_spellings() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repository = if manifest.join("tests/modules").is_dir() {
            manifest
        } else {
            manifest
                .parent()
                .and_then(|parent| parent.parent())
                .expect("frontend crate lives below the repository root")
                .to_path_buf()
        };
        let graph = repository.join("tests/modules/graph");
        let mut session = FrontendSession::default();
        let overlays = BTreeMap::new();
        let standard = ModuleRoot {
            path: repository.join("modules/bn"),
            provenance: RootProvenance::EntryAncestor,
        };
        let roots = effective_module_paths(&graph, &standard, &[]);
        let mut loader = Loader {
            root_directory: graph.clone(),
            states: HashMap::new(),
            modules: Vec::new(),
            session: &mut session,
            overlays: &overlays,
            roots: &roots,
        };
        let first = loader
            .visit(&graph.join("main.bn"), None, None)
            .expect("load module");
        let second = loader
            .visit(&graph.join("./main.bn"), None, None)
            .expect("reuse module");
        assert_eq!(first, second);
    }

    #[test]
    fn bn_home_overrides_ancestor_search_even_when_empty() {
        let scratch = std::env::temp_dir().join(format!("bn-home-{}", std::process::id()));
        let home = scratch.join("home");
        std::fs::create_dir_all(home.join("modules/bn")).expect("home stdlib");
        // Entry lives under a directory that has its own ancestor modules/bn.
        let project = scratch.join("hijack/modules/bn/../../src");
        std::fs::create_dir_all(&project).expect("project dir");
        let root = discover_standard_root(&project, Some(home.clone().into_os_string()));
        assert_eq!(root.provenance, RootProvenance::BnHome);
        assert_eq!(root.path, home.join("modules/bn"));

        let missing = scratch.join("nowhere");
        let root = discover_standard_root(&project, Some(missing.clone().into_os_string()));
        assert_eq!(root.provenance, RootProvenance::BnHome);
        assert_eq!(
            root.path,
            missing.join("modules/bn"),
            "BN_HOME never falls through"
        );

        let root = discover_standard_root(&project, None);
        assert_eq!(root.provenance, RootProvenance::EntryAncestor);
        std::fs::remove_dir_all(&scratch).ok();
    }

    #[test]
    fn effective_roots_keep_first_provenance_for_duplicate_paths() {
        let entry = PathBuf::from("/tmp/bn-roots/app");
        let standard = ModuleRoot {
            path: PathBuf::from("/tmp/bn-roots/modules/bn"),
            provenance: RootProvenance::Cwd,
        };
        let extras = [
            ModuleRoot {
                path: PathBuf::from("/tmp/bn-roots/app"),
                provenance: RootProvenance::CliFlag,
            },
            ModuleRoot {
                path: PathBuf::from("/opt/bn"),
                provenance: RootProvenance::Config,
            },
        ];
        let roots = effective_module_paths(&entry, &standard, &extras);
        let labels: Vec<_> = roots.iter().map(|root| root.provenance.label()).collect();
        assert_eq!(
            labels,
            ["entry-dir", "entry-dir", "cwd-ancestor", "config"],
            "duplicate entry dir from CLI is dropped, keeping entry-dir provenance"
        );
    }

    #[test]
    fn load_with_session_propagates_snapshot_identity_to_module_spans() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repository = if manifest.join("tests/modules").is_dir() {
            manifest
        } else {
            manifest
                .parent()
                .and_then(|parent| parent.parent())
                .expect("frontend crate lives below the repository root")
                .to_path_buf()
        };
        let entry = repository.join("tests/modules/graph/main.bn");
        let mut session = FrontendSession::default();
        let graph = load_with_session(&entry, &mut session).expect("load module graph");
        let root = graph
            .modules
            .iter()
            .find(|module| module.id == graph.root)
            .expect("graph root");
        let snapshot = session
            .snapshot(root.source.source_id)
            .expect("root snapshot");
        assert_eq!(root.source.source_id, snapshot.source);
        assert_eq!(root.source.revision, snapshot.revision);
        let span = match &root.program.items[0] {
            Item::Import { span, .. }
            | Item::Constant { span, .. }
            | Item::Declaration { span, .. } => *span,
        };
        assert_ne!(span.source_id(), crate::source::SourceId::UNKNOWN);
        assert_eq!(span.revision(), snapshot.revision);
    }
}

fn module_error(
    source: &SourceFile,
    code: &'static str,
    message: impl Into<String>,
    span: Span,
) -> ModuleError {
    let diagnostic = crate::diagnostic::DiagId::from_code(code)
        .and_then(|id| {
            Diagnostic::structured(
                id,
                vec![("detail".into(), message.into().into())],
                vec![crate::diagnostic::Label {
                    span,
                    style: crate::diagnostic::LabelStyle::Primary,
                    text: None,
                }],
            )
            .ok()
        })
        .unwrap_or_else(|| Diagnostic {
            code,
            message: "module graph error".into(),
            span,
            structured: None,
        });
    ModuleError {
        source: Box::new(SourceFile::new(source.name.clone(), source.text.clone())),
        diagnostic: Box::new(diagnostic),
    }
}

fn default_span() -> Span {
    let start = Position {
        source_id: Position::UNKNOWN_SOURCE,
        revision: Position::UNKNOWN_REVISION,
        offset: 0,
        line: 1,
        column: 1,
    };
    Span { start, end: start }
}
