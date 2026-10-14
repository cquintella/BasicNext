// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Regression tests for defects of section P of `ongoing/bucket-0.6.5.md`:
//! each runs one fixture on `bni` and on the native binary and checks the
//! exact output (or the support refusal) the known issue it names fixed.

mod support;

use std::{path::Path, process::Command, time::Duration};

use support::{ProcessOutput, TestDir, bnc, bni, compile_native, run, workspace_root};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

fn execute(command: &mut Command, input: Option<&[u8]>) -> ProcessOutput {
    run(command, input, PROCESS_TIMEOUT).expect("run child process")
}

fn interpret(path: &Path, input: Option<&[u8]>) -> ProcessOutput {
    execute(bni().arg("run").arg(path), input)
}

fn execute_artifact(artifact: &Path, arguments: &[&str], input: Option<&[u8]>) -> ProcessOutput {
    execute(Command::new(artifact).args(arguments), input)
}

fn assert_success(output: &ProcessOutput, context: &str) {
    assert!(
        output.status.success(),
        "{context}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn valid_fixture(name: &str) -> std::path::PathBuf {
    workspace_root().join("tests/grammar/valid").join(name)
}

/// A character from `s[i]` or `CHAR` outlives its frame (known issue K49):
/// native builds returned a pointer into a dead stack buffer, and a loop
/// grew the stack until the process crashed. The buffer is now a heap
/// allocation, so no `alloca` remains on those paths.
#[test]
fn string_index_and_char_results_outlive_their_frame() {
    let path = valid_fixture("string-index-escapes.bn");
    let emitted = execute(bnc().args(["--emit", "llvm"]).arg(&path), None);
    assert_success(&emitted, "emit string index");
    let llvm = String::from_utf8_lossy(&emitted.stdout);
    for buffer in ["%strindexbuffer", "%charbuffer"] {
        let lines: Vec<&str> = llvm
            .lines()
            .filter(|line| line.trim_start().starts_with(buffer) && line.contains(" = "))
            .collect();
        assert!(!lines.is_empty(), "{buffer} not emitted");
        for line in lines {
            assert!(line.contains("call ptr @malloc(i64 8)"), "{line}");
        }
    }
    let expected = "b ç\nx y z\n100\n";
    let directory = TestDir::new("string-index-escapes").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// The elements of a `STRING` vector without an initializer are `""` on both
/// backends, in a local and in a class field (known issue K50: native builds
/// stored a null pointer).
#[test]
fn string_vector_elements_default_to_the_empty_string() {
    let path = valid_fixture("string-vector-default.bn");
    let expected = "[] 0 TRUE\nx\n[] 0\n";
    let directory = TestDir::new("string-vector-default").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// `IS POINTER TO VOID` holds for every non-NULL pointer value, typed or
/// opaque, and guards the conversion back to a typed pointer (known issue
/// K51: native builds folded the test to `FALSE`, so the branch never ran).
#[test]
fn is_pointer_to_void_holds_for_a_non_null_pointer() {
    let path = valid_fixture("pointer-void.bn");
    let expected = "42\n";
    let directory = TestDir::new("pointer-void").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// `IS POINTER TO T` with `T` not `VOID` is refused natively with a support
/// diagnostic at the test's position until decision K55, and is never folded
/// to a constant (known issue K51). `bni` answers `TRUE` for an `INTEGER`
/// allocation, which every reading of K55 keeps.
#[test]
fn typed_pointer_test_on_an_opaque_pointer_is_a_support_refusal() {
    let path = valid_fixture("pointer-typed-test-on-opaque.bn");
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret typed pointer test");
    assert_eq!(interpreted.stdout, b"TRUE\n");
    let emitted = execute(bnc().arg(&path), None);
    assert!(!emitted.status.success());
    let diagnostics = String::from_utf8_lossy(&emitted.stderr);
    assert!(
        diagnostics.contains("error[TARGET_UNSUPPORTED_OP]"),
        "{diagnostics}"
    );
    assert!(diagnostics.contains(":15:11:"), "{diagnostics}");
    assert!(!diagnostics.contains("INVALID_IR"), "{diagnostics}");
}

/// Functions of an imported module copy, reassign, return and test objects
/// of the module's own class with the ARC behaviour of a one-file program
/// (known issues K57: both backends stopped with `INVALID_IR` on the copy
/// and the reassignment; K52: a `NEW` returned by a method of the imported
/// class was never registered natively, so the caller released another
/// object).
#[test]
fn imported_module_owns_objects_of_its_own_class() {
    let path = workspace_root().join("tests/modules/own-class-ownership/main.bn");
    let expected = "a c\nfree a\nfree c\nx2 x\nis box TRUE\nbox y\nfree y\n\
                    is box FALSE\nnone\nfree p\nfree q\n2\nend\nfree x2\nfree x\n";
    let directory = TestDir::new("own-class-ownership").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// The objects of a vector of more than one dimension are released once
/// each, row by row (known issue K30: native builds walked the rows as
/// objects and stopped with `INVALID_IR`).
#[test]
fn objects_in_vector_rows_are_released_once() {
    let path = valid_fixture("vector-rows-release.bn");
    let expected = "b c\nfree a\nfree b\nfree c\nfree d\nend\n";
    let directory = TestDir::new("vector-rows-release").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// `BNMath` range constants read through the `Limits` class fold to the
/// same values as the module members (math.md: one storage location; K59).
#[test]
fn bnmath_limits_class_reads_the_range_constants() {
    let directory = TestDir::new("bnmath-limits").expect("create directory");
    let path = directory.join("limits.bn");
    std::fs::write(
        &path,
        "IMPORT BNMath AS Math\n\nFUNCTION Start() AS VOID\n    \
         PRINT Math.Limits.MAX_FLOAT = Math.MAX_FLOAT, Math.Limits.MAX_INTEGER, \
         Math.Limits.MIN_POSITIVE_FLOAT32 = Math.MIN_POSITIVE_FLOAT32, Math.Limits.MAX_BYTE\n\
         END FUNCTION\n",
    )
    .expect("write program");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "TRUE 2147483647 TRUE 255\n",
            "{backend}"
        );
    }
}

/// A STRUCT made outside `Start` builds natively (known issue K13): returned
/// from a function, made on every turn of a loop, copied with value
/// semantics, and holding an object that is released exactly once.
#[test]
fn struct_values_outside_start_match_on_both_backends() {
    let path = valid_fixture("struct-values-outside-start.bn");
    let expected = "7 9 made\n50005000\n1\n1\nend\nfree k\n";
    let directory = TestDir::new("struct-values").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// Recursion that never ends stops with `STACK_OVERFLOW` naming the call, on
/// both backends, after the output written before it; a tail call takes
/// stack too, so `bnc` does not loop forever (0.6.md, "Recursion depth";
/// known issue K53).
#[test]
fn endless_recursion_stops_with_stack_overflow_on_both_backends() {
    for (path, callee) in [
        (
            workspace_root().join("tests/runtime-traps/stack-overflow.bn"),
            "Depth",
        ),
        (valid_fixture("build-recursive.bn"), "Loop"),
    ] {
        let directory = TestDir::new("stack-overflow").expect("create directory");
        let artifact = compile_native(&path, &directory);
        for (backend, output) in [
            ("bni", interpret(&path, None)),
            ("native", execute_artifact(&artifact, &[], None)),
        ] {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(1), "{backend}: {stderr}");
            assert!(
                stderr.contains(&format!(
                    "error[STACK_OVERFLOW]: Stack overflow: Calling {callee} would exhaust the call stack."
                )),
                "{backend}: {stderr}"
            );
            if callee == "Depth" {
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout),
                    "before\n",
                    "{backend}"
                );
            }
        }
    }
}

/// Fixed vectors of two and three dimensions are values natively, as under
/// `bni`: a copy, an argument, a returned vector, a default and a `STATIC`
/// each own their rows (0.6.md, value semantics; known issue K21).
#[test]
fn nested_vectors_are_values_on_both_backends() {
    let path = valid_fixture("nested-vector-values.bn");
    let expected = "8 80 3 1\n0 4\n0\n0 13\n0 13\nb z c\n6000\n";
    let directory = TestDir::new("nested-vectors").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// An index of any integer type names the same element on both backends
/// (known issue K71: native builds narrowed a wide index to 32 bits before
/// the bounds check). The out-of-range cases are traps in
/// `tests/runtime-traps/` (`wide-index-*`, `uint32-index`, `host-args-*`).
#[test]
fn indices_of_every_integer_width_match_on_both_backends() {
    let path = valid_fixture("index-widths.bn");
    let expected = "30 20 20 30 10\n33\n5\n7\nl h é\nTRUE\n";
    let directory = TestDir::new("index-widths").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// `HOST.Args` read from functions outside `Start` works on both backends
/// (known issue K79: native builds failed with undefined value `%argc` / `%argv`
/// in any function other than `Start`).
#[test]
fn host_args_outside_start_match_on_both_backends() {
    let path = valid_fixture("host-args-outside-start.bn");
    let expected = "TRUE\nTRUE\n";
    let directory = TestDir::new("host-args-outside-start").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// Global temporal builtins `Date.Parse`, `Time.Parse`, `Timestamp.Parse`,
/// and `Timestamp.Format` match on both backends (known issue K80).
#[test]
fn temporal_builtins_match_on_both_backends() {
    let path = valid_fixture("temporal-builtins.bn");
    let expected = "2026-01-02\n03:04:05.000\n1767323045000\n2026-01-02T03:04:05.000Z\n2026-01-02T03:04:05.000Z\n";
    let directory = TestDir::new("temporal-builtins").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// SQLite 64-bit rowid/changes (K73) and empty blob/text with NULL (K78) match on both backends.
#[test]
fn sqlite_types_and_empty_blob_match_on_both_backends() {
    let path = valid_fixture("sqlite-types-and-empty-blob.bn");
    let expected = "CHANGES: 1\nLAST_ID: 3000000000\nROWS: 1\nBLOB_LEN: 0\nTXT_LEN: 0\nOPT_IS_NA: TRUE\nSQLITE_TYPES_OK\n";
    let directory = TestDir::new("sqlite-types-and-empty-blob").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// Returning a struct by value containing ARC references and vectors outlives its frame (K10, K63).
#[test]
fn struct_return_arc_matches_on_both_backends() {
    let path = valid_fixture("struct-return-arc.bn");
    let expected = "golden\n42\n84\n";
    let directory = TestDir::new("struct-return-arc").expect("create directory");
    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}
