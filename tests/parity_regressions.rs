// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Regression tests for defects of section P of `ongoing/bucket-0.6.5.md`:
//! each runs one fixture on `bni` and on the native binary and checks the
//! exact output (or the support refusal) the known issue it names fixed.

mod support;

use std::{fs, path::Path, process::Command, time::Duration};

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
    for helper in [
        "call ptr @bn_rt_str_from_packed(",
        "call ptr @bn_rt_str_from_char(",
    ] {
        assert!(
            llvm.lines().any(|line| line.contains(helper)),
            "{helper} not emitted"
        );
    }
    for stack_buf in ["%strindexbuffer = alloca", "%charbuffer = alloca"] {
        assert!(
            !llvm.lines().any(|line| line.contains(stack_buf)),
            "unexpected stack buffer: {stack_buf}"
        );
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

/// `IS POINTER TO INTEGER` on a `POINTER TO VOID` that holds one INTEGER
/// (0.6.md, BDFL decision 2026-10-16).
#[test]
fn typed_pointer_test_on_an_opaque_pointer_reads_the_allocation() {
    let path = valid_fixture("pointer-typed-test-on-opaque.bn");
    let expected = "TRUE\n";
    let directory = TestDir::new("pointer-typed-test").expect("create directory");
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

/// Element and length of `IS POINTER TO T` on an opaque pointer, plus a null
/// alternative (0.6.md, BDFL decision 2026-10-16).
#[test]
fn pointer_is_reads_element_and_length() {
    let path = valid_fixture("pointer-is-allocation.bn");
    let expected = "TRUE\nTRUE\nFALSE\nTRUE\nTRUE\nTRUE\nFALSE\n";
    let directory = TestDir::new("pointer-is-allocation").expect("create directory");
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

/// `POINTER TO INTEGER[] OR NULL` holding `NEW INTEGER[3]` (BDFL decision,
/// 2026-10-16). `VOID` and the open length are true; a named length matches
/// the stored count; `NULL` fails every pointer test.
#[test]
fn nullable_integer_region_is_reads_length() {
    let path = valid_fixture("pointer-region-is.bn");
    let expected = "TRUE\nTRUE\nTRUE\nFALSE\nFALSE\nFALSE\nFALSE\nFALSE\nFALSE\nTRUE\n";
    let directory = TestDir::new("pointer-region-is").expect("create directory");
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

/// `IS POINTER TO Log.Logger` on `POINTER TO Log.Logger OR NULL` (K11).
/// The stored value is NULL. This does not read an allocation.
#[test]
fn qualified_null_pointer_alternative_is_not_the_pointer() {
    let path = valid_fixture("qualified-pointer-alternative-is.bn");
    let expected = "TRUE\nFALSE\n";
    let directory = TestDir::new("qualified-pointer-alternative").expect("create directory");
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

/// `IS POINTER TO Box` on `POINTER TO Box OR NULL` (K11).
/// The stored value is NULL; a live `POINTER TO Box` is not constructed here.
#[test]
fn null_named_pointer_alternative_is_not_the_pointer() {
    let path = valid_fixture("pointer-named-type.bn");
    let expected = "TRUE\nFALSE\n";
    let directory = TestDir::new("pointer-named-type").expect("create directory");
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

/// `IS Log.Logger` on `Log.Logger OR Error` (K64).
#[test]
fn imported_logger_alternative_is_the_logger() {
    let path = workspace_root().join("tests/grammar/valid/logger-alternative-is.bn");
    let expected = "TRUE\nFALSE\n";
    let directory = TestDir::new("logger-alternative-is").expect("create directory");
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
    let expected = "8 80 3 1\n0 4\n0\n0 13\n0 13\n10 20 30 77\n40 50\nb z c\n6000\n";
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

/// An `ASYNC` task runs on its own fresh copy of the `STATIC` fields (known
/// issue K44): native builds shared one copy across threads, so a task saw
/// the caller's values, its writes reached the caller, and concurrent tasks
/// lost updates. Every `STATIC` global is now thread-local, and each
/// trampoline resets them before its task, since a queue reuses threads.
#[test]
fn async_tasks_run_on_a_fresh_copy_of_the_static_fields() {
    let path = valid_fixture("dispatch-static-isolation.bn");
    let emitted = execute(bnc().args(["--emit", "llvm"]).arg(&path), None);
    assert_success(&emitted, "emit static isolation");
    let llvm = String::from_utf8_lossy(&emitted.stdout);
    let static_globals: Vec<&str> = llvm
        .lines()
        .filter(|line| {
            ["@bn_st_", "@bn_sv_", "@bn_svp_", "@bn_init_"]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        })
        .collect();
    assert!(!static_globals.is_empty(), "no STATIC global emitted");
    for line in static_globals {
        assert!(line.contains(" thread_local global "), "{line}");
    }
    let trampolines = llvm
        .split("\ndefine ")
        .filter(|function| function.contains("@bn_dispatch_trampoline_"))
        .collect::<Vec<_>>();
    assert!(!trampolines.is_empty(), "no dispatch trampoline emitted");
    for trampoline in trampolines {
        assert!(
            trampoline.contains("call void @bn_statics_reset()"),
            "{trampoline}"
        );
    }
    let task = "task 0 start 3 4 1 none\n";
    let expected = format!("{}main 10 main 300 4 500 set\n", task.repeat(5));
    let directory = TestDir::new("dispatch-static-isolation").expect("create directory");
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

/// The end of an `ASYNC` task is the end of its copy's program (known issue
/// K88): what its `STATIC` fields or a cycle still hold gets no
/// `DESTRUCTOR`, and the copy is discarded. Native builds kept those
/// objects for the life of the process, so each task leaked them and took
/// new object ids; now the next task reuses the reclaimed slots, and the
/// `BN_ARC_TRACE` lines match `bni`, which drops the copy's core.
#[test]
fn the_end_of_an_async_task_discards_its_copy() {
    let path = valid_fixture("dispatch-task-end-discards.bn");
    let directory = TestDir::new("dispatch-task-end-discards").expect("create directory");
    let artifact = compile_native(&path, &directory);
    let trace = |output: &ProcessOutput| -> Vec<String> {
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.starts_with("arc "))
            .map(str::to_owned)
            .collect()
    };
    let interpreted = execute(bni().env("BN_ARC_TRACE", "1").arg("run").arg(&path), None);
    let native = execute(Command::new(&artifact).env("BN_ARC_TRACE", "1"), None);
    let one_task = "destroyed temporary\ntask 1\n";
    let expected = format!("{}end\n", one_task.repeat(2));
    for (backend, output) in [("bni", &interpreted), ("native", &native)] {
        assert_success(output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
    let objects_of_one_task = "Node#0 Node#1 Node#0 Node#2 Node#3 Node#3 Node#2 Node#4 Node#4 \
                               Node#4 Node#3 Node#2 Node#0";
    let ids = |lines: &[String]| -> String {
        lines
            .iter()
            .filter_map(|line| line.split_whitespace().nth(2))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let native_trace = trace(&native);
    assert_eq!(trace(&interpreted), native_trace);
    assert_eq!(
        ids(&native_trace),
        format!("{objects_of_one_task} {objects_of_one_task}")
    );
}

/// Runs `path` on `bni` and natively, each in its own fresh directory (the
/// fixture writes files in the working directory), and checks the exact
/// standard output.
fn assert_same_output_in_fresh_directories(path: &Path, label: &str, expected: &str) {
    let directory = TestDir::new(label).expect("create directory");
    let artifact = compile_native(path, &directory);
    let interpreted_dir = TestDir::new(&format!("{label}-bni")).expect("create directory");
    let native_dir = TestDir::new(&format!("{label}-native")).expect("create directory");
    for (backend, output) in [
        (
            "bni",
            execute(
                bni()
                    .arg("run")
                    .arg(path)
                    .current_dir(interpreted_dir.path()),
                None,
            ),
        ),
        (
            "native",
            execute(Command::new(&artifact).current_dir(native_dir.path()), None),
        ),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
    }
}

/// A listener an `ASYNC` task keeps in a `STATIC` closes with the task's
/// copy (known issue K90): the program can bind its port afterwards. Native
/// builds kept it open for the life of the process.
#[test]
fn the_end_of_an_async_task_closes_its_sockets() {
    let path = valid_fixture("dispatch-task-end-closes-socket.bn");
    assert_same_output_in_fresh_directories(&path, "task-end-socket", "port released\n");
}

/// A connection an `ASYNC` task keeps in a `STATIC` closes with the task's
/// copy (known issue K90): its open transaction rolls back and the database
/// is free. Both backends kept it open (`BUSY`), the connection table being
/// process-wide.
#[test]
fn the_end_of_an_async_task_closes_its_connections() {
    let path = valid_fixture("dispatch-task-end-closes-sqlite.bn");
    assert_same_output_in_fresh_directories(&path, "task-end-sqlite", "database free\nrows: 1\n");
}

/// 200 tasks each keep an open file in a `STATIC` under a limit of 64
/// descriptors (known issue K90): every file closes with its task's copy,
/// so no open fails. Native builds failed from the 32nd task on.
#[cfg(unix)]
#[test]
fn the_end_of_an_async_task_closes_its_files() {
    let path = valid_fixture("dispatch-task-end-closes-files.bn");
    let directory = TestDir::new("task-end-files").expect("create directory");
    let artifact = compile_native(&path, &directory);
    let limited = |program: &str| {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("ulimit -n 64 && exec {program}"))
            .current_dir(directory.path());
        execute(&mut command, None)
    };
    let interpreter = format!(
        "'{}' run '{}'",
        bni().get_program().to_string_lossy(),
        path.display()
    );
    for (backend, output) in [
        ("bni", limited(&interpreter)),
        ("native", limited(&format!("'{}'", artifact.display()))),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "ok 200 failed 0\n",
            "{backend}"
        );
    }
}

/// A dynamic vector dimension (`LET values AS INTEGER[n]`) lowers cleanly
/// on both backends and zeros its memory (known issue K12 / T.K12).
#[test]
fn dynamic_vector_dimension_allocates_and_zeros_memory() {
    let directory = TestDir::new("dynamic-vector-dimension").expect("create directory");
    let path = directory.join("dynamic_vec.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS VOID\n\
            LET n AS INTEGER = 4\n\
            LET values AS INTEGER[n]\n\
            PRINT \"len:\", LEN(values)\n\
            PRINT \"default:\", values[0], values[1], values[2], values[3]\n\
            values[0] = 10\n\
            values[3] = 40\n\
            PRINT \"assigned:\", values[0], values[1], values[2], values[3]\n\
        END FUNCTION\n",
    )
    .expect("write dynamic vec test");
    let expected = "len: 4\ndefault: 0 0 0 0\nassigned: 10 0 0 40\n";
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

/// A vector literal inside a loop pre-allocates its storage in the entry block
/// and does not grow the call stack across iterations (known issue K62).
#[test]
fn vector_literal_in_loop_reuses_entry_slot_without_stack_growth() {
    let directory = TestDir::new("vector-loop-stack").expect("create directory");
    let path = directory.join("vec_loop.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS VOID\n\
            LET sum AS INTEGER = 0\n\
            FOR i AS INTEGER = 1 TO 20000\n\
                LET v AS INTEGER[3] = [1, 2, 3]\n\
                sum = sum + v[0] + v[1] + v[2]\n\
            END FOR\n\
            PRINT \"sum:\", sum\n\
        END FUNCTION\n",
    )
    .expect("write vec loop test");
    let emitted = execute(bnc().args(["--emit", "llvm"]).arg(&path), None);
    assert_success(&emitted, "emit vector loop llvm");
    let llvm = String::from_utf8_lossy(&emitted.stdout);
    let mut in_b0 = false;
    let mut in_loop = false;
    let mut found_b0_alloca = false;
    let mut loop_has_alloca = false;
    for line in llvm.lines() {
        let trimmed = line.trim();
        if trimmed == "b0:" {
            in_b0 = true;
            in_loop = false;
        } else if trimmed.starts_with('b') && trimmed.ends_with(':') {
            in_b0 = false;
            in_loop = true;
        }
        if in_b0 && trimmed.contains("alloca [3 x i32]") {
            found_b0_alloca = true;
        }
        if in_loop && trimmed.contains("alloca [3 x i32]") {
            loop_has_alloca = true;
        }
    }
    assert!(
        found_b0_alloca,
        "vector data alloca must be in entry block b0"
    );
    assert!(
        !loop_has_alloca,
        "no vector data alloca should exist inside loop blocks"
    );

    let expected = "sum: 120000\n";
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

/// Narrowed resources store cleanly into `T OR NULL` and static `Log.Logger OR Error` (K91).
#[test]
fn narrowed_resources_store_into_nullable_and_error_alternatives() {
    let directory = TestDir::new("parity-k91-narrowed-resources").expect("create directory");
    let path = directory.join("k91.bn");
    fs::write(
        &path,
        r#"IMPORT HOST.FileSystem AS FS
IMPORT BNLog AS Log

CLASS State
    PUBLIC STATIC file_kept AS FS.File OR NULL = NULL
    PUBLIC STATIC log_kept AS Log.Logger OR Error = Log.Logger.New("app", "JSON", "CONSOLE")
END CLASS

FUNCTION Start() AS VOID
    LET local_file AS FS.File OR NULL = NULL
    LET res AS FS.File OR Error = FS.Open("nonexistent_test_k91.txt", FS.READ)
    IF NOT (res IS Error) THEN
        local_file = res
        State.file_kept = res
    END IF

    LET log_res AS Log.Logger OR Error = Log.Logger.New("app2", "JSON", "CONSOLE")
    IF log_res IS Log.Logger THEN
        LET logger AS Log.Logger = log_res
        State.log_kept = logger
    END IF
    PRINT "k91 ok"
END FUNCTION
"#,
    )
    .expect("write k91 source");

    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "k91 ok",
            "{backend}"
        );
    }
}

/// ARC trace region labels match on both backends (K16 / T.K-trace).
#[test]
fn arc_trace_region_labels_match_on_both_backends() {
    let directory = TestDir::new("parity-k16-arc-trace-region").expect("create directory");
    let path = directory.join("k16.bn");
    fs::write(
        &path,
        r#"FUNCTION Start() AS VOID
    LET r AS POINTER TO FLOAT64[] = NEW FLOAT64[3]
    PRINT LEN(r)
END FUNCTION
"#,
    )
    .expect("write k16 source");

    let artifact = compile_native(&path, &directory);
    let interpreted = execute(bni().env("BN_ARC_TRACE", "1").arg("run").arg(&path), None);
    let native = execute(Command::new(&artifact).env("BN_ARC_TRACE", "1"), None);
    assert_success(&interpreted, "bni");
    assert_success(&native, "native");
    let interp_trace: Vec<_> = String::from_utf8_lossy(&interpreted.stderr)
        .lines()
        .filter(|l| l.starts_with("arc "))
        .map(str::to_owned)
        .collect();
    let native_trace: Vec<_> = String::from_utf8_lossy(&native.stderr)
        .lines()
        .filter(|l| l.starts_with("arc "))
        .map(str::to_owned)
        .collect();
    assert!(!interp_trace.is_empty(), "must produce ARC trace");
    assert_eq!(interp_trace, native_trace, "region trace lines must match");
    assert!(
        interp_trace[0].contains("Float64#"),
        "trace must include element type prefix: {}",
        interp_trace[0]
    );
}

/// Standard-library handles are released at end of binding (K63).
#[test]
fn standard_library_handles_are_released_at_end_of_binding() {
    let directory = TestDir::new("parity-k63-standard-handles").expect("create directory");
    let path = directory.join("k63.bn");
    fs::write(
        &path,
        r#"IMPORT BNJson AS Json

FUNCTION MakeAndRead() AS STRING
    LET doc AS Json.Json = Json.Json.Object()
    LET w AS VOID OR Error = Json.Json.SetString(doc, "name", "pardal")
    IF w IS Error THEN
        RETURN "set-failed"
    END IF
    LET val AS STRING OR Error = Json.Json.GetString(doc, "name")
    IF val IS STRING THEN
        RETURN val
    END IF
    RETURN "FAIL"
END FUNCTION

FUNCTION Start() AS VOID
    LET i AS INTEGER = 0
    WHILE i < 5
        PRINT MakeAndRead()
        i = i + 1
    END WHILE
END FUNCTION
"#,
    )
    .expect("write k63 source");

    let artifact = compile_native(&path, &directory);
    for (backend, output) in [
        ("bni", interpret(&path, None)),
        ("native", execute_artifact(&artifact, &[], None)),
    ] {
        assert_success(&output, backend);
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "pardal\npardal\npardal\npardal\npardal\n",
            "{backend}"
        );
    }
}
