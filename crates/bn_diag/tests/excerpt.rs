// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The source excerpt under a diagnostic: its gutter is as wide as the line
//! number, every `|` lines up, and the `^` sits under the reported column,
//! in both renderers (bucket 0.6.11, S6.11).

use bn_diag::{Catalog, Diagnostic};
use bn_source::{Position, SourceFile, Span};

/// A program whose line `line` reads `LET x = y`, and a diagnostic at
/// column 7 (the `=`) of that line.
fn diagnostic_at_line(line: usize) -> (SourceFile, Diagnostic) {
    let text = "\n".repeat(line - 1) + "LET x = y\n";
    let source = SourceFile::new("arquivo.bn", text);
    let position = Position {
        source_id: source.source_id,
        revision: source.revision,
        offset: line - 1 + 6,
        line,
        column: 7,
    };
    let diagnostic = Diagnostic {
        code: "E0100",
        message: "mensagem".into(),
        span: Span {
            start: position,
            end: position,
        },
        structured: None,
    };
    (source, diagnostic)
}

/// Every `|` of the excerpt sits in one column and the `^` sits under
/// column 7 of the source line, whatever the width of the line number.
fn assert_aligned(rendered: &str, line: usize) {
    let width = line.to_string().len();
    let gutter = " ".repeat(width);
    let lines: Vec<&str> = rendered.lines().collect();
    assert_eq!(lines[1], format!("{gutter}--> arquivo.bn:{line}:7"));
    assert_eq!(lines[2], format!("{gutter} |"));
    assert_eq!(lines[3], format!("{line} | LET x = y"));
    assert!(
        lines[4].starts_with(&format!("{gutter} |       ^")),
        "{}",
        lines[4]
    );
    // Column 7 of `LET x = y` is the `=`, after the `<line> | ` prefix.
    let caret = width + " | ".len() + 6;
    assert_eq!(lines[3].as_bytes()[caret], b'=');
    assert_eq!(lines[4].find('^'), Some(caret));
    for excerpt_line in &lines[2..5] {
        assert_eq!(excerpt_line.find('|'), Some(width + 1), "{excerpt_line}");
    }
}

#[test]
fn excerpt_gutter_follows_a_one_digit_line() {
    let (source, diagnostic) = diagnostic_at_line(4);
    assert_aligned(&diagnostic.render(&source), 4);
    assert_aligned(
        &diagnostic.render_with_catalog(&source, Catalog::embedded_global()),
        4,
    );
}

#[test]
fn excerpt_gutter_follows_a_four_digit_line() {
    let (source, diagnostic) = diagnostic_at_line(1234);
    assert_aligned(&diagnostic.render(&source), 1234);
    let rendered = diagnostic.render_with_catalog(&source, Catalog::embedded_global());
    assert_aligned(&rendered, 1234);
    let explanation: Vec<&str> = rendered
        .lines()
        .filter(|l| l.trim_start().starts_with("= cause") || l.trim_start().starts_with("= help"))
        .collect();
    assert!(!explanation.is_empty());
    assert!(
        explanation.iter().all(|l| l.starts_with("     = ")),
        "{explanation:?}"
    );
}
