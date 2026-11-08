#![allow(clippy::wildcard_imports)]
use super::*;
use std::{collections::BTreeMap, path::Path};

use lsp_types::CompletionItemKind;

use bn_frontend::token::{TokenKind, reserved_words};

const STANDARD_MODULES: &[&str] = &[
    "BNCrypto",
    "BNData",
    "BNDispatch",
    "BNJson",
    "BNLog",
    "BNMath",
    "BNSqlite",
    "BNWeb",
];

#[allow(
    clippy::implicit_hasher,
    reason = "Standard hasher used across LSP documents map"
)]
pub fn completion_items(
    source: &SourceFile,
    position: Position,
    documents: &HashMap<String, SourceFile>,
    workspace_root: Option<&Path>,
) -> Vec<CompletionItem> {
    let before = text_before(source, position);
    let (chain, prefix) = dotted_query(&before);
    let index = document_index(source);
    let mut items = BTreeMap::<String, CompletionItem>::new();
    if chain.is_empty() {
        add_keywords(&mut items, &prefix);
        add_builtins(&mut items, &prefix);
        if import_context(&before) {
            add_filtered(
                &mut items,
                STANDARD_MODULES.iter().copied(),
                CompletionItemKind::MODULE,
                "standard module",
                &prefix,
            );
        }
        for name in &index.functions {
            add(
                &mut items,
                name,
                CompletionItemKind::FUNCTION,
                "function",
                &prefix,
            );
        }
        for name in &index.classes {
            add(
                &mut items,
                name,
                CompletionItemKind::CLASS,
                "class",
                &prefix,
            );
        }
        for name in &index.bindings {
            add(
                &mut items,
                name,
                CompletionItemKind::VARIABLE,
                "binding",
                &prefix,
            );
        }
        for (_, alias) in &index.imports {
            add(
                &mut items,
                alias,
                CompletionItemKind::MODULE,
                "import",
                &prefix,
            );
        }
        add_analyze_symbols(source, &mut items, &prefix);
    } else {
        let path = resolve_chain(&chain, &index.imports);
        let kind = if path == "HOST" {
            CompletionItemKind::MODULE
        } else {
            CompletionItemKind::FUNCTION
        };
        let catalog = members_of(&path);
        if catalog.is_empty() {
            add_filtered(
                &mut items,
                module_exports(&path, source, documents, workspace_root),
                kind,
                &path,
                &prefix,
            );
            if items.is_empty() {
                add_semantic_members(source, &path, &mut items, &prefix);
            }
        } else {
            add_filtered(
                &mut items,
                catalog.iter().map(String::as_str),
                kind,
                &path,
                &prefix,
            );
        }
    }
    items.into_values().collect()
}

fn add_keywords(items: &mut BTreeMap<String, CompletionItem>, prefix: &str) {
    add_filtered(
        items,
        reserved_words().iter().copied(),
        CompletionItemKind::KEYWORD,
        "Basic Next keyword",
        prefix,
    );
}

fn add_builtins(items: &mut BTreeMap<String, CompletionItem>, prefix: &str) {
    add_filtered(
        items,
        [
            "ASC",
            "TOLOWER",
            "TOUPPER",
            "CHAR",
            "Date",
            "Error",
            "Float",
            "HOST",
            "Time",
            "TimeZone",
            "Timestamp",
        ],
        CompletionItemKind::MODULE,
        "built-in",
        prefix,
    );
}

fn add_analyze_symbols(
    source: &SourceFile,
    items: &mut BTreeMap<String, CompletionItem>,
    prefix: &str,
) {
    let Ok(tokens) = lex(source) else {
        return;
    };
    let Ok(program) = parse_named(&tokens, source.name.clone()) else {
        return;
    };
    let Ok(model) = analyze(&program) else {
        return;
    };
    for symbol in model.symbols {
        let kind = if matches!(symbol.ty, bn_types::Type::Function { .. }) {
            CompletionItemKind::FUNCTION
        } else {
            CompletionItemKind::VARIABLE
        };
        add(items, &symbol.name, kind, &symbol.type_name, prefix);
    }
}

fn add_filtered<S: AsRef<str>>(
    items: &mut BTreeMap<String, CompletionItem>,
    names: impl IntoIterator<Item = S>,
    kind: CompletionItemKind,
    detail: &str,
    prefix: &str,
) {
    for name in names {
        add(items, name.as_ref(), kind, detail, prefix);
    }
}

fn add(
    items: &mut BTreeMap<String, CompletionItem>,
    name: &str,
    kind: CompletionItemKind,
    detail: &str,
    prefix: &str,
) {
    if !name.starts_with(prefix) {
        return;
    }
    items.entry(name.to_string()).or_insert(CompletionItem {
        label: name.into(),
        kind: Some(kind),
        detail: Some(detail.into()),
        ..CompletionItem::default()
    });
}

struct DocumentIndex {
    imports: Vec<(Vec<String>, String)>,
    functions: Vec<String>,
    classes: Vec<String>,
    bindings: Vec<String>,
}

fn document_index(source: &SourceFile) -> DocumentIndex {
    let mut index = DocumentIndex {
        imports: Vec::new(),
        functions: Vec::new(),
        classes: Vec::new(),
        bindings: Vec::new(),
    };
    let Ok(tokens) = lex(source) else {
        return index;
    };
    let kinds = tokens.iter().map(|token| &token.kind).collect::<Vec<_>>();
    let mut i = 0;
    while i < kinds.len() {
        match kinds[i] {
            TokenKind::Keyword(word) if word == "IMPORT" => {
                i += 1;
                let mut path = Vec::new();
                while let Some(name) = ident_or_host(kinds.get(i)) {
                    path.push(name);
                    i += 1;
                    if matches!(
                        kinds.get(i),
                        Some(TokenKind::Symbol(bn_frontend::token::Symbol::Dot))
                    ) {
                        i += 1;
                    } else {
                        break;
                    }
                }
                let alias = if kinds
                    .get(i)
                    .is_some_and(|kind| matches!(kind, TokenKind::Keyword(word) if word == "AS"))
                {
                    i += 1;
                    ident_or_host(kinds.get(i))
                        .unwrap_or_else(|| path.last().cloned().unwrap_or_default())
                } else {
                    path.last().cloned().unwrap_or_default()
                };
                if !path.is_empty() && !alias.is_empty() {
                    index.imports.push((path, alias));
                }
            }
            TokenKind::Keyword(word) if word == "FUNCTION" => {
                i += 1;
                if let Some(name) = ident_or_host(kinds.get(i)) {
                    index.functions.push(name);
                }
            }
            TokenKind::Keyword(word) if word == "CLASS" => {
                i += 1;
                if let Some(name) = ident_or_host(kinds.get(i)) {
                    index.classes.push(name);
                }
            }
            TokenKind::Keyword(word) if word == "LET" || word == "CONST" => {
                i += 1;
                if let Some(name) = ident_or_host(kinds.get(i)) {
                    index.bindings.push(name);
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    index
}

fn ident_or_host(kind: Option<&&TokenKind>) -> Option<String> {
    match kind {
        Some(TokenKind::Identifier(name)) => Some(name.clone()),
        Some(TokenKind::Keyword(name)) => Some((*name).clone()),
        _ => None,
    }
}

fn resolve_chain(chain: &[String], imports: &[(Vec<String>, String)]) -> String {
    if chain.is_empty() {
        return String::new();
    }
    if chain[0] == "HOST" {
        return chain.join(".");
    }
    if let Some((path, _)) = imports.iter().find(|(_, alias)| alias == &chain[0]) {
        let mut full = path.clone();
        full.extend(chain.iter().skip(1).cloned());
        return full.join(".");
    }
    chain.join(".")
}

/// Completion members for `path`: the frontend's HOST catalog for catalog
/// owners (`Error`, `HOST.Net`, `FS.File`, …), and the few namespaces that are
/// not catalog tables.
fn members_of(path: &str) -> Vec<String> {
    let owner = if path == "HOST.FileSystem.File" {
        "FS.File"
    } else {
        path
    };
    let members = bn_frontend::host_member_names(owner);
    if !members.is_empty() {
        return members;
    }
    let local: &[&str] = match path {
        "HOST" => &[
            "Args",
            "Clock",
            "Console",
            "Exec",
            "FileSystem",
            "Net",
            "NumProcs",
            "Random",
        ],
        "Date" | "Time" => &["Parse"],
        "TimeZone" => &["IsDST", "Offset", "Parse"],
        "Timestamp" => &["AtZone", "Format", "FromLocal", "Parse"],
        _ => &[],
    };
    local.iter().map(|member| (*member).to_string()).collect()
}

fn module_exports(
    path: &str,
    source: &SourceFile,
    documents: &HashMap<String, SourceFile>,
    workspace_root: Option<&Path>,
) -> Vec<String> {
    let Some(name) = path.split('.').next() else {
        return Vec::new();
    };
    if !STANDARD_MODULES.contains(&name) {
        return Vec::new();
    }
    if let Some(document) = documents.values().find(|document| {
        document
            .name
            .rsplit('/')
            .next()
            .is_some_and(|file| file == format!("{name}.bn"))
    }) {
        return export_names(document);
    }
    let Some(path) = find_standard_module(name, &source.name, workspace_root) else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    export_names(&SourceFile::new(path.display().to_string(), text))
}

fn find_standard_module(
    name: &str,
    document_uri: &str,
    workspace_root: Option<&Path>,
) -> Option<PathBuf> {
    let file = format!("{name}.bn");
    let mut candidates = Vec::new();
    if let Some(root) = workspace_root {
        candidates.push(root.join("modules/bn").join(&file));
        candidates.push(root.join(&file));
    }
    if let Some(path) = file_path(document_uri) {
        let mut dir = path.parent();
        while let Some(current) = dir {
            candidates.push(current.join("modules/bn").join(&file));
            dir = current.parent();
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("modules/bn").join(&file));
        candidates.push(cwd.join("basicnext/modules/bn").join(&file));
    }
    candidates.into_iter().find(|path| path.is_file())
}

fn export_names(source: &SourceFile) -> Vec<String> {
    let Ok(tokens) = lex(source) else {
        return Vec::new();
    };
    let kinds = tokens.iter().map(|token| &token.kind).collect::<Vec<_>>();
    let mut names = Vec::new();
    let mut i = 0;
    while i < kinds.len() {
        let exported =
            matches!(kinds[i], TokenKind::Keyword(word) if word == "EXPORT" || word == "PUBLIC");
        if exported {
            i += 1;
        }
        match kinds.get(i) {
            Some(TokenKind::Keyword(word))
                if *word == "FUNCTION"
                    || *word == "STATIC"
                    || *word == "CLASS"
                    || *word == "STRUCT"
                    || *word == "CONST" =>
            {
                i += 1;
                if let Some(name) = ident_or_host(kinds.get(i)) {
                    names.push(name);
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn import_context(before: &str) -> bool {
    before
        .trim_start()
        .strip_prefix("IMPORT")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

pub(super) fn text_before(source: &SourceFile, position: Position) -> String {
    source
        .text
        .lines()
        .nth(position.line as usize)
        .map(|line| {
            let mut units = 0;
            line.chars()
                .take_while(|character| {
                    let width = if character.len_utf16() == 1 { 1 } else { 2 };
                    if units + width > position.character {
                        return false;
                    }
                    units += width;
                    true
                })
                .collect()
        })
        .unwrap_or_default()
}

fn dotted_query(before: &str) -> (Vec<String>, String) {
    let prefix = trailing_ident(before);
    let head = &before[..before.len().saturating_sub(prefix.len())];
    let trimmed = head.trim_end();
    if !trimmed.ends_with('.') {
        return (Vec::new(), prefix.to_string());
    }
    let mut chain = Vec::new();
    let mut rest = trimmed.trim_end_matches('.').trim_end();
    loop {
        let ident = trailing_ident(rest);
        if ident.is_empty() {
            break;
        }
        chain.push(ident.to_string());
        rest = rest[..rest.len().saturating_sub(ident.len())].trim_end();
        if let Some(stripped) = rest.strip_suffix('.') {
            rest = stripped.trim_end();
        } else {
            break;
        }
    }
    chain.reverse();
    (chain, prefix.to_string())
}

fn trailing_ident(text: &str) -> &str {
    let start = text
        .char_indices()
        .rev()
        .take_while(|(_, character)| character.is_ascii_alphanumeric() || *character == '_')
        .map(|(index, _)| index)
        .last()
        .unwrap_or(text.len());
    if start == text.len() {
        ""
    } else {
        &text[start..]
    }
}

fn add_semantic_members(
    source: &SourceFile,
    path: &str,
    items: &mut BTreeMap<String, CompletionItem>,
    prefix: &str,
) {
    let parsed = if let Ok(tokens) = lex(source)
        && let Ok(prog) = parse_named(&tokens, source.name.clone())
    {
        Some(prog)
    } else {
        let lines: Vec<&str> = source.text.lines().collect();
        if lines.len() > 1 {
            let prior = lines[..lines.len() - 1].join("\n");
            let prior_source = SourceFile::new(source.name.clone(), prior);
            lex(&prior_source)
                .ok()
                .and_then(|tokens| parse_named(&tokens, prior_source.name.clone()).ok())
        } else {
            None
        }
    };

    let Some(program) = parsed else {
        return;
    };

    let mut target_type = path.to_string();
    if let Ok(model) = analyze(&program)
        && let Some(sym) = model.symbols.iter().find(|s| s.name == path)
    {
        target_type.clone_from(&sym.type_name);
    }
    if target_type == path {
        // Fallback: search AST for a binding named `path` to get its type name
        for item in &program.items {
            if let bn_frontend::ast::Item::Declaration { statements, .. } = item {
                for stmt in statements {
                    if let bn_frontend::ast::Statement::Binding { name, type_ref, .. } = stmt
                        && name == path
                        && let Some(atom) = type_ref.alternatives.first()
                    {
                        target_type.clone_from(&atom.name);
                        break;
                    }
                }
            }
        }
    }

    for item in &program.items {
        if let bn_frontend::ast::Item::Declaration {
            kind,
            name,
            statements,
            ..
        } = item
        {
            let is_target = name == &target_type;
            let is_struct = matches!(kind, bn_frontend::ast::DeclarationKind::Struct);
            if is_target {
                for stmt in statements {
                    match stmt {
                        bn_frontend::ast::Statement::MemberFunction {
                            name: member_name,
                            visibility: Some(bn_frontend::ast::Visibility::Public),
                            ..
                        } => {
                            add(
                                items,
                                member_name,
                                CompletionItemKind::METHOD,
                                &target_type,
                                prefix,
                            );
                        }
                        bn_frontend::ast::Statement::Binding {
                            name: member_name,
                            visibility,
                            ..
                        } if matches!(visibility, Some(bn_frontend::ast::Visibility::Public))
                            || (is_struct && visibility.is_none()) =>
                        {
                            add(
                                items,
                                member_name,
                                CompletionItemKind::FIELD,
                                &target_type,
                                prefix,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Autocomplete helper for interactive line editors (such as `bnr`).
///
/// Returns `(replacement_start_byte_offset, candidate_labels)`.
#[must_use]
pub fn complete_interactive(
    line: &str,
    char_pos: usize,
    context: Option<&str>,
    workspace_root: Option<&Path>,
) -> (usize, Vec<String>) {
    let (full_source, line_offset) = match context {
        Some(ctx) if !ctx.trim().is_empty() => {
            let trimmed = ctx.trim_end_matches(['\r', '\n']);
            let count = u32::try_from(trimmed.lines().count()).unwrap_or(0);
            (format!("{trimmed}\n{line}"), count)
        }
        _ => (line.to_string(), 0),
    };
    let source_file = SourceFile::new("<interactive>", full_source);
    let position = Position::new(line_offset, u32::try_from(char_pos).unwrap_or(0));
    let documents = HashMap::new();
    let items = completion_items(&source_file, position, &documents, workspace_root);

    let before = text_before(&source_file, position);
    let (_, prefix) = dotted_query(&before);
    let word_start = char_pos.saturating_sub(prefix.len());

    let candidates = items.into_iter().map(|item| item.label).collect();
    (word_start, candidates)
}

fn file_path(uri: &str) -> Option<PathBuf> {
    file_uri_to_path(uri)
}
