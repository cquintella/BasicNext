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

fn assert_native_parity(path: &Path, input: Option<&[u8]>) {
    let directory = TestDir::new("native-parity").expect("create parity directory");
    let artifact = compile_native(path, &directory);
    let interpreted = interpret(path, input);
    let compiled = execute_artifact(&artifact, &[], input);
    assert_eq!(
        compiled.status.code(),
        interpreted.status.code(),
        "{}",
        path.display()
    );
    assert_eq!(compiled.stdout, interpreted.stdout, "{}", path.display());
}

#[test]
fn csv_audit_cases_match_native_and_interpreter() {
    let path = valid_fixture("build-csv-audit.bn");
    let directory = TestDir::new("csv-audit").expect("create CSV directory");
    let artifact = compile_native(&path, &directory);
    let csv = directory.join("data.csv");
    let cases: [(&str, &str, &[u8]); 7] = [
        ("\"é,quoted\",\n", "no-header", "1 2\né,quoted\n".as_bytes()),
        ("", "no-header", b"0 0\n"),
        ("Column1,b\n", "header", b"0 2\n"),
        ("a,b\n1\n", "header", b"csv-error\n"),
        ("a,b\n1,2,3\n", "header", b"csv-error\n"),
        ("a,a\n1,2\n", "header", b"csv-error\n"),
        ("a\n\"unfinished", "header", b"csv-error\n"),
    ];
    for (content, mode, expected) in cases {
        fs::write(&csv, content).expect("write CSV fixture");
        let interpreted = execute(
            bni().args(["run"]).arg(&path).arg("--").arg(&csv).arg(mode),
            None,
        );
        let compiled = execute_artifact(
            &artifact,
            &[csv.to_str().expect("UTF-8 CSV path"), mode],
            None,
        );
        assert_success(&interpreted, content);
        assert_success(&compiled, content);
        assert_eq!(interpreted.stdout, expected, "{content:?}");
        assert_eq!(compiled.stdout, expected, "{content:?}");
    }
}

#[test]
fn slice_rejects_huge_counts_without_materializing_indices() {
    let path = valid_fixture("build-slice-audit.bn");
    let directory = TestDir::new("slice-audit").expect("create slice directory");
    let artifact = compile_native(&path, &directory);
    let interpreted = interpret(&path, None);
    let compiled = execute_artifact(&artifact, &[], None);
    for output in [&interpreted, &compiled] {
        assert_success(output, "slice audit");
        assert_eq!(output.stdout, b"TRUE\nTRUE\n20\n");
    }
}

#[test]
fn supported_constant_programs_match_interpreter() {
    for fixture in [
        "examples/hello.bn",
        "print-integer.bn",
        "print-const.bn",
        "print-expression.bn",
        "print-float.bn",
        "print-string.bn",
        "print-comparison.bn",
        "print-variable.bn",
        "print-args-length.bn",
        "print-if-constant.bn",
        "print-while-false.bn",
        "build-print-same-value.bn",
        "build-euclidean-div.bn",
        "build-euclidean-rem.bn",
        "build-euclidean-runtime.bn",
        "build-power-shift.bn",
        "build-power-shift-runtime.bn",
        "build-widths.bn",
        "build-clock.bn",
        "cls-and-beep.bn",
        "print-call.bn",
        "print-call-nested.bn",
        "print-predicate-call.bn",
        "print-string-call.bn",
        "build-integer-error-compare.bn",
        "increment-statement.bn",
        "increment-expression.bn",
    ] {
        let path = if fixture.starts_with("examples/") {
            workspace_root().join(fixture)
        } else {
            valid_fixture(fixture)
        };
        assert_native_parity(&path, None);
    }
}

#[test]
fn phi_predecessors_are_function_local() {
    let path = valid_fixture("build-phi-function-isolation.bn");
    let directory = TestDir::new("phi-isolation").expect("create phi directory");
    let artifact = compile_native(&path, &directory);
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret phi fixture");
    assert_eq!(
        interpreted.stdout,
        b"TRUE\nFALSE\nTRUE\nFALSE\nTRUE\nFALSE\n"
    );
    let compiled = execute_artifact(&artifact, &[], None);
    assert_success(&compiled, "compile phi fixture");
    assert_eq!(compiled.stdout, interpreted.stdout);
}

#[test]
fn distance_examples_cover_input_and_eof() {
    let cases: [(&[u8], i32, &[u8]); 6] = [
        (b"kitten\nsitting\n", 0, b"3\n"),
        ("café\ncafe\n".as_bytes(), 0, b"1\n"),
        (b"EOF\nEOF\n", 0, b"0\n"),
        (b"\n\n", 0, b"0\n"),
        (b"", 1, b"Expected word 1.\n"),
        (b"kitten\n", 1, b"Expected word 2.\n"),
    ];
    for fixture in ["edit_distance.bn", "levenshtein.bn"] {
        let path = workspace_root().join("examples").join(fixture);
        let directory = TestDir::new("distance").expect("create distance directory");
        let artifact = compile_native(&path, &directory);
        for (input, exit_code, suffix) in cases {
            let interpreted = interpret(&path, Some(input));
            let compiled = execute_artifact(&artifact, &[], Some(input));
            assert_eq!(interpreted.status.code(), Some(exit_code), "{fixture}");
            assert!(interpreted.stdout.ends_with(suffix), "{fixture}");
            assert_eq!(compiled.status.code(), Some(exit_code), "{fixture}");
            assert_eq!(compiled.stdout, interpreted.stdout, "{fixture}");
        }
    }
}

#[test]
fn input_eof_sentinel_is_distinct_from_string_contents() {
    let path = valid_fixture("build-input-type-tests.bn");
    let directory = TestDir::new("input-types").expect("create input directory");
    let artifact = compile_native(&path, &directory);
    for (input, expected_exit, expected_stdout) in [
        (b"".as_slice(), 1, b"EOF\n".as_slice()),
        (b"EOF\n".as_slice(), 0, b"STRING\n".as_slice()),
        ("café\n".as_bytes(), 0, b"STRING\n".as_slice()),
    ] {
        let interpreted = interpret(&path, Some(input));
        let compiled = execute_artifact(&artifact, &[], Some(input));
        assert_eq!(interpreted.status.code(), Some(expected_exit));
        assert_eq!(interpreted.stdout, expected_stdout);
        assert_eq!(compiled.status.code(), interpreted.status.code());
        assert_eq!(compiled.stdout, interpreted.stdout);
    }
}

#[test]
fn nullable_integer_type_tests_inspect_the_runtime_tag() {
    let path = valid_fixture("build-nullable-integer-type-tests.bn");
    let directory = TestDir::new("nullable-integer").expect("create nullable directory");
    let artifact = compile_native(&path, &directory);
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret nullable integer fixture");
    assert_eq!(interpreted.stdout, b"TRUE\nFALSE\nFALSE\nTRUE\n");
    let compiled = execute_artifact(&artifact, &[], None);
    assert_success(&compiled, "compile nullable integer fixture");
    assert_eq!(compiled.stdout, interpreted.stdout);
}

#[test]
fn indexed_struct_and_static_fields_preserve_language_semantics() {
    let path = valid_fixture("indexed-member-assignment.bn");
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret indexed member fixture");
    assert_eq!(interpreted.stdout, b"7\n9\n");
    let emitted = execute(bnc().arg(&path), None);
    assert!(!emitted.status.success());
    let diagnostics = String::from_utf8_lossy(&emitted.stderr);
    assert!(diagnostics.contains("error[TARGET_UNSUPPORTED_OP]"));
    assert!(!diagnostics.contains("INVALID_IR"));
}

#[test]
fn inherited_fields_have_one_complete_native_object_layout() {
    let path = valid_fixture("build-inherited-field-layout.bn");
    let emitted = execute(bnc().arg(&path), None);
    assert_success(&emitted, "emit inherited layout");
    let allocation = b"call ptr @calloc(i64 1, i64 32)";
    assert!(
        emitted
            .stdout
            .windows(allocation.len())
            .any(|bytes| bytes == allocation)
    );
    let directory = TestDir::new("inherited-layout").expect("create layout directory");
    let artifact = compile_native(&path, &directory);
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret inherited layout");
    assert_eq!(interpreted.stdout, b"17\n23\n31\n");
    let compiled = execute_artifact(&artifact, &[], None);
    assert_success(&compiled, "run inherited layout");
    assert_eq!(compiled.stdout, interpreted.stdout);
}

#[test]
fn struct_fields_have_distinct_layout_and_bounded_native_lifetime() {
    let path = valid_fixture("build-struct-layout-lifetime.bn");
    let emitted = execute(bnc().arg(&path), None);
    assert_success(&emitted, "emit struct layout");
    for fragment in [
        b"call ptr @calloc(i64 1, i64 32)".as_slice(),
        b"getelementptr i8, ptr %fieldobj".as_slice(),
        b"i32 16".as_slice(),
        b"i32 20".as_slice(),
        b"call void @free(ptr %structfree".as_slice(),
    ] {
        assert!(
            emitted
                .stdout
                .windows(fragment.len())
                .any(|bytes| bytes == fragment)
        );
    }
    let directory = TestDir::new("struct-layout").expect("create struct directory");
    let artifact = compile_native(&path, &directory);
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret struct layout");
    assert_eq!(interpreted.stdout, b"17 23\n");
    let compiled = execute_artifact(&artifact, &[], None);
    assert_success(&compiled, "run struct layout");
    assert_eq!(compiled.stdout, interpreted.stdout);
}

#[test]
fn struct_return_without_ownership_transfer_fails_closed() {
    let path = valid_fixture("struct-return-lifetime-deferred.bn");
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret deferred struct return");
    assert_eq!(interpreted.stdout, b"ok\n");
    let emitted = execute(bnc().arg(&path), None);
    assert!(!emitted.status.success());
    let diagnostics = String::from_utf8_lossy(&emitted.stderr);
    assert!(diagnostics.contains("error[TARGET_UNSUPPORTED_OP]"));
    assert!(diagnostics.contains("STRUCT default allocation requires an acyclic Start lifetime"));
    assert!(!diagnostics.contains("INVALID_IR"));
}

#[test]
fn multidimensional_vector_matches_interpreter() {
    let path = valid_fixture("multidimensional-vectors.bn");
    let directory = TestDir::new("multidimensional-vector").expect("create vector directory");
    let artifact = compile_native(&path, &directory);
    let interpreted = interpret(&path, None);
    assert_success(&interpreted, "interpret multidimensional vector");
    assert_eq!(interpreted.stdout, b"9\n");
    let compiled = execute_artifact(&artifact, &[], None);
    assert_eq!(compiled.status.code(), interpreted.status.code());
    assert_eq!(compiled.stdout, interpreted.stdout);
}

#[test]
fn input_program_matches_interpreter() {
    assert_native_parity(&valid_fixture("build-input.bn"), Some(b"hello\r\n"));
}

#[test]
fn seeded_random_program_matches_interpreter() {
    assert_native_parity(&valid_fixture("host-random.bn"), None);
}

#[test]
fn seeded_random_sequence_matches_interpreter() {
    assert_native_parity(&valid_fixture("host-random-twice.bn"), None);
}

#[test]
fn seeded_random_branch_matches_interpreter() {
    assert_native_parity(&valid_fixture("build-random-branch.bn"), None);
}
