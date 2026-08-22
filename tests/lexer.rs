use std::{fs, path::Path};

use bn::{
    lexer::lex,
    source::SourceFile,
    token::{Symbol, TokenKind},
};

fn lex_path(path: &str) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    lex(&SourceFile::new(path, text))
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.message)
}

#[test]
fn valid_grammar_fixtures_are_lexically_valid() {
    let directory = Path::new("tests/grammar/valid");
    for entry in fs::read_dir(directory).expect("valid fixture directory") {
        let path = entry.expect("fixture entry").path();
        if path.extension().is_some_and(|extension| extension == "bn") {
            lex_path(path.to_str().expect("UTF-8 path")).expect("valid lexical fixture");
        }
    }
}

#[test]
fn lexical_error_fixtures_are_rejected() {
    for path in [
        "tests/grammar/invalid/bad-escape.bn",
        "tests/grammar/invalid/caret-exponentiation.bn",
        "tests/grammar/invalid/malformed-binary.bn",
        "tests/grammar/invalid/malformed-hexadecimal.bn",
        "tests/grammar/invalid/malformed-number.bn",
        "tests/grammar/invalid/unterminated-comment.bn",
        "tests/grammar/invalid/unterminated-string.bn",
    ] {
        assert!(lex_path(path).is_err(), "{path} must fail lexical analysis");
    }
}

#[test]
fn token_stream_and_spans_are_exact() {
    let source = SourceFile::new(
        "tokens.bn",
        "LET value AS FLOAT = 0x0F ** 2.0 // note\n/* block\n*/ NAN -INF += \"x\\\\y\"\n",
    );
    let tokens = lex(&source).expect("lex token stream");
    let kinds: Vec<TokenKind> = tokens.iter().map(|token| token.kind.clone()).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Keyword("LET".into()),
            TokenKind::Identifier("value".into()),
            TokenKind::Keyword("AS".into()),
            TokenKind::Keyword("FLOAT".into()),
            TokenKind::Symbol(Symbol::Assign),
            TokenKind::Integer("0x0F".into()),
            TokenKind::Symbol(Symbol::Power),
            TokenKind::Float("2.0".into()),
            TokenKind::Newline,
            TokenKind::Newline,
            TokenKind::Special("NAN"),
            TokenKind::Symbol(Symbol::Minus),
            TokenKind::Special("INF"),
            TokenKind::Symbol(Symbol::PlusAssign),
            TokenKind::String("x\\y".into()),
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
    assert_eq!(
        (tokens[0].span.start.line, tokens[0].span.start.column),
        (1, 1)
    );
    assert_eq!(
        (tokens[9].span.start.line, tokens[9].span.start.column),
        (2, 9)
    );
}

#[test]
fn crlf_and_missing_final_newline_emit_one_newline() {
    for text in ["LET x AS INTEGER = 1\r\n", "LET x AS INTEGER = 1"] {
        let tokens = lex(&SourceFile::new("line-endings.bn", text)).expect("lex line ending");
        assert_eq!(
            tokens
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Newline))
                .count(),
            1
        );
        assert!(matches!(
            tokens.last().map(|token| &token.kind),
            Some(TokenKind::Eof)
        ));
    }
}
