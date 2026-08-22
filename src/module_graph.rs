use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    ast::{Item, Program},
    diagnostic::Diagnostic,
    lexer::lex,
    parser::parse_named,
    source::{Position, SourceFile, Span},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ModuleId(pub u32);

#[derive(Debug)]
pub struct LoadedModule {
    pub id: ModuleId,
    pub path: PathBuf,
    pub source: SourceFile,
    pub program: Program,
    pub imports: Vec<ModuleId>,
}

#[derive(Debug)]
pub struct ModuleGraph {
    pub root: ModuleId,
    pub modules: Vec<LoadedModule>,
}

#[derive(Debug)]
pub struct ModuleError {
    pub source: Box<SourceFile>,
    pub diagnostic: Diagnostic,
}

/// Loads the executable module and its non-HOST imports beneath its directory.
///
/// # Errors
///
/// Returns the source file that owns a lexical, syntax, missing-module, or
/// import-cycle diagnostic.
pub fn load(entry: impl AsRef<Path>) -> Result<ModuleGraph, ModuleError> {
    let entry = entry.as_ref().to_path_buf();
    let root_directory = entry.parent().map_or_else(PathBuf::new, PathBuf::from);
    let mut loader = Loader {
        root_directory,
        states: HashMap::new(),
        modules: Vec::new(),
    };
    let root = loader.visit(&entry, None)?;
    Ok(ModuleGraph {
        root,
        modules: loader.modules,
    })
}

enum State {
    Visiting,
    Loaded(ModuleId),
}

struct Loader {
    root_directory: PathBuf,
    states: HashMap<PathBuf, State>,
    modules: Vec<LoadedModule>,
}

impl Loader {
    fn visit(
        &mut self,
        path: &Path,
        importer: Option<(&SourceFile, Span)>,
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
        let source = read_source(&path, importer)?;
        let tokens = lex(&source).map_err(|diagnostic| ModuleError {
            source: Box::new(source.clone()),
            diagnostic,
        })?;
        let program = parse_named(&tokens, &source.name).map_err(|diagnostic| ModuleError {
            source: Box::new(source.clone()),
            diagnostic,
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
            let imported_path = self.import_path(import);
            imports.push(self.visit(&imported_path, Some((&source, *span)))?);
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
        self.modules.push(LoadedModule {
            id,
            path,
            source,
            program,
            imports,
        });
        Ok(id)
    }

    fn import_path(&self, parts: &[String]) -> PathBuf {
        let mut path = self.root_directory.clone();
        for part in parts {
            path.push(part);
        }
        path.set_extension("bn");
        path
    }
}

fn read_source(
    path: &Path,
    importer: Option<(&SourceFile, Span)>,
) -> Result<SourceFile, ModuleError> {
    let name = path.display().to_string();
    match fs::read_to_string(path) {
        Ok(text) => Ok(SourceFile::new(name, text)),
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
                diagnostic: Diagnostic {
                    code: "MODULE_NOT_FOUND",
                    message: format!("cannot load module {name}: {error}"),
                    span,
                },
            })
        }
    }
}

fn normalize(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn module_error(
    source: &SourceFile,
    code: &'static str,
    message: impl Into<String>,
    span: Span,
) -> ModuleError {
    ModuleError {
        source: Box::new(SourceFile::new(source.name.clone(), source.text.clone())),
        diagnostic: Diagnostic {
            code,
            message: message.into(),
            span,
        },
    }
}

fn default_span() -> Span {
    let start = Position {
        offset: 0,
        line: 1,
        column: 1,
    };
    Span { start, end: start }
}
