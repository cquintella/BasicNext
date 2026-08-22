use std::io::Cursor;

use bn::{
    ir::lower, lexer::lex, parser::parse_named, runtime::execute, semantic::analyze,
    source::SourceFile,
};

fn run(source_text: &str, input: &str) -> Result<(u8, String), bn::diagnostic::Diagnostic> {
    let source = SourceFile::new("runtime.bn", source_text);
    let tokens = lex(&source).expect("lex source");
    let program = parse_named(&tokens, &source.name).expect("parse source");
    let model = analyze(&program).expect("analyze source");
    let module = lower(&program, &model).expect("lower source");
    let mut input = Cursor::new(input.as_bytes());
    let mut output = Vec::new();
    let code = execute(&module, &mut input, &mut output)?;
    Ok((code, String::from_utf8(output).expect("UTF-8 output")))
}

#[test]
fn executes_calls_loops_and_checked_arithmetic() {
    let source = "FUNCTION Factorial(value AS INTEGER) AS INTEGER\nIF value = 0 THEN\nRETURN 1\nELSE\nRETURN value * Factorial(value - 1)\nEND IF\nEND FUNCTION\nFUNCTION Start() AS INTEGER\nLET total AS INTEGER = Factorial(5)\nLET quotient AS INTEGER = -5 DIV 3\nLET remainder AS INTEGER = -5 % 3\nPRINT total, quotient, remainder\nRETURN 7\nEND FUNCTION\n";
    let (code, output) = run(source, "").expect("execute source");
    assert_eq!(code, 7);
    assert_eq!(output, "120-21\n");
}

#[test]
fn boolean_and_or_short_circuit() {
    let source = "FUNCTION Unexpected() AS BOOLEAN\nPRINT \"unexpected\"\nRETURN TRUE\nEND FUNCTION\nFUNCTION Start() AS VOID\nIF FALSE AND Unexpected() THEN\nPRINT \"bad\"\nEND IF\nIF TRUE OR Unexpected() THEN\nPRINT \"ok\"\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "ok\n");
}

#[test]
fn floating_division_uses_ieee_special_values() {
    let source = "FUNCTION Start() AS VOID\nPRINT 1 / 0, 0 / 0\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "INFNAN\n");
}

#[test]
fn checked_integer_overflow_is_a_runtime_error() {
    let source = "FUNCTION Start() AS VOID\nLET value AS INT8 = 127\nvalue += 1\nEND FUNCTION\n";
    let error = run(source, "").expect_err("INT8 overflow must fail");
    assert_eq!(error.code, "NUMERIC_OVERFLOW");
}

#[test]
fn executes_vectors_foreach_input_and_math() {
    let source = "FUNCTION Start() AS VOID\nLET values AS INTEGER[3] = [1, 2, 3]\nLET total AS INTEGER = 0\nFOR EACH item AS INTEGER IN values\ntotal += item\nEND FOR\nLET line AS STRING OR EOF = INPUT()\nPRINT total, Math.SQRT(9.0), line\nEND FUNCTION\n";
    let (_, output) = run(source, "ready\n").expect("execute source");
    assert_eq!(output, "63.0ready\n");
}

#[test]
fn converts_timestamps_to_utc_components() {
    let source = "FUNCTION Start() AS VOID\nPRINT Math.TOHOUR(0 AS TIMESTAMP), Math.TOWEEKDAY(0 AS TIMESTAMP)\nPRINT Math.TOHOUR(-1 AS TIMESTAMP), Math.TOWEEKDAY(-1 AS TIMESTAMP)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute timestamp conversion");
    assert_eq!(output, "04\n233\n");
}

#[test]
fn stop_propagates_through_function_calls() {
    let source = "FUNCTION Halt() AS VOID\nSTOP 23\nEND FUNCTION\nFUNCTION Start() AS VOID\nHalt()\nEND FUNCTION\n";
    let (code, output) = run(source, "").expect("execute source");
    assert_eq!(code, 23);
    assert!(output.is_empty());
}

#[test]
fn primitive_and_vector_bindings_receive_defaults() {
    let source = "FUNCTION Start() AS VOID\nLET count AS INTEGER\nLET ready AS BOOLEAN\nLET text AS STRING\nLET values AS INTEGER[2]\nPRINT count, ready, text, values[1]\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "0FALSE0\n");
}

#[test]
fn try_parse_returns_an_inspectable_error_value() {
    let source = "FUNCTION Start() AS VOID\nLET parsed AS FLOAT OR Error = Float.TryParse(\"bad\")\nIF parsed IS Error THEN\nPRINT parsed.Code, parsed.Message\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "1'bad' is not a FLOAT\n");
}
