// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A destructuring `LET` reads the `STRUCT` fields it names, so they are not
//! `UNUSED_BINDING`; a field nothing reads still is (`ongoing/issues.md`, A15).

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze_with_warnings;
use bn_source::SourceFile;

const UNUSED_BINDING: &str = "UNUSED_BINDING";

fn unused_names(text: &str) -> Vec<String> {
    let source = SourceFile::new("unused_struct_field.bn", text);
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    let analysis = analyze_with_warnings(&program).expect("warning analysis");
    analysis
        .warnings
        .iter()
        .filter(|warning| warning.code == UNUSED_BINDING)
        .map(|warning| warning.message.to_string())
        .collect()
}

#[test]
fn destructured_struct_fields_are_read() {
    let warnings = unused_names(
        "STRUCT Site\n    name AS STRING\n    port AS INTEGER\n    secure AS BOOLEAN\nEND STRUCT\n\
         FUNCTION Start() AS VOID\n    LET site AS Site = [\"example\", 80, FALSE]\n\
         \x20   LET [name, port] = site\n    PRINT name, port\nEND FUNCTION\n",
    );
    assert_eq!(warnings, ["secure"]);
}

#[test]
fn vector_destructuring_reads_no_struct_field() {
    let warnings = unused_names(
        "STRUCT Point\n    X AS INTEGER\nEND STRUCT\n\
         FUNCTION Start() AS VOID\n    LET p AS Point = [1]\n    LET v AS INTEGER[2] = [1, 2]\n\
         \x20   LET [X, Y] = v\n    PRINT X, Y, p.X\nEND FUNCTION\n",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let warnings = unused_names(
        "STRUCT Point\n    X AS INTEGER\nEND STRUCT\n\
         FUNCTION Start() AS VOID\n    LET p AS Point = [1]\n    LET v AS INTEGER[1] = [1]\n\
         \x20   LET [X] = v\n    PRINT X, p\nEND FUNCTION\n",
    );
    assert_eq!(warnings, ["X"]);
}
