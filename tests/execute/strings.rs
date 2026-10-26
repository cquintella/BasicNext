// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;

#[test]
fn print_concatenates_with_plus_and_separates_comma_arguments() {
    let source = "FUNCTION Start() AS VOID\nPRINT \"CASA\" + \"SHO\"\nPRINT \"CASA\", \"SHO\"\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute PRINT source");
    assert_eq!(output, "CASASHO\nCASA SHO\n");
}

#[test]
fn indexes_unicode_strings_by_scalar() {
    let source = "FUNCTION Start() AS VOID\nLET text AS STRING = \"café\"\nPRINT text[3], LEN(text)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "é 4\n");
}

#[test]
fn string_index_out_of_bounds_is_diagnosed() {
    let source =
        "FUNCTION Start() AS VOID\nLET text AS STRING = \"é\"\nPRINT text[1]\nEND FUNCTION\n";
    let error = run(source, "").expect_err("string index must be bounds-checked");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn asc_and_char_convert_unicode_scalars() {
    let source = "FUNCTION Start() AS VOID\nLET a AS INTEGER OR Error = ASC(\"é\")\nLET c AS STRING OR Error = CHAR(66)\nPRINT a, c\nLET empty AS INTEGER OR Error = ASC(\"\")\nIF empty IS Error THEN\nPRINT empty.Code\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute ASC and CHAR");
    assert_eq!(output, "233 B\n1\n");
}

#[test]
fn input_statement_into_string_target_reads_line() {
    let source = "FUNCTION Start() AS VOID\nLET name AS STRING = \"initial\"\nINPUT name\nPRINT \"Nome: [\", name, \"]\"\nEND FUNCTION\n";
    let (_, output) = run(source, "Carlos\n").expect("execute INPUT name");
    assert_eq!(output, "Nome: [ Carlos ]\n");
}

#[test]
fn input_statement_into_string_target_assigns_empty_on_eof() {
    let source = "FUNCTION Start() AS VOID\nLET name AS STRING = \"initial\"\nINPUT name\nPRINT \"Nome: [\", name, \"]\"\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute INPUT name on EOF");
    assert_eq!(output, "Nome: [  ]\n");
}

#[test]
fn input_expression_fails_to_bind_to_string_target() {
    let source = "FUNCTION Start() AS VOID\nLET name AS STRING = INPUT()\nEND FUNCTION\n";
    let error = run(source, "").expect_err("INPUT() expression must not bind directly to STRING");
    assert_eq!(error.code, "TYPE_MISMATCH");
}
