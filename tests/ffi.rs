#![allow(clippy::similar_names)]
// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `BNFfi` integration, parity, and compliance tests.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[path = "support/mod.rs"]
pub(crate) mod support;
use support::{TestDir, bni, compile_native, run, workspace_root};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

fn shared_library_filename_for_os(base_name: &str, target_os: &str) -> String {
    if target_os == "windows" {
        format!("{base_name}.dll")
    } else if target_os == "macos" {
        format!("lib{base_name}.dylib")
    } else {
        format!("lib{base_name}.so")
    }
}

fn shared_library_compiler_flags_for_os(target_os: &str) -> &'static [&'static str] {
    if target_os == "windows" {
        &["-shared", "-o"]
    } else if target_os == "macos" {
        &["-dynamiclib", "-o"]
    } else {
        &["-shared", "-fPIC", "-o"]
    }
}

fn system_c_library_name_for_os(target_os: &str) -> &'static str {
    if target_os == "windows" {
        "ucrtbase"
    } else if target_os == "macos" {
        "libSystem.B.dylib"
    } else {
        "libc.so.6"
    }
}

fn build_c_library(source: &Path, out_dir: &Path, base_name: &str) -> PathBuf {
    let lib_name = shared_library_filename_for_os(base_name, env::consts::OS);
    let out_path = out_dir.join(&lib_name);

    let cc = env::var("CC").unwrap_or_else(|_| "clang".to_string());
    let mut cmd = Command::new(&cc);
    cmd.args(shared_library_compiler_flags_for_os(env::consts::OS))
        .arg(&out_path)
        .arg(source);

    let status = cmd.status().unwrap_or_else(|err| {
        panic!("failed to run C compiler {cc}: {err}");
    });
    assert!(
        status.success(),
        "C library build failed for {}",
        source.display()
    );
    out_path
}

#[test]
fn s6_a_and_b_full_suite_parity() {
    let root = workspace_root();
    let temp_dir = TestDir::new("bnffi-test").expect("create test dir");
    let c_source = root.join("tests/ffi/bnffi_test.c");
    build_c_library(&c_source, temp_dir.path(), "bnffi_test");

    let bn_source = root.join("tests/ffi/test_full_suite.bn");

    // 1. Run under interpreter (bni)
    let mut bni_cmd = bni();
    bni_cmd
        .arg("run")
        .arg("--ffi-path")
        .arg(temp_dir.path())
        .arg(&bn_source);
    let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        bni_out.status.success(),
        "bni run failed: {}",
        String::from_utf8_lossy(&bni_out.stderr)
    );
    let bni_stdout = String::from_utf8_lossy(&bni_out.stdout);
    assert!(
        bni_stdout.contains("ALL INTERPRETER BNFfi TESTS PASSED"),
        "unexpected bni output: {bni_stdout}"
    );

    // 2. Compile and run under native compiler (bnc)
    let native_bin = compile_native(&bn_source, &temp_dir);
    let mut native_cmd = Command::new(&native_bin);
    native_cmd.env("BN_LIBRARY_PATH", temp_dir.path());
    let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run compiled native binary");
    assert!(
        bnc_out.status.success(),
        "bnc binary failed: {}",
        String::from_utf8_lossy(&bnc_out.stderr)
    );
    let bnc_stdout = String::from_utf8_lossy(&bnc_out.stdout);
    assert!(
        bnc_stdout.contains("ALL INTERPRETER BNFfi TESTS PASSED"),
        "unexpected bnc output: {bnc_stdout}"
    );

    // Parity assertion
    assert_eq!(
        bni_stdout, bnc_stdout,
        "Interpreter and compiler outputs must match exactly"
    );
}

#[test]
fn s6_d_new_handle_rejected_statically() {
    let source = "IMPORT BNFfi AS C\nFUNCTION Start() AS VOID\nLET h AS C.Handle = NEW C.Handle()\nEND FUNCTION\n";
    let temp_dir = TestDir::new("bnffi-handle-check").expect("create test dir");
    let file_path = temp_dir.join("check_handle.bn");
    fs::write(&file_path, source).expect("write check_handle.bn");

    let mut cmd = bni();
    cmd.arg("check").arg(&file_path);
    let out = run(&mut cmd, None, PROCESS_TIMEOUT).expect("run bni check");
    assert!(
        !out.status.success(),
        "NEW C.Handle() must be rejected because constructor is private"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("private")
            || stderr.contains("PRIVATE")
            || stderr.contains("cannot be called")
            || stderr.contains("constructor")
            || stderr.contains("CONSTRUCTOR"),
        "Expected private constructor diagnostic, got: {stderr}"
    );
}

#[test]
fn s6_e_resolution_order_precedence() {
    let root = workspace_root();
    let dir1 = TestDir::new("bnffi-res1").expect("create test dir 1");
    let dir2 = TestDir::new("bnffi-res2").expect("create test dir 2");

    let c_source = root.join("tests/ffi/bnffi_test.c");
    build_c_library(&c_source, dir1.path(), "bnffi_test");

    let bn_source = dir1.join("test_res_order.bn");
    let src = "\
IMPORT BNFfi AS C
FUNCTION Start() AS INTEGER
    LET args AS C.Args = NEW C.Args()
    args.Add(NEW C.Double(2.5))
    args.Add(NEW C.Double(4.0))
    LET res AS C.Double = NEW C.Double(0.0)
    LET ok AS VOID OR Error = C.Call(\"bnffi_test\", \"test_double\", args, res)
    IF ok IS Error THEN
        RETURN 1
    END IF
    RETURN 0
END FUNCTION
";
    fs::write(&bn_source, src).expect("write test_res_order.bn");

    // --ffi-path takes precedence over BN_LIBRARY_PATH
    let mut cmd = bni();
    cmd.arg("run")
        .arg("--ffi-path")
        .arg(dir1.path())
        .arg(&bn_source)
        .env("BN_LIBRARY_PATH", dir2.path());
    let out = run(&mut cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        out.status.success(),
        "s6_e failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn s6_g_system_library_smoke() {
    assert_eq!(system_c_library_name_for_os("windows"), "ucrtbase");
    assert_eq!(system_c_library_name_for_os("macos"), "libSystem.B.dylib");
    assert_eq!(system_c_library_name_for_os("linux"), "libc.so.6");
    let temp_dir = TestDir::new("bnffi-sys-smoke").expect("create test dir");
    let libc_name = system_c_library_name_for_os(env::consts::OS);
    let bn_source = temp_dir.join("test_strlen_smoke.bn");
    let src = format!(
        "\
IMPORT BNFfi AS C
FUNCTION Start() AS INTEGER
    LET args AS C.Args = NEW C.Args()
    args.Add(NEW C.String(\"hello foreign function interface\"))
    LET res AS C.SizeT = NEW C.SizeT(0)
    LET ok AS VOID OR Error = C.Call(\"{libc_name}\", \"strlen\", args, res)
    IF ok IS Error THEN
        PRINT \"ERROR: \", ok.Message
        RETURN 1
    END IF
    PRINT \"LENGTH: \", res.Value()
    RETURN 0
END FUNCTION
"
    );
    fs::write(&bn_source, src).expect("write test_strlen_smoke.bn");

    // Interpreter
    let mut bni_cmd = bni();
    bni_cmd.arg("run").arg(&bn_source);
    let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        bni_out.status.success(),
        "bni failed: {}",
        String::from_utf8_lossy(&bni_out.stderr)
    );
    let bni_stdout = String::from_utf8_lossy(&bni_out.stdout);
    assert!(bni_stdout.contains("LENGTH:  32"));

    // Native
    let native_bin = compile_native(&bn_source, &temp_dir);
    let mut native_cmd = Command::new(&native_bin);
    let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run native");
    assert!(
        bnc_out.status.success(),
        "bnc failed: {}",
        String::from_utf8_lossy(&bnc_out.stderr)
    );
    let bnc_stdout = String::from_utf8_lossy(&bnc_out.stdout);
    assert!(bnc_stdout.contains("LENGTH:  32"));

    assert_eq!(bni_stdout, bnc_stdout);
}

#[test]
fn s6_i_ac12_marker_no_load_on_check() {
    let root = workspace_root();
    let temp_dir = TestDir::new("bnffi-init").expect("create test dir");
    let c_source = root.join("tests/ffi/bnffi_init_marker.c");
    build_c_library(&c_source, temp_dir.path(), "bnffi_init_marker");

    let marker_file = temp_dir.join("bnffi_init_marker.tmp");
    if marker_file.exists() {
        let _ = fs::remove_file(&marker_file);
    }

    let program_src = "IMPORT BNFfi AS C\nFUNCTION Start() AS INTEGER\nLET args AS C.Args = NEW C.Args()\nLET res AS C.Int32 = NEW C.Int32(0)\nLET ok AS VOID OR Error = C.Call(\"bnffi_init_marker\", \"get_marker_value\", args, res)\nRETURN 0\nEND FUNCTION\n";
    let program_path = temp_dir.join("program.bn");
    fs::write(&program_path, program_src).expect("write program.bn");

    // 1. `bni check` must NOT load library or create marker file
    let mut check_cmd = bni();
    check_cmd.current_dir(temp_dir.path());
    check_cmd.arg("check").arg(&program_path);
    let check_out = run(&mut check_cmd, None, PROCESS_TIMEOUT).expect("run bni check");
    assert!(check_out.status.success());
    assert!(
        !marker_file.exists(),
        "AC12 violation: bni check triggered library initializer!"
    );

    // 2. `bni run` MUST load the library and trigger initializer
    let mut run_cmd = bni();
    run_cmd.current_dir(temp_dir.path());
    run_cmd
        .arg("run")
        .arg("--ffi-path")
        .arg(temp_dir.path())
        .arg(&program_path);
    let run_out = run(&mut run_cmd, None, PROCESS_TIMEOUT).expect("run bni run");
    assert!(run_out.status.success());
    assert!(
        marker_file.exists(),
        "bni run should have executed library and triggered initializer"
    );
}

#[test]
fn iss01_use_after_release_region_rejected_in_bni_and_bnc() {
    let root = workspace_root();
    let temp_dir = TestDir::new("bnffi-uar").expect("create test dir");
    let c_source = root.join("tests/ffi/bnffi_test.c");
    build_c_library(&c_source, temp_dir.path(), "bnffi_test");

    let cases = [
        (
            "uar_call.bn",
            "\
IMPORT BNFfi AS C
FUNCTION Start() AS INTEGER
    LET arr AS POINTER TO FLOAT[] = NEW FLOAT[3]
    LET darr AS C.DoubleArray = NEW C.DoubleArray(arr)
    RELEASE arr
    LET scale_args AS C.Args = NEW C.Args()
    scale_args.Add(darr)
    scale_args.Add(NEW C.Int32(3))
    scale_args.Add(NEW C.Double(2.0))
    LET void_res AS C.Void = NEW C.Void()
    LET ok AS VOID OR Error = C.Call(\"bnffi_test\", \"test_double_array_scale\", scale_args, void_res)
    RETURN 0
END FUNCTION
",
        ),
        (
            "uar_read_from.bn",
            "\
IMPORT BNFfi AS C
FUNCTION Start() AS INTEGER
    LET res_ptr AS C.HandleResult = NEW C.HandleResult()
    LET ok AS VOID OR Error = C.Call(\"bnffi_test\", \"test_static_string\", NEW C.Args(), res_ptr)
    LET h AS C.Handle OR NULL = res_ptr.Value()
    IF h IS NULL THEN
        RETURN 1
    END IF
    LET arr AS POINTER TO BYTE[] = NEW BYTE[4]
    LET barr AS C.ByteArray = NEW C.ByteArray(arr)
    RELEASE arr
    LET r AS VOID OR Error = barr.ReadFrom(h)
    RETURN 0
END FUNCTION
",
        ),
        (
            "uar_write_to.bn",
            "\
IMPORT BNFfi AS C
FUNCTION Start() AS INTEGER
    LET alloc_args AS C.Args = NEW C.Args()
    alloc_args.Add(NEW C.SizeT(16))
    LET res_ptr AS C.HandleResult = NEW C.HandleResult()
    LET ok AS VOID OR Error = C.Call(\"bnffi_test\", \"test_alloc_bytes\", alloc_args, res_ptr)
    LET h AS C.Handle OR NULL = res_ptr.Value()
    IF h IS NULL THEN
        RETURN 1
    END IF
    LET arr AS POINTER TO BYTE[] = NEW BYTE[4]
    LET barr AS C.ByteArray = NEW C.ByteArray(arr)
    RELEASE arr
    LET w AS VOID OR Error = barr.WriteTo(h)
    RETURN 0
END FUNCTION
",
        ),
    ];

    for (filename, program_src) in cases {
        let program_path = temp_dir.join(filename);
        fs::write(&program_path, program_src).expect("write uar program");

        let mut bni_cmd = bni();
        bni_cmd
            .arg("run")
            .arg("--ffi-path")
            .arg(temp_dir.path())
            .arg(&program_path);
        let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
        assert!(
            !bni_out.status.success(),
            "{filename}: bni must reject use-after-release on region"
        );
        let bni_stderr = String::from_utf8_lossy(&bni_out.stderr);
        assert!(
            bni_stderr.contains("USE_AFTER_RELEASE"),
            "{filename}: expected USE_AFTER_RELEASE from bni, got: {bni_stderr}"
        );

        let native_bin = compile_native(&program_path, &temp_dir);
        let mut native_cmd = Command::new(&native_bin);
        native_cmd.env("BN_LIBRARY_PATH", temp_dir.path());
        let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run native");
        assert!(
            !bnc_out.status.success(),
            "{filename}: bnc binary must reject use-after-release on region"
        );
        let bnc_stderr = String::from_utf8_lossy(&bnc_out.stderr);
        assert!(
            bnc_stderr.contains("USE_AFTER_RELEASE"),
            "{filename}: expected USE_AFTER_RELEASE from bnc binary, got: {bnc_stderr}"
        );
    }
}

#[test]
fn iss07_homonym_library_cache_isolation_by_ffi_path() {
    let dir_a = TestDir::new("bnffi-dup-a").expect("create dir_a");
    let dir_b = TestDir::new("bnffi-dup-b").expect("create dir_b");

    let c_src_a = dir_a.join("dup.c");
    let c_src_b = dir_b.join("dup.c");
    fs::write(
        &c_src_a,
        "#ifdef _WIN32\n#define EXPORT __declspec(dllexport)\n#else\n#define EXPORT\n#endif\nEXPORT int dup_val(void) { return 10; }\n",
    )
    .expect("write dup.c in dir_a");
    fs::write(
        &c_src_b,
        "#ifdef _WIN32\n#define EXPORT __declspec(dllexport)\n#else\n#define EXPORT\n#endif\nEXPORT int dup_val(void) { return 20; }\n",
    )
    .expect("write dup.c in dir_b");

    build_c_library(&c_src_a, dir_a.path(), "bnffi_dup");
    build_c_library(&c_src_b, dir_b.path(), "bnffi_dup");

    let prog_src = "\
IMPORT BNFfi AS C
FUNCTION Start() AS VOID
    LET res AS C.Int32 = NEW C.Int32(0)
    LET ok AS VOID OR Error = C.Call(\"bnffi_dup\", \"dup_val\", NEW C.Args(), res)
    IF ok IS Error THEN
        PRINT \"ERR:\", ok.Message
        RETURN
    END IF
    PRINT res.Value()
END FUNCTION
";
    let prog_path = dir_a.join("test_dup.bn");
    fs::write(&prog_path, prog_src).expect("write test_dup.bn");

    let mut cmd_a = bni();
    cmd_a
        .arg("run")
        .arg("--ffi-path")
        .arg(dir_a.path())
        .arg(&prog_path);
    let out_a = run(&mut cmd_a, None, PROCESS_TIMEOUT).expect("run with dir_a");
    assert!(out_a.status.success());
    assert_eq!(String::from_utf8_lossy(&out_a.stdout).trim(), "10");

    let mut cmd_b = bni();
    cmd_b
        .arg("run")
        .arg("--ffi-path")
        .arg(dir_b.path())
        .arg(&prog_path);
    let out_b = run(&mut cmd_b, None, PROCESS_TIMEOUT).expect("run with dir_b");
    assert!(out_b.status.success());
    assert_eq!(String::from_utf8_lossy(&out_b.stdout).trim(), "20");
}

fn expected_c_long_bound_line(c_long_bytes: usize) -> &'static str {
    if c_long_bytes == 4 {
        "BOUND: VALUE_OUT_OF_RANGE"
    } else {
        "BOUND: VAL: 2147483649"
    }
}

#[test]
fn iss09_c_long_platform_width_parity() {
    assert_eq!(expected_c_long_bound_line(4), "BOUND: VALUE_OUT_OF_RANGE");
    assert_eq!(expected_c_long_bound_line(8), "BOUND: VAL: 2147483649");

    let root = workspace_root();
    let temp_dir = TestDir::new("bnffi-clong").expect("create test dir");
    let c_source = root.join("tests/ffi/bnffi_test.c");
    build_c_library(&c_source, temp_dir.path(), "bnffi_test");

    // Executes both the in-range (`VAL:`) path and the symbolic error (`Error.Code`)
    // formatting path on every OS, plus the 32-bit/64-bit `C.Long` boundary call.
    let prog_src = "\
IMPORT BNFfi AS C

FUNCTION ReportCall(prefix AS STRING, ok AS VOID OR Error, res AS C.Long) AS VOID
    IF ok IS Error THEN
        IF ok.Code = C.VALUE_OUT_OF_RANGE THEN
            PRINT prefix, \"VALUE_OUT_OF_RANGE\"
        ELSE
            IF ok.Code = C.SYMBOL_NOT_FOUND THEN
                PRINT prefix, \"SYMBOL_NOT_FOUND\"
            ELSE
                PRINT prefix, \"OTHER_CODE:\", ok.Code
            END IF
        END IF
    ELSE
        PRINT prefix, \"VAL:\", res.Value()
    END IF
END FUNCTION

FUNCTION Start() AS VOID
    LET ok_args AS C.Args = NEW C.Args()
    ok_args.Add(NEW C.Long(100))
    ok_args.Add(NEW C.Long(23))
    LET ok_res AS C.Long = NEW C.Long(0)
    LET ok1 AS VOID OR Error = C.Call(\"bnffi_test\", \"test_long\", ok_args, ok_res)
    ReportCall(\"IN_RANGE:\", ok1, ok_res)

    LET err_res AS C.Long = NEW C.Long(0)
    LET ok2 AS VOID OR Error = C.Call(\"bnffi_test\", \"missing_symbol_xyz\", ok_args, err_res)
    ReportCall(\"ERR_PATH:\", ok2, err_res)

    LET args AS C.Args = NEW C.Args()
    args.Add(NEW C.Long(2147483648))
    args.Add(NEW C.Long(1))
    LET res AS C.Long = NEW C.Long(0)
    LET ok3 AS VOID OR Error = C.Call(\"bnffi_test\", \"test_long\", args, res)
    ReportCall(\"BOUND:\", ok3, res)
END FUNCTION
";
    let prog_path = temp_dir.join("clong_width.bn");
    fs::write(&prog_path, prog_src).expect("write clong_width.bn");

    let mut bni_cmd = bni();
    bni_cmd
        .arg("run")
        .arg("--ffi-path")
        .arg(temp_dir.path())
        .arg(&prog_path);
    let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        bni_out.status.success(),
        "bni failed: {}",
        String::from_utf8_lossy(&bni_out.stderr)
    );

    let native_bin = compile_native(&prog_path, &temp_dir);
    let mut native_cmd = Command::new(&native_bin);
    native_cmd.env("BN_LIBRARY_PATH", temp_dir.path());
    let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run native");
    assert!(bnc_out.status.success());

    let bni_stdout = String::from_utf8_lossy(&bni_out.stdout);
    let bnc_stdout = String::from_utf8_lossy(&bnc_out.stdout);
    assert_eq!(bni_stdout, bnc_stdout);
    let lines: Vec<&str> = bni_stdout.lines().map(str::trim).collect();
    assert_eq!(
        lines,
        vec![
            "IN_RANGE: VAL: 123",
            "ERR_PATH: SYMBOL_NOT_FOUND",
            expected_c_long_bound_line(std::mem::size_of::<std::os::raw::c_long>()),
        ]
    );
}

#[test]
fn iss03_bnc_rejects_ffi_path_per_d4() {
    let root = workspace_root();
    let bn_source = root.join("tests/ffi/test_strlen.bn");
    let temp_dir = TestDir::new("bnffi-bnc-flag").expect("create test dir");

    let mut cmd = support::bnc();
    cmd.arg("--ffi-path").arg(temp_dir.path()).arg(&bn_source);
    let out = run(&mut cmd, None, PROCESS_TIMEOUT).expect("run bnc");
    assert!(
        !out.status.success(),
        "bnc must reject --ffi-path because D4 restricts compiled programs to BN_LIBRARY_PATH"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown or repeated option '--ffi-path'"),
        "expected unknown option diagnostic from bnc, got: {stderr}"
    );
}

#[test]
fn temporal_inversion_after_close_rejected_across_subsystems_in_bni_and_bnc() {
    let temp_dir = TestDir::new("bn-temporal-close").expect("create test dir");
    let db_path = temp_dir
        .join("test.db")
        .to_string_lossy()
        .replace('\\', "/");
    let txt_file = temp_dir.join("sample.txt");
    fs::write(&txt_file, "hello").expect("write sample.txt");
    let txt_path = txt_file.to_string_lossy().replace('\\', "/");
    let prog_src = format!(
        "\
IMPORT HOST.FileSystem AS FS
IMPORT BNSqlite AS Sqlite
IMPORT BNData AS Data
FUNCTION Start() AS VOID
    LET conn AS Sqlite.Connection OR Error = Sqlite.Open(\"{db_path}\")
    IF conn IS Error THEN
        PRINT \"FAIL_OPEN\"
        RETURN
    END IF
    LET c1 AS VOID OR Error = conn.Close()
    LET qAfterClose AS Data.DataFrame OR Error = conn.Query(\"SELECT 1\")
    IF qAfterClose IS Error THEN
        PRINT \"SQLITE_CLOSED_OK\"
    END IF

    LET f AS FS.File OR Error = FS.Open(\"{txt_path}\", FS.READ)
    IF f IS Error THEN
        PRINT \"FAIL_FS\"
        RETURN
    END IF
    LET c2 AS VOID OR Error = f.Close()
    LET rAfterClose AS STRING OR Error = f.ReadAll()
    IF rAfterClose IS Error THEN
        PRINT \"FS_CLOSED_OK\"
    END IF
END FUNCTION
"
    );
    let prog_path = temp_dir.join("temporal_close.bn");
    fs::write(&prog_path, prog_src).expect("write temporal_close.bn");

    let mut bni_cmd = bni();
    bni_cmd.arg("run").arg(&prog_path);
    let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        bni_out.status.success(),
        "{}",
        String::from_utf8_lossy(&bni_out.stderr)
    );

    let native_bin = compile_native(&prog_path, &temp_dir);
    let mut native_cmd = Command::new(&native_bin);
    let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run native");
    assert!(
        bnc_out.status.success(),
        "{}",
        String::from_utf8_lossy(&bnc_out.stderr)
    );

    let bni_stdout = String::from_utf8_lossy(&bni_out.stdout);
    let bnc_stdout = String::from_utf8_lossy(&bnc_out.stdout);
    assert_eq!(bni_stdout, bnc_stdout);
    assert!(bni_stdout.contains("SQLITE_CLOSED_OK"));
}
