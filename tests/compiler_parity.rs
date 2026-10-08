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
    let emitted = execute(bnc().args(["--emit", "llvm"]).arg(&path), None);
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
    let emitted = execute(bnc().args(["--emit", "llvm"]).arg(&path), None);
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

/// `examples/dispatch_net_echo.bn`: two dispatch workers talk to one
/// listener at once. Natively a blocking read must not hold the socket-table
/// lock (the other client's write and the server's read would stall until
/// the 5 s timeouts).
#[test]
fn dispatch_net_echo_matches_native_and_interpreter() {
    let path = workspace_root().join("examples/dispatch_net_echo.bn");
    let output = interpret(&path, None);
    assert_success(&output, "bni run");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "client 1 got: hello from client 1\nclient 2 got: hello from client 2\n\
         bytes echoed: 38\n"
    );
    assert_native_parity(&path, None);
}

/// A `NEW` region stored in a field is owned by the field: the constructor
/// cancels the statement's release instead of freeing the region it just
/// stored (a use-after-free that Windows reported as heap corruption in
/// `examples/linear_collections.bn`). Both backends print the same values.
#[test]
fn region_fields_are_strong_bindings() {
    // Parity is the check: with the bug, the native build printed
    // `alias 6` and `after release 3` on macOS (verified 2026-09-28).
    let path = valid_fixture("arc-region-field.bn");
    let output = interpret(&path, None);
    assert_success(&output, "bni run");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "sum 33\ngrown 10 alias 33\nafter release 12\n"
    );
    assert_native_parity(&path, None);
}

/// Bucket 0.6.2b R6: a native runtime trap prints the diagnostic `bni`
/// prints (title, excerpt, cause, help; `bnc` renders it from the shared
/// catalog at compile time) and exits with the same status. One program per
/// trap kind lives in `tests/runtime-traps/`.
#[test]
fn native_runtime_traps_print_the_interpreter_diagnostic() {
    let directory = workspace_root().join("tests/runtime-traps");
    let mut paths = fs::read_dir(&directory)
        .expect("runtime trap fixtures")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "bn"))
        .collect::<Vec<_>>();
    paths.sort();
    assert!(
        paths.len() >= 19,
        "fixtures missing from {}",
        directory.display()
    );
    let build = TestDir::new("runtime-traps").expect("create trap directory");
    // Every program runs; the failures are reported together, so one broken
    // trap kind does not hide the others.
    let mut failures = Vec::new();
    for path in paths {
        let artifact = compile_native(&path, &build);
        let interpreted = interpret(&path, None);
        let compiled = execute_artifact(&artifact, &[], None);
        // `bni run` also prints frontend warnings; `bnc` printed them when
        // building, so compare from the runtime error on.
        let interpreted_error = String::from_utf8_lossy(&interpreted.stderr);
        let runtime_error = interpreted_error
            .find("error[")
            .map_or("", |start| &interpreted_error[start..]);
        let compiled_error = String::from_utf8_lossy(&compiled.stderr);
        let checks = [
            (interpreted.status.code() == Some(1), "bni status is not 1"),
            (compiled.status.code() == Some(1), "native status is not 1"),
            (compiled.stdout == interpreted.stdout, "stdout differs"),
            (compiled_error == runtime_error, "stderr differs"),
        ];
        for (ok, what) in checks {
            if !ok {
                failures.push(format!(
                    "{}: {what} (bni {:?}, native {:?})\n  bni: {runtime_error}\n  native: {compiled_error}",
                    path.display(),
                    interpreted.status.code(),
                    compiled.status.code(),
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `BN_FS_POLICY=deny` narrows both backends the same way: the capability
/// stays bound and every operation returns `Error(FS.POLICY_DENIED)`. The
/// interpreter used to fail before Start as if the host had no filesystem,
/// and the native runtime printed an extra line and returned Code 1.
#[test]
fn filesystem_policy_deny_returns_policy_denied_on_both_backends() {
    let path = workspace_root().join("tests/host/fs_policy_deny.bn");
    let build = TestDir::new("fs-policy-deny").expect("create directory");
    let artifact = compile_native(&path, &build);
    let interpreted = execute(
        bni().arg("run").arg(&path).env("BN_FS_POLICY", "deny"),
        None,
    );
    let compiled = execute(Command::new(&artifact).env("BN_FS_POLICY", "deny"), None);
    let expected = "TRUE Error 9 in HOST.FileSystem.Exists: cannot check whether \"policy-deny.txt\" exists (cause: the execution policy denies file access)\n\
                    TRUE HOST.FileSystem.Open\n";
    for (backend, output) in [("bni", &interpreted), ("native", &compiled)] {
        assert_eq!(output.status.code(), Some(0), "{backend}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
        assert!(
            output.stderr.is_empty(),
            "{backend}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// `BN_ENV_POLICY=deny` narrows both backends the same way: the capability
/// stays bound and every operation returns `Error(Env.POLICY_DENIED)`.
#[test]
fn env_policy_deny_returns_policy_denied_on_both_backends() {
    let path = workspace_root().join("tests/host/env_policy_deny.bn");
    let build = TestDir::new("env-policy-deny").expect("create directory");
    let artifact = compile_native(&path, &build);
    let interpreted = execute(
        bni().arg("run").arg(&path).env("BN_ENV_POLICY", "deny"),
        None,
    );
    let compiled = execute(Command::new(&artifact).env("BN_ENV_POLICY", "deny"), None);
    let expected = "TRUE HOST.Env.Get\nTRUE HOST.Env.Has\n";
    for (backend, output) in [("bni", &interpreted), ("native", &compiled)] {
        assert_eq!(output.status.code(), Some(0), "{backend}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            expected,
            "{backend}"
        );
        assert!(
            output.stderr.is_empty(),
            "{backend}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A malformed policy input stops every program before Start on both
/// backends (0.6.md: `CONFIG_INVALID`, exit 2) with the same text; natively the
/// check ran only when the program imported a HOST capability.
#[test]
fn malformed_policy_stops_programs_without_host_imports() {
    let directory = TestDir::new("policy-no-imports").expect("create directory");
    let source = directory.join("plain.bn");
    fs::write(
        &source,
        "FUNCTION Start() AS VOID\nPRINT \"ran\"\nEND FUNCTION\n",
    )
    .expect("write program");
    let artifact = compile_native(&source, &directory);
    let interpreted = execute(
        bni().arg("run").arg(&source).env("BN_EXEC_POLICY", "allow"),
        None,
    );
    let compiled = execute(Command::new(&artifact).env("BN_EXEC_POLICY", "allow"), None);
    for (backend, output) in [("bni", &interpreted), ("native", &compiled)] {
        assert_eq!(output.status.code(), Some(2), "{backend}");
        assert!(output.stdout.is_empty(), "{backend}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "error[CONFIG_INVALID]: invalid BN_EXEC_POLICY 'allow' (expected deny)\n",
            "{backend}"
        );
    }
}

/// A scalar or STRING stored into an alternative binding, argument, or
/// field is wrapped natively as `RETURN` wraps it (it stored the bare
/// scalar, and clang rejected the LLVM).
#[test]
fn scalars_stored_into_alternatives_match_the_interpreter() {
    let path = valid_fixture("scalar-into-alternative.bn");
    let output = interpret(&path, None);
    assert_success(&output, "bni run");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "count 3\nTRUE\nx\nint 5\nint 7\nfloat 2.5\n"
    );
    assert_native_parity(&path, None);
}

/// D9: `NEW FS.File()` and a plain `FS.File` binding or parameter compile
/// natively and behave as in the interpreter (a never-opened file).
#[test]
fn never_opened_files_match_the_interpreter() {
    let path = workspace_root().join("tests/host/fs_new_file.bn");
    let output = interpret(&path, None);
    assert_success(&output, "bni run");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "TRUE Error 5 in HOST.FileSystem.File.ReadLine: cannot read a line from a file (cause: the file was never opened (NEW FS.File() makes a closed file))\n\
         TRUE HOST.FileSystem.File.Write\n"
    );
    assert_native_parity(&path, None);
}

/// `IMPORT HOST.FileSystem AS Disk`: `Disk.File` is the type `Disk.Open`
/// returns (it was "Expected Disk.File OR Error, but found FS.File OR Error"
/// on both backends), in bindings, `IS` tests, and parameters.
#[test]
fn filesystem_types_do_not_depend_on_the_import_alias() {
    let path = workspace_root().join("tests/host/fs_alias.bn");
    let output = interpret(&path, None);
    assert_success(&output, "bni run");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "TRUE hello\nFALSE\n"
    );
    assert_native_parity(&path, None);
}

/// HOST types and methods (`FS.File.ReadAll`, `FS.File.Close`, etc.) invoked
/// from an imported module do not receive a module prefix (`#N.FS.File.ReadAll`),
/// compiling and interpreting correctly across backends.
#[test]
fn host_methods_in_imported_modules_match_the_interpreter() {
    let directory = TestDir::new("imported-fs").expect("create test dir");
    let helper = directory.join("ConfigLoader.bn");
    fs::write(
        &helper,
        "IMPORT HOST.FileSystem AS FS\n\
         EXPORT CLASS ConfigLoader\n\
             PUBLIC FUNCTION CONSTRUCTOR()\n\
             END FUNCTION\n\
             PUBLIC FUNCTION Read(path AS STRING) AS STRING\n\
                 LET file AS FS.File OR Error = FS.Open(path, FS.READ)\n\
                 IF file IS Error THEN\n\
                     RETURN \"open-error: \" + file.Message\n\
                 END IF\n\
                 LET text AS STRING OR Error = file.ReadAll()\n\
                 file.Close()\n\
                 IF text IS Error THEN\n\
                     RETURN \"read-error\"\n\
                 END IF\n\
                 RETURN text\n\
             END FUNCTION\n\
         END CLASS\n",
    )
    .expect("write ConfigLoader.bn");
    let main_file = directory.join("main.bn");
    let data_file = directory.join("data.txt");
    fs::write(&data_file, "send-to-kindle-ok").expect("write data.txt");
    let data_path_literal = data_file.display().to_string().replace('\\', "\\\\");
    fs::write(
        &main_file,
        format!(
            "IMPORT ConfigLoader AS ConfigLoader\n\
             FUNCTION Start() AS VOID\n\
                 LET loader AS ConfigLoader.ConfigLoader = NEW ConfigLoader.ConfigLoader()\n\
                 PRINT loader.Read(\"{data_path_literal}\")\n\
             END FUNCTION\n",
        ),
    )
    .expect("write main.bn");

    let interpreted = interpret(&main_file, None);
    assert_success(&interpreted, "bni run imported host call");
    assert_eq!(
        String::from_utf8_lossy(&interpreted.stdout),
        "send-to-kindle-ok\n"
    );

    let artifact = compile_native(&main_file, &directory);
    let compiled = execute_artifact(&artifact, &[], None);
    assert_success(&compiled, "native run imported host call");
    assert_eq!(compiled.stdout, interpreted.stdout);
}

/// `Env.Get` / `Env.Has` read the process environment with the same result
/// on both backends: a set value, an empty value, present and absent names.
#[test]
fn env_reads_the_process_environment_on_both_backends() {
    let path = workspace_root().join("tests/host/env.bn");
    let build = TestDir::new("env-read").expect("create directory");
    let artifact = compile_native(&path, &build);
    let environment = |command: &mut Command| {
        command
            .env("BN_TEST_ENV", "value")
            .env("BN_TEST_EMPTY", "")
            .env_remove("BN_NONEXISTENT_VAR");
    };
    let mut interpreted_command = bni();
    interpreted_command.arg("run").arg(&path);
    environment(&mut interpreted_command);
    let mut compiled_command = Command::new(&artifact);
    environment(&mut compiled_command);
    let interpreted = execute(&mut interpreted_command, None);
    let compiled = execute(&mut compiled_command, None);
    for (backend, output) in [("bni", &interpreted), ("native", &compiled)] {
        assert_eq!(output.status.code(), Some(0), "{backend}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "value\n0\nTRUE FALSE\n",
            "{backend}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A value that is not UTF-8 is `Env.INVALID_UTF8` on both backends. Unix
/// only: the test builds the value from raw bytes; Windows is covered by the
/// `bn_host_env` unit test with an unpaired surrogate.
#[cfg(unix)]
#[test]
fn env_value_not_utf8_is_invalid_utf8_on_both_backends() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    let path = workspace_root().join("tests/host/env_invalid_utf8.bn");
    let build = TestDir::new("env-utf8").expect("create directory");
    let artifact = compile_native(&path, &build);
    let value = OsStr::from_bytes(b"a\xffb");
    let interpreted = execute(bni().arg("run").arg(&path).env("BN_BAD_UTF8", value), None);
    let compiled = execute(Command::new(&artifact).env("BN_BAD_UTF8", value), None);
    for (backend, output) in [("bni", &interpreted), ("native", &compiled)] {
        assert_eq!(output.status.code(), Some(0), "{backend}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "TRUE HOST.Env.Get\nTRUE\n",
            "{backend}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// A malformed `BN_ENV_POLICY` stops the program before `Start` on both
/// backends (`CONFIG_INVALID`, exit 2) with the same text.
#[test]
fn malformed_env_policy_stops_both_backends() {
    let path = workspace_root().join("tests/host/env_policy_deny.bn");
    let build = TestDir::new("env-policy-bogus").expect("create directory");
    let artifact = compile_native(&path, &build);
    let interpreted = execute(
        bni().arg("run").arg(&path).env("BN_ENV_POLICY", "bogus"),
        None,
    );
    let compiled = execute(Command::new(&artifact).env("BN_ENV_POLICY", "bogus"), None);
    for (backend, output) in [("bni", &interpreted), ("native", &compiled)] {
        assert_eq!(output.status.code(), Some(2), "{backend}");
        assert!(output.stdout.is_empty(), "{backend}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "error[CONFIG_INVALID]: invalid BN_ENV_POLICY 'bogus' (expected deny)\n",
            "{backend}"
        );
    }
}

