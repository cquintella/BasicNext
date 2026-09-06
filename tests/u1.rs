// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{fs, process::Command};

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
