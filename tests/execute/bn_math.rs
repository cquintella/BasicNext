// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
#[test]
fn executes_bnmath_02_conversion_constants_and_statistics() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS INTEGER[4] = [1, 2, 2, 5]\nPRINT Math.VAL(\" 3,14\"), Math.MAX_INTEGER, Math.MEAN(values), Math.MEDIAN(values), Math.MODE(values), Math.RANGE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute BNMath 0.2");
    assert_eq!(output, "3.0 2147483647 2.5 2.0 2.0 4.0\n");
}

#[test]
fn bnmath_intrinsics_require_the_bnmath_module_identity() {
    let (_, output) = run_path("tests/modules/bnmath-provider-identity/main.bn")
        .expect("execute non-BNMath name collision");
    assert_eq!(output, "7\n");
}

#[test]
fn bnmath_vector_min_max_preserve_integer_type() {
    let (_, output) = run_path("tests/grammar/valid/bnmath-vector-minmax.bn")
        .expect("integer vector min/max execute");
    assert_eq!(output, "1 2\n");
}

#[test]
fn bnmath_min_max_reject_empty_vectors_of_every_numeric_type() {
    for declaration in ["INTEGER[0]", "FLOAT[0]"] {
        let source = format!(
            "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS {declaration} = []\nPRINT Math.MIN(values)\nEND FUNCTION\n"
        );
        let error = run(&source, "").expect_err("empty vector must have no minimum");
        assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
    }
}

#[test]
fn bnmath_min_max_reject_empty_numeric_regions() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS POINTER TO FLOAT[0] = NEW FLOAT[0]\nPRINT Math.MAX(values)\nEND FUNCTION\n";
    let error = run(source, "").expect_err("empty region must have no maximum");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn bnmath_mode_propagates_nan_before_considering_ties() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT[4] = [NAN, NAN, 1.0, 1.0]\nPRINT Math.MODE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute MODE with NAN");
    assert_eq!(output, "NAN\n");
}

#[test]
fn bnmath_mode_returns_na_for_a_non_nan_tie() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT[4] = [1.0, 1.0, 2.0, 2.0]\nPRINT Math.MODE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute MODE tie");
    assert_eq!(output, "NA\n");
}

#[test]
fn bnmath_vector_min_max_preserve_float32_rounding() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT32[2] = [16777217.0 AS FLOAT32, 1.0 AS FLOAT32]\nPRINT Math.MAX(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute FLOAT32 vector maximum");
    assert_eq!(output, "16777216.0\n");
}

#[test]
fn bnmath_mode_returns_na_for_an_empty_vector() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT[0] = []\nPRINT Math.MODE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("empty MODE is NA");
    assert_eq!(output, "NA\n");
}
