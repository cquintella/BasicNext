#![allow(clippy::wildcard_imports)]
use super::*;
use std::{collections::BTreeMap, path::Path};

use lsp_types::CompletionItemKind;

use crate::token::{TokenKind, reserved_words};

const STANDARD_MODULES: &[&str] = &["BNData", "BNDispatch", "BNJson", "BNLog", "BNMath", "BNWeb"];

pub(super) fn completion_items(
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
        } else {
            add_filtered(&mut items, catalog.iter().copied(), kind, &path, &prefix);
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
        let kind = if matches!(symbol.ty, crate::semantic::Type::Function { .. }) {
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
                        Some(TokenKind::Symbol(crate::token::Symbol::Dot))
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

fn members_of(path: &str) -> &'static [&'static str] {
    match path {
        "HOST" => &[
            "Args",
            "Clock",
            "Console",
            "FileSystem",
            "Net",
            "NumProcs",
            "Random",
        ],
        "HOST.Clock" => &["Now", "Timer"],
        "HOST.Console" => &["Beep", "Cls", "NumCols", "NumRows", "PrintAt"],
        "HOST.Random" => &["Random", "Seed"],
        "HOST.FileSystem" => &[
            "APPEND",
            "DeleteFile",
            "Exists",
            "File",
            "Open",
            "READ",
            "WRITE",
        ],
        "HOST.FileSystem.File" | "FS.File" => &[
            "Close",
            "ReadAll",
            "ReadBytes",
            "ReadLine",
            "Write",
            "WriteBytes",
            "WriteLine",
        ],
        "HOST.Net" => &[
            "Address",
            "Addresses",
            "CIDR",
            "Endpoint",
            "Neighbor",
            "Ping",
            "Resolve",
            "Reverse",
            "TCPConnect",
            "TCPListen",
            "TCPListener",
            "TCPStream",
            "UDPBind",
        ],
        "HOST.Net.Address" => &[
            "IsIPv4",
            "IsIPv6",
            "IsLinkLocal",
            "IsLoopback",
            "IsMulticast",
            "IsPrivate",
            "Parse",
            "ToString",
        ],
        "HOST.Net.CIDR" => &["Contains", "Network", "Parse", "PrefixLength"],
        "Error" => &["Code", "Message"],
        "Date" | "Time" | "TimeZone" => &["Parse"],
        "Timestamp" => &["Format", "Parse"],
        _ => &[],
    }
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
            Some(TokenKind::Keyword(word)) if *word == "FUNCTION" || *word == "STATIC" => {
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

fn file_path(uri: &str) -> Option<PathBuf> {
    uri.strip_prefix("file://").map(PathBuf::from)
}
