// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{fs, path::Path, process::Command};

use bn::{
    keyword_registry::{
        SPECIAL_BEGIN, SPECIAL_END, parse_ebnf_quoted_production, parse_keywords_md,
        parse_marked_list,
    },
    lexer::lex,
    source::SourceFile,
    token::{TokenKind, reserved_words, special_float_literals},
};

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("read {path}: {error}"))
}

fn lex_kinds(text: &str) -> Vec<TokenKind> {
    lex(&SourceFile::new("u1.bn", text))
        .unwrap_or_else(|error| panic!("lex {text:?}: {}", error.message))
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

#[test]
fn registry_matches_ebnf_and_generated_tables() {
    let registry = parse_keywords_md(&read("docs/language/0.4/keywords.md")).expect("registry");
    let ebnf = read("docs/language/0.4/0.4.ebnf");
    let reserved =
        parse_ebnf_quoted_production(&ebnf, "reserved-word").expect("EBNF reserved-word");
    let special = parse_ebnf_quoted_production(&ebnf, "special-float-literal")
        .expect("EBNF special-float-literal");
    assert_eq!(registry.reserved, reserved);
    assert_eq!(registry.special_literals, special);
    assert_eq!(
        registry.reserved,
        reserved_words()
            .iter()
            .map(|word| (*word).to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        registry.special_literals,
        special_float_literals()
            .iter()
            .map(|word| (*word).to_string())
            .collect::<Vec<_>>()
    );
    assert!(registry.reserved.contains(&"IF".to_string()));
    assert_eq!(
        registry.special_literals,
        vec!["INF".to_string(), "NAN".to_string()]
    );

    let mut missing_if = registry.reserved.clone();
    missing_if.retain(|word| word != "IF");
    assert_ne!(missing_if, reserved);
}

#[test]
fn registry_parser_rejects_invalid_lists() {
    let valid = "<!-- reserved-words:start -->\nIF\nLET\n<!-- reserved-words:end -->";
    parse_marked_list(
        valid,
        "<!-- reserved-words:start -->",
        "<!-- reserved-words:end -->",
        "reserved word",
    )
    .expect("valid list");

    let cases = [
        (
            "<!-- reserved-words:start -->\n<!-- reserved-words:end -->",
            "empty",
        ),
        (
            "<!-- reserved-words:start -->\n123\n<!-- reserved-words:end -->",
            "leading digits",
        ),
        (
            "<!-- reserved-words:start -->\nif\n<!-- reserved-words:end -->",
            "lowercase",
        ),
        (
            "<!-- reserved-words:start -->\nIF!\n<!-- reserved-words:end -->",
            "punctuation",
        ),
        (
            "<!-- reserved-words:start -->\nLET\nIF\n<!-- reserved-words:end -->",
            "unsorted",
        ),
        (
            "<!-- reserved-words:start -->\nIF\nIF\n<!-- reserved-words:end -->",
            "duplicate",
        ),
        (
            "<!-- reserved-words:start -->\nIF\n<!-- reserved-words:end -->\n<!-- reserved-words:start -->",
            "duplicate start marker",
        ),
        (
            "<!-- reserved-words:end -->\nIF\n<!-- reserved-words:start -->",
            "reversed markers",
        ),
    ];
    for (text, label) in cases {
        assert!(
            parse_marked_list(
                text,
                "<!-- reserved-words:start -->",
                "<!-- reserved-words:end -->",
                "reserved word",
            )
            .is_err(),
            "{label} must fail"
        );
    }

    let duplicate_special = read("docs/language/0.2/keywords.md")
        .replace(SPECIAL_BEGIN, &format!("{SPECIAL_BEGIN}\n{SPECIAL_BEGIN}"));
    assert!(parse_keywords_md(&duplicate_special).is_err());
    let _ = SPECIAL_END;
}

#[test]
fn lexer_classifies_every_generated_entry() {
    for word in reserved_words() {
        let kinds = lex_kinds(&format!("{word}\n"));
        assert!(
            matches!(kinds.first(), Some(TokenKind::Keyword(text)) if text == word),
            "{word} must lex as a keyword"
        );
        let lower = word.to_ascii_lowercase();
        let kinds = lex_kinds(&format!("{lower}\n"));
        assert!(
            matches!(kinds.first(), Some(TokenKind::Identifier(text)) if text == &lower),
            "{lower} must lex as an identifier"
        );
        let mut mixed = word.to_string();
        mixed.replace_range(..1, &word[..1].to_ascii_lowercase());
        if mixed != *word {
            let kinds = lex_kinds(&format!("{mixed}\n"));
            assert!(
                matches!(kinds.first(), Some(TokenKind::Identifier(text)) if text == &mixed),
                "{mixed} must lex as an identifier"
            );
        }
    }
    for literal in special_float_literals() {
        let kinds = lex_kinds(&format!("{literal}\n"));
        assert_eq!(kinds.first(), Some(&TokenKind::Special(literal)));
        let lower = literal.to_ascii_lowercase();
        let kinds = lex_kinds(&format!("{lower}\n"));
        assert!(matches!(
            kinds.first(),
            Some(TokenKind::Identifier(text)) if text == &lower
        ));
    }
    let kinds = lex_kinds("NAN -INF\n");
    assert_eq!(
        &kinds[..4],
        &[
            TokenKind::Special("NAN"),
            TokenKind::Symbol(bn::token::Symbol::Minus),
            TokenKind::Special("INF"),
            TokenKind::Newline,
        ]
    );
}

#[test]
fn non_reserved_names_remain_identifiers() {
    for name in [
        "ASC", "CHAR", "CLS", "BEEP", "PRINTAT", "OPEN", "CLOSE", "READ", "WRITE", "MATCH", "OK",
        "ERR", "Error",
    ] {
        let kinds = lex_kinds(&format!("{name}\n"));
        assert!(
            matches!(kinds.first(), Some(TokenKind::Identifier(text)) if text == name),
            "{name} must remain an identifier"
        );
    }
    for word in ["PARALLEL", "SYSTEM"] {
        let kinds = lex_kinds(&format!("{word}\n"));
        assert!(
            matches!(kinds.first(), Some(TokenKind::Keyword(text)) if text == word),
            "{word} must stay reserved"
        );
    }
    for literal in ["NAN", "INF"] {
        let kinds = lex_kinds(&format!("{literal}\n"));
        assert_eq!(kinds.first(), Some(&TokenKind::Special(literal)));
    }
}

#[test]
fn markdown_links_resolve() {
    let exceptions = [
        "AGENTS.md",
        "docs/language/0.1/0.1.md::../project/usage.md",
        "docs/language/0.1/0.1.md::diagnostics.md",
        "docs/language/0.1/0.1.md::../library/temporal.md",
        "docs/language/0.1/0.1.md::../library/math.md",
        "docs/language/0.1/0.1.md::../library/host.md",
    ];
    let mut failures = Vec::new();
    for root in ["docs", "ongoing", "done", "todo"] {
        walk_markdown(Path::new(root), &mut |path, text| {
            check_links(path, text, &exceptions, &mut failures);
        });
    }
    for name in [
        "README.md",
        "CONTRIBUTING.md",
        "PHILOSOPHY.md",
        "GOVERNANCE.md",
    ] {
        if Path::new(name).exists() {
            let text = read(name);
            check_links(Path::new(name), &text, &exceptions, &mut failures);
        }
    }
    assert!(
        failures.is_empty(),
        "broken Markdown links:\n{}",
        failures.join("\n")
    );
}

#[test]
fn done_tree_is_closed_and_todo_accepted_docs_are_pointers() {
    let mut failures = Vec::new();
    walk_markdown(Path::new("done"), &mut |path, text| {
        if text.contains("Status: Open") {
            failures.push(format!("{} still says Status: Open", path.display()));
        }
        if text.contains("- [ ]") {
            failures.push(format!("{} has an unchecked gate", path.display()));
        }
        if text.lines().any(|line| line.contains("TODO:")) {
            failures.push(format!("{} contains TODO:", path.display()));
        }
    });
    walk_markdown(Path::new("todo/proposals"), &mut |path, text| {
        if path.file_name().is_some_and(|name| name == "README.md") {
            return;
        }
        let accepted = text.contains("Accepted into") || text.contains("Accepted for");
        let pointer = text.contains("Historical proposal")
            || text.contains("This file is a pointer")
            || text.contains("the rest of this document remains proposed")
            || text.contains("The rest of this document remains proposed")
            || text.contains("Exploratory")
            || text.contains("unresolved");
        if accepted && !pointer {
            failures.push(format!(
                "{} is marked accepted under todo/ without remaining scope",
                path.display()
            ));
        }
    });
    assert!(
        failures.is_empty(),
        "workflow location failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn cargo_package_lists_build_script_and_keyword_registry() {
    let output = Command::new("cargo")
        .args(["package", "--list", "--allow-dirty"])
        .output()
        .expect("cargo package --list");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let list = String::from_utf8(output.stdout).expect("utf-8 package list");
    assert!(
        list.lines().any(|line| line.ends_with("build.rs")),
        "package list must contain build.rs\n{list}"
    );
    assert!(
        list.lines()
            .any(|line| line.ends_with("docs/language/0.2/keywords.md")),
        "package list must contain docs/language/0.2/keywords.md\n{list}"
    );
    assert!(
        !list
            .lines()
            .any(|line| line.ends_with("docs/language/keywords.md")),
        "package list must not use the removed docs/language/keywords.md path\n{list}"
    );
}

fn walk_markdown(root: &Path, visit: &mut impl FnMut(&Path, &str)) {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                let text = fs::read_to_string(&path).expect("read markdown");
                visit(&path, &text);
            }
        }
    }
}

fn check_links(path: &Path, text: &str, exceptions: &[&str], failures: &mut Vec<String>) {
    let key = path.to_string_lossy();
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let target = rest[..end].trim();
        rest = &rest[end + 1..];
        if target.is_empty()
            || target.starts_with('#')
            || target.starts_with("http://")
            || target.starts_with("https://")
            || target.starts_with("mailto:")
        {
            continue;
        }
        let target = target.split_once('#').map_or(target, |(path, _)| path);
        let exception = format!("{key}::{target}");
        if exceptions.contains(&key.as_ref()) || exceptions.iter().any(|item| *item == exception) {
            continue;
        }
        let resolved = path.parent().unwrap_or_else(|| Path::new(".")).join(target);
        if !resolved.exists() {
            failures.push(format!("{} -> {target}", path.display()));
        }
    }
}
