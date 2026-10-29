// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The 0.6.12 `HOST.FileSystem` members type-check from the same table as
//! the older ones (language/0.6/host.md, "Directories, private files, copy
//! and move"; bucket 0.6.12, FS-FE-1): the lifecycle fixture passes, and a
//! wrong arity, argument or result type is a `TYPE_MISMATCH`.

use bn_frontend::lexer::lex;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_source::SourceFile;
use std::fs;

const FIXTURE: &str = "tests/host/fs_directories.bn";
const TYPE_MISMATCH: &str = "TYPE_MISMATCH";

fn analysis(text: String) -> Result<(), String> {
    let source = SourceFile::new("fs.bn", text);
    let tokens = lex(&source).expect("lex source");
    let program = parse(&tokens).expect("parse source");
    analyze(&program)
        .map(|_| ())
        .map_err(|diagnostic| diagnostic.code.to_string())
}

#[test]
fn the_lifecycle_fixture_type_checks() {
    let text = fs::read_to_string(FIXTURE).expect("read fixture");
    assert_eq!(analysis(text), Ok(()));
}

#[test]
fn wrong_arity_argument_or_result_is_a_type_mismatch() {
    for body in [
        "LET r AS VOID OR Error = FS.Copy(\"a\")",
        "LET r AS VOID OR Error = FS.Move(\"a\", 1)",
        "LET r AS BOOLEAN OR Error = FS.CreateDirectory(\"a\")",
        "LET r AS FS.File OR Error = FS.OpenNew()",
        "LET r AS STRING OR Error = FS.GetCurrentDirectory(\"a\")",
        "LET r AS INTEGER = FS.ALREADY_EXISTS + FS.IsDirectory(\"a\")",
    ] {
        let text = format!(
            "IMPORT HOST.FileSystem AS FS\n\nFUNCTION Start() AS VOID\n    {body}\n    PRINT r\nEND FUNCTION\n"
        );
        assert_eq!(analysis(text), Err(TYPE_MISMATCH.to_owned()), "{body}");
    }
}
