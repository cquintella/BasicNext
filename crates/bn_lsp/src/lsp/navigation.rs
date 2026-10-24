//! Hover, definition and references over the semantic model already produced
//! for diagnostics. The snapshot is empty when that analysis does not yield
//! models; navigation then returns nothing instead of matching identifier text.

use std::{collections::HashMap, path::Path};

use lsp_types::{Location, Position as LspPosition, Uri};

use bn_frontend::{
    module_graph::{ModuleGraph, ModuleId},
    semantic::{MemberTarget, ResolvedSymbol, SemanticModel, SymbolId},
};
use bn_source::{SourceFile, SourceId, Span};

use crate::file_uri::{file_uri_to_path, path_to_file_uri};
use crate::position::{lsp_pos_to_scalar, span_to_lsp_range, utf16_to_scalar_col};

#[derive(Default)]
pub(crate) struct NavigationSnapshot {
    modules: Vec<NavigableModule>,
}

struct NavigableModule {
    uri: String,
    source_id: SourceId,
    module_id: ModuleId,
    text: String,
    model: SemanticModel,
}

struct Focus {
    module_id: ModuleId,
    expression: Option<ExpressionHit>,
    symbol: Option<SymbolHit>,
}

struct ExpressionHit {
    type_name: String,
    symbol_id: Option<SymbolId>,
    member_target: Option<MemberTarget>,
}

struct SymbolHit {
    id: SymbolId,
    name: String,
    type_name: String,
    span: Span,
}

impl NavigationSnapshot {
    pub(crate) fn from_graph<S: std::hash::BuildHasher>(
        graph: &ModuleGraph,
        models: Vec<SemanticModel>,
        documents: &HashMap<String, SourceFile, S>,
    ) -> Self {
        let modules = graph
            .modules
            .iter()
            .zip(models)
            .map(|(loaded, model)| NavigableModule {
                uri: client_uri(&loaded.path, documents),
                source_id: loaded.source.source_id,
                module_id: loaded.id,
                text: loaded.source.text.clone(),
                model,
            })
            .collect();
        Self { modules }
    }

    pub(crate) fn hover(&self, uri: &str, position: LspPosition) -> Option<String> {
        let focus = self.focus(uri, position)?;
        if let Some(expression) = focus.expression {
            let name = expression
                .symbol_id
                .and_then(|id| {
                    self.symbol(focus.module_id, id)
                        .map(|symbol| symbol.name.clone())
                })
                .or_else(|| {
                    expression
                        .member_target
                        .as_ref()
                        .map(|target| target.name.clone())
                })
                .unwrap_or_default();
            return hover_text(&name, &expression.type_name);
        }
        focus
            .symbol
            .as_ref()
            .and_then(|symbol| hover_text(&symbol.name, &symbol.type_name))
    }

    pub(crate) fn definition(&self, uri: &str, position: LspPosition) -> Vec<Location> {
        let Some(focus) = self.focus(uri, position) else {
            return Vec::new();
        };
        let location = if let Some(expression) = focus.expression {
            let declared = expression
                .symbol_id
                .and_then(|id| self.symbol(focus.module_id, id).map(|symbol| symbol.span));
            declared
                .and_then(|span| self.location_of(span))
                .or_else(|| {
                    expression
                        .member_target
                        .as_ref()
                        .and_then(|target| self.member_location(target))
                })
        } else {
            focus
                .symbol
                .and_then(|symbol| self.location_of(symbol.span))
        };
        location.into_iter().collect()
    }

    pub(crate) fn references(
        &self,
        uri: &str,
        position: LspPosition,
        include_declaration: bool,
    ) -> Vec<Location> {
        let Some(focus) = self.focus(uri, position) else {
            return Vec::new();
        };
        if let Some(expression) = focus.expression {
            if let Some(id) = expression.symbol_id {
                return self.symbol_references(focus.module_id, id, include_declaration);
            }
            if let Some(target) = expression.member_target {
                return self.member_references(&target, include_declaration);
            }
            return Vec::new();
        }
        focus.symbol.map_or(Vec::new(), |symbol| {
            self.symbol_references(focus.module_id, symbol.id, include_declaration)
        })
    }

    fn focus(&self, uri: &str, position: LspPosition) -> Option<Focus> {
        let module = self.module_for_uri(uri)?;
        let source = SourceFile::new(module.uri.clone(), module.text.clone());
        let (line, column) = lsp_pos_to_scalar(&source, position);
        let expression = smallest(module.model.expressions.iter().filter_map(|expression| {
            contains(expression.span, line, column).then_some((
                expression.span,
                ExpressionHit {
                    type_name: expression.type_name.clone(),
                    symbol_id: expression.symbol_id,
                    member_target: expression.member_target.clone(),
                },
            ))
        }));
        let word = identifier_at(&module.text, position);
        let symbol = if expression.is_some() || word.is_empty() {
            None
        } else {
            smallest(module.model.symbols.iter().filter_map(|symbol| {
                (symbol.name == word && contains(symbol.span, line, column)).then_some((
                    symbol.span,
                    SymbolHit {
                        id: symbol.id,
                        name: symbol.name.clone(),
                        type_name: symbol.type_name.clone(),
                        span: symbol.span,
                    },
                ))
            }))
        };
        Some(Focus {
            module_id: module.module_id,
            expression,
            symbol,
        })
    }

    fn module_for_uri(&self, uri: &str) -> Option<&NavigableModule> {
        self.modules
            .iter()
            .find(|module| module.uri == uri)
            .or_else(|| {
                self.modules
                    .iter()
                    .find(|module| uri_matches_module(uri, module))
            })
    }

    fn symbol(&self, module_id: ModuleId, symbol_id: SymbolId) -> Option<&ResolvedSymbol> {
        self.modules
            .iter()
            .find(|module| module.module_id == module_id)?
            .model
            .symbols
            .iter()
            .find(|symbol| symbol.id == symbol_id)
    }

    fn member_location(&self, target: &MemberTarget) -> Option<Location> {
        let module_id = target.module?;
        let module = self
            .modules
            .iter()
            .find(|module| module.module_id == module_id)?;
        let symbol = smallest(module.model.symbols.iter().filter_map(|symbol| {
            (symbol.name == target.name && symbol.span.start.source_id == module.source_id)
                .then_some((symbol.span, symbol))
        }))?;
        let span = symbol.span;
        self.location_of(span)
    }

    fn symbol_references(
        &self,
        module_id: ModuleId,
        symbol_id: SymbolId,
        include_declaration: bool,
    ) -> Vec<Location> {
        let Some(module) = self
            .modules
            .iter()
            .find(|module| module.module_id == module_id)
        else {
            return Vec::new();
        };
        let Some(symbol) = module
            .model
            .symbols
            .iter()
            .find(|symbol| symbol.id == symbol_id)
        else {
            return Vec::new();
        };
        let declaration_span = symbol.span;
        let use_spans = module
            .model
            .expressions
            .iter()
            .filter(|expression| expression.symbol_id == Some(symbol_id))
            .map(|expression| expression.span)
            .collect::<Vec<_>>();
        let declaration = self.location_of(declaration_span);
        let mut locations = use_spans
            .into_iter()
            .filter_map(|span| self.location_of(span))
            .collect::<Vec<_>>();
        push_declaration(&mut locations, declaration, include_declaration);
        sort_locations(&mut locations);
        locations
    }

    fn member_references(&self, target: &MemberTarget, include_declaration: bool) -> Vec<Location> {
        let declaration = self.member_location(target);
        let use_spans = self
            .modules
            .iter()
            .flat_map(|module| module.model.expressions.iter())
            .filter(|expression| expression.member_target.as_ref() == Some(target))
            .map(|expression| expression.span)
            .collect::<Vec<_>>();
        let mut locations = use_spans
            .into_iter()
            .filter_map(|span| self.location_of(span))
            .collect::<Vec<_>>();
        push_declaration(&mut locations, declaration, include_declaration);
        sort_locations(&mut locations);
        locations
    }

    fn location_of(&self, span: Span) -> Option<Location> {
        if span.start.source_id == SourceId::UNKNOWN {
            return None;
        }
        let module = self
            .modules
            .iter()
            .find(|module| module.source_id == span.start.source_id)?;
        let source = SourceFile::new(module.uri.clone(), module.text.clone());
        let uri = module.uri.parse::<Uri>().ok()?;
        Some(Location::new(uri, span_to_lsp_range(&source, span)))
    }
}

fn hover_text(name: &str, type_name: &str) -> Option<String> {
    if type_name.is_empty() {
        return None;
    }
    if name.is_empty() {
        return Some(type_name.to_string());
    }
    Some(format!("{name}\n{type_name}"))
}

fn push_declaration(
    locations: &mut Vec<Location>,
    declaration: Option<Location>,
    include_declaration: bool,
) {
    let Some(declaration) = declaration else {
        return;
    };
    if include_declaration {
        locations.push(declaration);
        return;
    }
    locations.retain(|location| location != &declaration);
}

fn sort_locations(locations: &mut Vec<Location>) {
    locations.sort_by(|left, right| {
        left.uri
            .as_str()
            .cmp(right.uri.as_str())
            .then(left.range.start.line.cmp(&right.range.start.line))
            .then(left.range.start.character.cmp(&right.range.start.character))
            .then(left.range.end.line.cmp(&right.range.end.line))
            .then(left.range.end.character.cmp(&right.range.end.character))
    });
    locations.dedup();
}

fn smallest<T>(items: impl Iterator<Item = (Span, T)>) -> Option<T> {
    items
        .min_by_key(|(span, _)| {
            (
                span.end.offset.saturating_sub(span.start.offset),
                span.end.line.saturating_sub(span.start.line),
                span.start.offset,
            )
        })
        .map(|(_, item)| item)
}

fn contains(span: Span, line: usize, column: usize) -> bool {
    if line == 0 || column == 0 {
        return false;
    }
    let after_start =
        span.start.line < line || (span.start.line == line && span.start.column <= column);
    let before_end = span.end.line > line || (span.end.line == line && span.end.column > column);
    after_start && before_end
}

fn client_uri<S: std::hash::BuildHasher>(
    path: &Path,
    documents: &HashMap<String, SourceFile, S>,
) -> String {
    documents
        .keys()
        .find(|uri| uri_matches_path(uri, path))
        .cloned()
        .unwrap_or_else(|| path_to_file_uri(path))
}

fn uri_matches_module(uri: &str, module: &NavigableModule) -> bool {
    file_uri_to_path(uri).is_some_and(|asked| {
        file_uri_to_path(&module.uri).is_some_and(|stored| same_path(&asked, &stored))
    })
}

fn uri_matches_path(uri: &str, path: &Path) -> bool {
    file_uri_to_path(uri).is_some_and(|asked| same_path(&asked, path))
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn identifier_at(text: &str, position: LspPosition) -> String {
    let Some(line) = text
        .lines()
        .nth(usize::try_from(position.line).unwrap_or(usize::MAX))
    else {
        return String::new();
    };
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let char_idx = utf16_to_scalar_col(line, position.character).saturating_sub(1);
    let idx = char_idx.min(chars.len().saturating_sub(1));
    let is_ident = |character: char| character.is_ascii_alphanumeric() || character == '_';
    if !is_ident(chars[idx]) {
        if idx > 0 && is_ident(chars[idx - 1]) {
            return word_at(&chars, idx - 1);
        }
        return String::new();
    }
    word_at(&chars, idx)
}

fn word_at(chars: &[char], idx: usize) -> String {
    let is_ident = |character: char| character.is_ascii_alphanumeric() || character == '_';
    let mut start = idx;
    while start > 0 && is_ident(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && is_ident(chars[end]) {
        end += 1;
    }
    chars[start..end].iter().collect()
}
