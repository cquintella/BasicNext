// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Destructuring `LET` and positional `STRUCT` literals (0.6.md, bucket S.11).

mod support;

use std::{
    fs,
    io::Cursor,
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
use support::{ProcessOutput, TestDir, bnc, bni, compile_native, run};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

fn program(body: &str) -> String {
    format!("FUNCTION Start() AS VOID\n{body}END FUNCTION\n")
}

fn diagnostic_of(source: &str) -> Diagnostic {
    let file = SourceFile::new("destructure.bn", source);
    let tokens = lex(&file).expect("lex");
    match parse(&tokens) {
        Ok(parsed) => analyze(&parsed).expect_err("expected a diagnostic"),
        Err(diagnostic) => diagnostic,
    }
}

fn interpret(source: &str) -> Result<(u8, String), Diagnostic> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "bn-destructure-{}-{}.bn",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, source).expect("write source");
    let graph = load(&path).map_err(|error| *error.diagnostic)?;
    let models =
        bn_frontend::semantic::analyze_modules(&graph).map_err(|error| *error.diagnostic)?;
    let module = lower_graph(&graph, &models)?;
    let host = HostEnv::fixed(vec!["destructure.bn".into()], 0, 0).with_default_providers();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let code = execute_with_host(&module, &mut input, &mut output, &host)?;
    let _ = fs::remove_file(&path);
    Ok((code, String::from_utf8(output).expect("UTF-8 output")))
}

fn execute(mut command: Command) -> ProcessOutput {
    run(&mut command, None, PROCESS_TIMEOUT).expect("run child process")
}

fn native_stdout(source: &str) -> Vec<u8> {
    let directory = TestDir::new("destructure").expect("create directory");
    let path = directory.join("program.bn");
    fs::write(&path, source).expect("write source");
    let artifact = compile_native(&path, &directory);
    let output = execute(Command::new(&artifact));
    assert!(
        output.status.success(),
        "native run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn assert_both(source: &str, expected: &str) {
    let (code, output) = interpret(source).expect("interpret");
    assert_eq!(code, 0, "{output}");
    assert_eq!(output, expected);
    assert_eq!(native_stdout(source), expected.as_bytes());
}

fn assert_both_fail(source: &str, code: &str) {
    let error = interpret(source).expect_err("interpreter rejects");
    assert_eq!(error.code, code);
    let directory = TestDir::new("destructure-fail").expect("create directory");
    let path = directory.join("program.bn");
    fs::write(&path, source).expect("write source");
    let mut compiler = bnc();
    compiler.arg(&path).arg("-o").arg(directory.join("out"));
    let compiled = execute(compiler);
    let mut checker = bni();
    checker.arg("check").arg(&path);
    let checked = execute(checker);
    let compiled_text = String::from_utf8_lossy(&compiled.stderr);
    let checked_text = String::from_utf8_lossy(&checked.stderr);
    assert!(!compiled.status.success(), "{compiled_text}");
    assert!(!checked.status.success(), "{checked_text}");
    assert!(compiled_text.contains(code), "{compiled_text}");
    assert!(checked_text.contains(code), "{checked_text}");
}

#[test]
fn fixed_vector_struct_and_literals_match_on_both_backends() {
    let source = r#"STRUCT Site
    port AS INTEGER = 80
    name AS STRING
END STRUCT

STRUCT Holder
    site AS Site
END STRUCT

STRUCT Wrap
    site AS Site = [7, "init"]
END STRUCT

FUNCTION Made() AS Site
    RETURN [1, "return"]
END FUNCTION

FUNCTION Take(site AS Site) AS VOID
    PRINT site.port, site.name
END FUNCTION

FUNCTION Start() AS VOID
    LET point AS INTEGER[2] = [3, 4]
    LET [x AS INTEGER, y AS INTEGER] = point
    PRINT x, y
    LET grid AS INTEGER[2][3] = [[1, 2, 3], [4, 5, 6]]
    LET [top, bottom] = grid
    PRINT top[0], bottom[2]
    LET site AS Site = [10, "web"]
    LET [name, port] = site
    PRINT name, port
    site = [12, "assigned"]
    PRINT site.port, site.name
    LET holder AS Holder = [[9, "field"]]
    PRINT holder.site.port, holder.site.name
    LET wrap AS Wrap
    PRINT wrap.site.port, wrap.site.name
    Take([8, "arg"])
    LET made AS Site = Made()
    PRINT made.port, made.name
END FUNCTION
"#;
    assert_both(
        source,
        "3 4\n1 6\nweb 10\n12 assigned\n9 field\n7 init\n8 arg\n1 return\n",
    );
}

#[test]
fn the_source_is_evaluated_once() {
    let source = r#"FUNCTION Pair() AS INTEGER[2]
    PRINT "once"
    RETURN [8, 9]
END FUNCTION

FUNCTION Start() AS VOID
    LET [left, right] = Pair()
    PRINT left, right
END FUNCTION
"#;
    assert_both(source, "once\n8 9\n");
}

#[test]
fn a_runtime_vector_length_must_match() {
    // `bnc` rejects `Index` of a local `T[n]` (the same diagnostic as
    // `values[0]`). The pointer region is the run-time length both backends
    // execute; the interpreter still checks the local form.
    let source = r#"FUNCTION Start() AS VOID
    LET n AS INTEGER = 2
    LET values AS INTEGER[n] = [4, 5]
    LET [first, second] = values
    PRINT first, second
END FUNCTION
"#;
    let (code, output) = interpret(source).expect("interpret local length");
    assert_eq!(code, 0);
    assert_eq!(output, "4 5\n");
    let mismatch = r#"FUNCTION Start() AS VOID
    LET n AS INTEGER = 3
    LET values AS INTEGER[n] = [1, 2, 3]
    LET [first, second] = values
    PRINT first, second
END FUNCTION
"#;
    let interpreted = interpret(mismatch).expect_err("bounds");
    assert_eq!(interpreted.code, "INDEX_OUT_OF_BOUNDS");

    let pointer_mismatch = r#"FUNCTION Start() AS VOID
    LET n AS INTEGER = 3
    LET region AS POINTER TO INTEGER[] = NEW INTEGER[n]
    LET [first, second] = region
    PRINT first, second
END FUNCTION
"#;
    let directory = TestDir::new("destructure-bounds").expect("create directory");
    let path = directory.join("program.bn");
    fs::write(&path, pointer_mismatch).expect("write source");
    let interpreted = interpret(pointer_mismatch).expect_err("pointer bounds");
    assert_eq!(interpreted.code, "INDEX_OUT_OF_BOUNDS");
    let artifact = compile_native(&path, &directory);
    let compiled = execute(Command::new(&artifact));
    assert!(!compiled.status.success());
    assert!(
        String::from_utf8_lossy(&compiled.stderr).contains("INDEX_OUT_OF_BOUNDS"),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
}

#[test]
fn a_pointer_region_destructures_by_position() {
    let source = r#"FUNCTION Start() AS VOID
    LET n AS INTEGER = 2
    LET region AS POINTER TO INTEGER[] = NEW INTEGER[n]
    region[0] = 7
    region[1] = 8
    LET [first, second] = region
    PRINT first, second
END FUNCTION
"#;
    assert_both(source, "7 8\n");
}

#[test]
fn invalid_destructuring_is_rejected_on_both_backends() {
    assert_eq!(
        diagnostic_of(&program(
            "LET point AS INTEGER[2] = [1, 2]\nLET [a] = point\n"
        ))
        .code,
        "TYPE_MISMATCH"
    );
    assert_eq!(
        diagnostic_of(&program(
            "LET point AS INTEGER[2] = [1, 2]\nLET x AS INTEGER = 0\nLET [x, y] = point\n"
        ))
        .code,
        "DUPLICATE_NAME"
    );
    assert_eq!(
        diagnostic_of(&program(
            "LET point AS INTEGER[2] = [1, 2]\nLET [x, x] = point\n"
        ))
        .code,
        "DUPLICATE_NAME"
    );
    assert_eq!(
        diagnostic_of(
            "STRUCT Site\n    port AS INTEGER = 80\n    name AS STRING\nEND STRUCT\nFUNCTION Start() AS VOID\nLET site AS Site = [10, \"web\"]\nLET [missing] = site\nEND FUNCTION\n"
        )
        .code,
        "NAME_NOT_FOUND"
    );
    assert_eq!(
        diagnostic_of(
            "STRUCT Site\n    port AS INTEGER = 80\n    name AS STRING\nEND STRUCT\nFUNCTION Start() AS VOID\nLET site AS Site = [10]\nEND FUNCTION\n"
        )
        .code,
        "TYPE_MISMATCH"
    );
    assert_eq!(
        diagnostic_of(
            "CLASS Box\n    PUBLIC value AS INTEGER = 0\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET box AS Box = NEW Box()\nLET [value] = box\nEND FUNCTION\n"
        )
        .code,
        "TYPE_MISMATCH"
    );
    assert_eq!(
        diagnostic_of(&program(
            "LET point AS INTEGER[2] = [1, 2]\n[x, y] = point\n"
        ))
        .code,
        "E0101"
    );
    assert_both_fail(
        &program("LET point AS INTEGER[3] = [1, 2, 3]\nLET [a, b] = point\n"),
        "TYPE_MISMATCH",
    );
}
