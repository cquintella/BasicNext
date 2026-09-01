// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardModule {
    BNData,
    BNMath,
    BNLog,
    BNWeb,
    BNJson,
    BNDispatch,
}

#[derive(Debug)]
pub struct LoadedModule {
    pub id: ModuleId,
    pub path: PathBuf,
    pub source: SourceFile,
    pub program: Program,
    pub imports: Vec<ModuleId>,
    pub standard_module: Option<StandardModule>,
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
    let entry = normalize(entry.as_ref());
    let root_directory = entry.parent().map_or_else(PathBuf::new, PathBuf::from);
    let standard_directory = root_directory
        .ancestors()
        .map(|directory| directory.join("modules/bn"))
        .find(|directory| directory.is_dir())
        .or_else(|| {
            std::env::current_exe().ok().and_then(|executable| {
                executable
                    .parent()?
                    .ancestors()
                    .map(|directory| directory.join("modules/bn"))
                    .find(|directory| directory.is_dir())
            })
        })
        .unwrap_or_else(|| root_directory.join("modules/bn"));
    let mut loader = Loader {
        root_directory,
        standard_directory,
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
    standard_directory: PathBuf,
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
        let standard_module = standard_module(&path);
        self.modules.push(LoadedModule {
            id,
            path,
            source,
            program,
            imports,
            standard_module,
        });
        Ok(id)
    }

    fn import_path(&self, parts: &[String]) -> PathBuf {
        if parts.len() == 1 && parts[0].starts_with("BN") {
            let mut path = self.standard_directory.clone();
            path.push(&parts[0]);
            path.set_extension("bn");
            return path;
        }
        let mut path = self.root_directory.clone();
        path.push("modules");
        for part in parts {
            path.push(part);
        }
        path.set_extension("bn");
        if !path.exists() {
            path.clone_from(&self.root_directory);
            for part in parts {
                path.push(part);
            }
            path.set_extension("bn");
        }
        path
    }
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
    use super::{HashMap, Loader, Path, PathBuf};

    #[test]
    fn loader_reuses_a_module_across_equivalent_path_spellings() {
        let mut loader = Loader {
            root_directory: PathBuf::from("tests/modules/graph"),
            standard_directory: PathBuf::from("modules/bn"),
            states: HashMap::new(),
            modules: Vec::new(),
        };
        let first = loader
            .visit(Path::new("tests/modules/graph/main.bn"), None)
            .expect("load module");
        let second = loader
            .visit(Path::new("./tests/modules/graph/main.bn"), None)
            .expect("reuse module");
        assert_eq!(first, second);
    }
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
