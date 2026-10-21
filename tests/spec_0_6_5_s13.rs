// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Spec 0.6.5 item S.13: the decided control-flow, module, vector, narrowing
//! and value rules, on the interpreter and on the native compiler.

mod support;

use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use bn_diag::Diagnostic;
use bn_frontend::lexer::lex;
use bn_frontend::lowering::lower_graph;
use bn_frontend::module_graph::load;
use bn_frontend::parser::parse;
use bn_frontend::semantic::analyze;
use bn_interp::{HostEnv, execute_with_host};
use bn_interpret_driver::environment::HostEnvDefaults;
use bn_source::SourceFile;
use support::{TestDir, bnc, compile_native, run};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

fn program(body: &str) -> String {
    format!("FUNCTION Start() AS VOID\n{body}\nEND FUNCTION\n")
}

fn diagnostic_of(source: &str) -> Diagnostic {
    let file = SourceFile::new("s13.bn", source);
    let tokens = lex(&file).expect("lex");
    match parse(&tokens) {
        Ok(parsed) => analyze(&parsed).expect_err("expected a diagnostic"),
        Err(diagnostic) => diagnostic,
    }
}

fn interpret_file(path: &Path) -> Result<(u8, String), Diagnostic> {
    let graph = load(path).map_err(|error| *error.diagnostic)?;
    let models =
        bn_frontend::semantic::analyze_modules(&graph).map_err(|error| *error.diagnostic)?;
    let module = lower_graph(&graph, &models)?;
    let host = HostEnv::fixed(vec!["s13.bn".into()], 0, 0).with_default_providers();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let code = execute_with_host(&module, &mut input, &mut output, &host)?;
    Ok((code, String::from_utf8(output).expect("UTF-8 output")))
}

fn interpret(source: &str) -> Result<(u8, String), Diagnostic> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "bn-s13-{}-{}.bn",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, source).expect("write source");
    let result = interpret_file(&path);
    let _ = fs::remove_file(&path);
    result
}

fn execute(mut command: Command) -> support::ProcessOutput {
    run(&mut command, None, PROCESS_TIMEOUT).expect("run child process")
}

fn write_program(directory: &TestDir, source: &str) -> PathBuf {
    let path = directory.join("program.bn");
    fs::write(&path, source).expect("write source");
    path
}

fn both(source: &str, expected: &str) {
    let (code, stdout) = interpret(source).expect("interpret");
    assert_eq!(code, 0, "{stdout}");
    assert_eq!(stdout, expected);
    let directory = TestDir::new("s13").expect("create directory");
    let path = write_program(&directory, source);
    let artifact = compile_native(&path, &directory);
    let output = execute(Command::new(&artifact));
    assert!(
        output.status.success(),
        "native run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, expected.as_bytes());
}

fn reject(source: &str, code: &str) {
    let diagnostic = diagnostic_of(source);
    assert_eq!(diagnostic.code, code, "{}", diagnostic.message);
    let directory = TestDir::new("s13-reject").expect("create directory");
    let path = write_program(&directory, source);
    let mut command = bnc();
    command.arg(&path).arg("--emit").arg("llvm");
    let output = execute(command);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains(code), "{stderr}");
}

fn trap(source: &str, code: &str) {
    let error = interpret(source).expect_err("interpreter must stop");
    assert_eq!(error.code, code, "{}", error.message);
    let directory = TestDir::new("s13-trap").expect("create directory");
    let path = write_program(&directory, source);
    let artifact = compile_native(&path, &directory);
    let output = execute(Command::new(&artifact));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains(code), "{stderr}");
}

#[test]
fn single_line_if_exits_and_continues() {
    both(
        &program(
            "LET n AS INTEGER = 0\n\
             WHILE TRUE\n\
             IF TRUE THEN EXIT WHILE\n\
             n = 1\n\
             END WHILE\n\
             FOR i = 1 TO 3\n\
             IF i = 2 THEN CONTINUE FOR\n\
             n = n + 1\n\
             END FOR\n\
             PRINT n",
        ),
        "2\n",
    );
}

#[test]
fn counted_for_without_as_and_end_of_type() {
    both(
        &program(
            "LET n AS INTEGER = 0\n\
             FOR i = 1 TO 3\n\
             n = n + i\n\
             END FOR\n\
             LET small AS INTEGER = 0\n\
             FOR j AS INT8 = 125 TO 127\n\
             small = small + 1\n\
             END FOR\n\
             LET down AS INTEGER = 0\n\
             FOR k AS INT8 = -126 TO -128 STEP -1\n\
             down = down + 1\n\
             END FOR\n\
             LET byte AS INTEGER = 0\n\
             FOR b AS BYTE = 254 TO 255\n\
             byte = byte + 1\n\
             END FOR\n\
             PRINT n, small, down, byte",
        ),
        "6 3 3 2\n",
    );
}

#[test]
fn for_body_may_assign_the_variable_and_bounds_run_once() {
    both(
        "FUNCTION Next() AS INTEGER\n\
         PRINT \"e\"\n\
         RETURN 1\n\
         END FUNCTION\n\
         FUNCTION Start() AS VOID\n\
         LET n AS INTEGER = 0\n\
         FOR i = 1 TO 10\n\
         i = 10\n\
         n = n + 1\n\
         END FOR\n\
         FOR j = Next() TO Next()\n\
         PRINT j\n\
         END FOR\n\
         PRINT n\n\
         END FUNCTION\n",
        "e\ne\n1\n1\n",
    );
}

#[test]
fn for_literal_step_zero_and_oversized_bound_are_rejected() {
    reject(
        &program("FOR i = 1 TO 2 STEP 0\nPRINT i\nEND FOR"),
        "TYPE_MISMATCH",
    );
    reject(
        &program("FOR i = 1 TO 3000000000\nPRINT i\nEND FOR"),
        "TYPE_MISMATCH",
    );
    reject(
        &program("FOR i = 1.5 TO 3\nPRINT i\nEND FOR"),
        "TYPE_MISMATCH",
    );
}

#[test]
fn runtime_step_zero_is_invalid_for_step() {
    trap(
        &program("LET s AS INTEGER = 0\nFOR i = 1 TO 2 STEP s\nPRINT i\nEND FOR"),
        "INVALID_FOR_STEP",
    );
}

#[test]
fn for_each_string_scalars_and_flattened_vector() {
    both(
        &program(
            "LET text AS STRING = \"ação\"\n\
             LET built AS STRING = \"\"\n\
             FOR EACH c IN text\n\
             built = built + c\n\
             END FOR\n\
             LET grid AS INTEGER[2][3] = [[1, 2, 3], [4, 5, 6]]\n\
             LET n AS INTEGER = 0\n\
             FOR EACH item IN grid\n\
             n = n + item\n\
             END FOR\n\
             PRINT built, n",
        ),
        "ação 21\n",
    );
    reject(
        &program("FOR EACH c AS INTEGER IN \"ab\"\nPRINT c\nEND FOR"),
        "TYPE_MISMATCH",
    );
}

#[test]
fn for_variable_is_not_visible_after_the_loop() {
    reject(
        &program("FOR i = 1 TO 2\nPRINT i\nEND FOR\nPRINT i"),
        "NAME_NOT_FOUND",
    );
    reject(
        &program("LET i AS INTEGER = 1\nFOR i = 1 TO 2\nPRINT i\nEND FOR"),
        "DUPLICATE_NAME",
    );
}

#[test]
fn stop_zero_ends_the_program() {
    both(&program("STOP 0"), "");
    reject(&program("STOP 256"), "INVALID_EXIT_CODE");
}

#[test]
fn string_order_identity_cast_and_power() {
    both(
        &program(
            "IF \"Z\" < \"a\" AND \"ab\" < \"abc\" AND \"\" < \"a\" THEN\n\
             PRINT \"ord\"\n\
             END IF\n\
             LET name AS STRING = \"BN\"\n\
             LET flag AS BOOLEAN = TRUE\n\
             LET narrow AS FLOAT32 = 1.5\n\
             PRINT name AS STRING, flag AS BOOLEAN, narrow AS FLOAT32\n\
             PRINT 2 ** -1, 2 ** 3\n\
             PRINT SIZEOF(\"ção\")",
        ),
        "ord\nBN TRUE 1.5\n0.5 8\n5\n",
    );
}

#[test]
fn negative_runtime_exponent_and_float32_overflow() {
    trap(
        "FUNCTION Exponent() AS INTEGER\n\
         RETURN -1\n\
         END FUNCTION\n\
         FUNCTION Start() AS VOID\n\
         PRINT 2 ** Exponent()\n\
         END FUNCTION\n",
        "INVALID_EXPONENT",
    );
    trap(
        &program(
            "LET big AS FLOAT64 = 1000000000000000000000000000000000000000.0\n\
             LET small AS FLOAT32 = big AS FLOAT32\n\
             PRINT small",
        ),
        "NUMERIC_OVERFLOW",
    );
}

#[test]
fn nan_survives_float32_cast() {
    both(
        &program(
            "LET n AS FLOAT32 = (0.0 / 0.0) AS FLOAT32\n\
             IF n IS NAN THEN\n\
             PRINT \"nan\"\n\
             END IF",
        ),
        "nan\n",
    );
}

#[test]
fn function_values_are_equal_and_sizeof_rejects_unsized_kinds() {
    both(
        "FUNCTION Id(n AS INTEGER) AS INTEGER\n\
         RETURN n\n\
         END FUNCTION\n\
         FUNCTION Start() AS VOID\n\
         LET a AS FUNCTION(INTEGER) AS INTEGER = Id\n\
         LET b AS FUNCTION(INTEGER) AS INTEGER = Id\n\
         IF a = b THEN\n\
         PRINT \"eq\"\n\
         END IF\n\
         END FUNCTION\n",
        "eq\n",
    );
    reject(
        "CLASS Box\n\
         PUBLIC FUNCTION CONSTRUCTOR()\n\
         END FUNCTION\n\
         END CLASS\n\
         FUNCTION Start() AS VOID\n\
         LET box AS Box = NEW Box()\n\
         PRINT SIZEOF(box)\n\
         END FUNCTION\n",
        "TYPE_MISMATCH",
    );
    reject(
        &program("LET zone AS TIMEZONE\nPRINT SIZEOF(zone)"),
        "TYPE_MISMATCH",
    );
    reject(
        &program("LET value AS INTEGER OR NULL = 1\nPRINT SIZEOF(value)"),
        "TYPE_MISMATCH",
    );
    reject(
        "FUNCTION Id(n AS INTEGER) AS INTEGER\n\
         RETURN n\n\
         END FUNCTION\n\
         FUNCTION Start() AS VOID\n\
         LET a AS FUNCTION(INTEGER) AS INTEGER = Id\n\
         PRINT SIZEOF(a)\n\
         END FUNCTION\n",
        "TYPE_MISMATCH",
    );
    reject(
        &program("LET region AS POINTER TO INTEGER[] = NEW INTEGER[1]\nPRINT SIZEOF(region)"),
        "TYPE_MISMATCH",
    );
}

#[test]
fn system_is_not_a_type() {
    let diagnostic = diagnostic_of(&program("LET x AS SYSTEM"));
    assert_eq!(diagnostic.code, "NAME_NOT_FOUND");
    assert!(
        diagnostic.message.contains("HOST.Args"),
        "{}",
        diagnostic.message
    );
}

#[test]
fn diagnostics_spell_integer_and_float_keywords() {
    let diagnostic = diagnostic_of(&program(
        "LET count AS INTEGER = 1\nLET ratio AS FLOAT = count",
    ));
    assert_eq!(diagnostic.code, "TYPE_MISMATCH");
    assert!(
        diagnostic.message.contains("FLOAT64") && diagnostic.message.contains("INT32"),
        "{}",
        diagnostic.message
    );
    assert!(
        !diagnostic.message.contains("INTEGER(INT32)"),
        "{}",
        diagnostic.message
    );
    assert!(
        !diagnostic.message.contains("FLOAT(FLOAT64)"),
        "{}",
        diagnostic.message
    );
}

#[test]
fn open_vector_parameters_and_rejected_open_forms() {
    both(
        "FUNCTION Sum(values AS INTEGER[]) AS INTEGER\n\
         RETURN LEN(values)\n\
         END FUNCTION\n\
         FUNCTION Width(grid AS INTEGER[][3]) AS INTEGER\n\
         RETURN 1\n\
         END FUNCTION\n\
         FUNCTION Id(values AS INTEGER[]) AS INTEGER\n\
         RETURN LEN(values)\n\
         END FUNCTION\n\
         FUNCTION Start() AS VOID\n\
         LET row AS INTEGER[3] = [4, 5, 6]\n\
         LET grid AS INTEGER[1][3] = [[1, 2, 3]]\n\
         LET f AS FUNCTION(INTEGER[]) AS INTEGER = Id\n\
         PRINT Sum(row), Width(grid), f(row)\n\
         END FUNCTION\n",
        "3 1 3\n",
    );
    reject(&program("LET values AS INTEGER[]"), "INVALID_VECTOR_TYPE");
    reject(
        "FUNCTION Bad() AS INTEGER[]\nRETURN [1]\nEND FUNCTION\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
        "INVALID_VECTOR_TYPE",
    );
    reject(
        "FUNCTION Bad(values AS INTEGER[3][]) AS INTEGER\nRETURN 1\nEND FUNCTION\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
        "TYPE_MISMATCH",
    );
}

#[test]
fn narrowing_through_and_or_and_while() {
    both(
        &program(
            "LET value AS INTEGER OR NULL = 1\n\
             IF value IS INTEGER AND value + 1 = 2 THEN\n\
             PRINT \"and\"\n\
             END IF\n\
             IF NOT (value IS INTEGER) OR value + 1 = 2 THEN\n\
             PRINT \"or\"\n\
             END IF\n\
             WHILE value IS INTEGER\n\
             PRINT value\n\
             value = NULL\n\
             END WHILE",
        ),
        "and\nor\n1\n",
    );
    reject(
        &program(
            "LET value AS INTEGER OR NULL = 1\n\
             WHILE value IS INTEGER\n\
             value = NULL\n\
             PRINT value + 1\n\
             END WHILE",
        ),
        "TYPE_MISMATCH",
    );
    reject(
        &program("LET value AS INTEGER OR NULL = 1\nPRINT value + 1"),
        "TYPE_MISMATCH",
    );
}

#[test]
fn recursive_struct_is_rejected_and_a_class_cycle_is_not() {
    reject(
        "STRUCT Node\nnext AS Node\nEND STRUCT\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
        "TYPE_MISMATCH",
    );
    reject(
        "STRUCT Node\nkids AS Node[1]\nEND STRUCT\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
        "TYPE_MISMATCH",
    );
    reject(
        "STRUCT Inner\nnode AS Node\nEND STRUCT\nSTRUCT Node\ninner AS Inner\nEND STRUCT\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
        "TYPE_MISMATCH",
    );
    both(
        "CLASS Node\n\
         PUBLIC next AS Node OR NULL = NULL\n\
         PUBLIC FUNCTION CONSTRUCTOR()\n\
         END FUNCTION\n\
         END CLASS\n\
         FUNCTION Start() AS VOID\n\
         PRINT \"class\"\n\
         END FUNCTION\n",
        "class\n",
    );
}

#[test]
fn private_module_constant_and_re_export() {
    let directory = TestDir::new("s13-mod").expect("create directory");
    fs::write(
        directory.join("Lib.bn"),
        "CONST Secret = 3\n\
         EXPORT FUNCTION Shown() AS INTEGER\n\
         RETURN Secret\n\
         END FUNCTION\n",
    )
    .expect("write Lib");
    let main = "IMPORT Lib AS Lib\n\
         FUNCTION Start() AS VOID\n\
         PRINT Lib.Shown()\n\
         END FUNCTION\n";
    fs::write(directory.join("main.bn"), main).expect("write main");
    let (code, stdout) = interpret_file(&directory.join("main.bn")).expect("interpret module");
    assert_eq!((code, stdout.as_str()), (0, "3\n"));
    let artifact = compile_native(directory.join("main.bn"), &directory);
    let output = execute(Command::new(&artifact));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"3\n");

    let hidden = "IMPORT Lib AS Lib\n\
         FUNCTION Start() AS VOID\n\
         PRINT Lib.Secret\n\
         END FUNCTION\n";
    fs::write(directory.join("hidden.bn"), hidden).expect("write hidden");
    let graph = load(directory.join("hidden.bn")).expect("load hidden");
    let diagnostic = bn_frontend::semantic::analyze_modules(&graph).expect_err("private constant");
    assert_eq!(diagnostic.diagnostic.code, "NAME_NOT_FOUND");

    let exported = TestDir::new("s13-export").expect("create directory");
    fs::write(
        exported.join("A.bn"),
        "EXPORT FUNCTION Value() AS INTEGER\nRETURN 7\nEND FUNCTION\n",
    )
    .expect("write A");
    fs::write(exported.join("B.bn"), "EXPORT IMPORT A AS Inner\n").expect("write B");
    fs::write(
        exported.join("main.bn"),
        "IMPORT B AS This\n\
         FUNCTION Start() AS VOID\n\
         PRINT This.Inner.Value()\n\
         END FUNCTION\n",
    )
    .expect("write export main");
    let (code, stdout) = interpret_file(&exported.join("main.bn")).expect("interpret re-export");
    assert_eq!((code, stdout.as_str()), (0, "7\n"));
    let artifact = compile_native(exported.join("main.bn"), &exported);
    let output = execute(Command::new(&artifact));
    assert_eq!(output.stdout, b"7\n");

    let cycle = TestDir::new("s13-cycle").expect("create directory");
    fs::write(cycle.join("A.bn"), "EXPORT IMPORT B AS Other\n").expect("write cycle A");
    fs::write(cycle.join("B.bn"), "EXPORT IMPORT A AS Other\n").expect("write cycle B");
    fs::write(
        cycle.join("main.bn"),
        "IMPORT A AS A\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
    )
    .expect("write cycle main");
    let diagnostic = load(cycle.join("main.bn")).expect_err("import cycle");
    assert_eq!(diagnostic.diagnostic.code, "IMPORT_CYCLE");

    reject(
        "EXPORT IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nEND FUNCTION\n",
        "E0101",
    );
}

#[test]
fn unreachable_file_closes_when_its_last_reference_ends() {
    let directory = TestDir::new("s13-file").expect("create directory");
    let path = directory.join("note.txt");
    let path_text = path.display().to_string();
    // A Windows path's `\` is an escape inside a BN string.
    let path_literal = path_text.replace('\\', "\\\\");
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\n\
         FUNCTION WriteOnly(path AS STRING) AS INTEGER\n\
         LET file AS FS.File OR Error = FS.Open(path, FS.WRITE)\n\
         IF file IS Error THEN\n\
         RETURN 1\n\
         END IF\n\
         LET wrote AS VOID OR Error = file.Write(\"hello\")\n\
         IF wrote IS Error THEN\n\
         RETURN 2\n\
         END IF\n\
         RETURN 0\n\
         END FUNCTION\n\
         FUNCTION Start() AS INTEGER\n\
         IF WriteOnly(\"{path_literal}\") <> 0 THEN\n\
         RETURN 1\n\
         END IF\n\
         LET file AS FS.File OR Error = FS.Open(\"{path_literal}\", FS.READ)\n\
         IF file IS Error THEN\n\
         RETURN 3\n\
         END IF\n\
         LET text AS STRING OR Error = file.ReadAll()\n\
         IF text IS Error THEN\n\
         RETURN 4\n\
         END IF\n\
         IF text <> \"hello\" THEN\n\
         RETURN 5\n\
         END IF\n\
         RETURN 0\n\
         END FUNCTION\n"
    );
    let (code, stdout) = interpret(&source).expect("interpret file");
    assert_eq!((code, stdout.as_str()), (0, ""), "{path_text}");
    let source_path = write_program(&directory, &source);
    let artifact = compile_native(&source_path, &directory);
    let output = execute(Command::new(&artifact));
    assert!(
        output.status.success(),
        "native file status {}: {}",
        output.status.code().unwrap_or(255),
        String::from_utf8_lossy(&output.stderr)
    );
}
