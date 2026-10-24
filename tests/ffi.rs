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

fn build_c_library(source: &Path, out_dir: &Path, base_name: &str) -> PathBuf {
    let lib_name = if cfg!(target_os = "windows") {
        format!("{base_name}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{base_name}.dylib")
    } else {
        format!("lib{base_name}.so")
    };
    let out_path = out_dir.join(&lib_name);

    let cc = env::var("CC").unwrap_or_else(|_| "clang".to_string());
    let mut cmd = Command::new(&cc);
    if cfg!(target_os = "windows") {
        cmd.args(["-shared", "-o"]).arg(&out_path).arg(source);
    } else if cfg!(target_os = "macos") {
        cmd.args(["-dynamiclib", "-o"]).arg(&out_path).arg(source);
    } else {
        cmd.args(["-shared", "-fPIC", "-o"])
            .arg(&out_path)
            .arg(source);
    }

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
    let source =
        "IMPORT BNFfi AS C\nFUNCTION Start() AS VOID\nLET h = NEW C.Handle()\nEND FUNCTION\n";
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

    let bn_source = root.join("tests/ffi/test_strlen.bn");

    // --ffi-path takes precedence over BN_LIBRARY_PATH
    let mut cmd = bni();
    cmd.arg("run")
        .arg("--ffi-path")
        .arg(dir1.path())
        .arg(&bn_source)
        .env("BN_LIBRARY_PATH", dir2.path());
    let out = run(&mut cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(out.status.success());
}

#[test]
fn s6_g_system_library_smoke() {
    let root = workspace_root();
    let bn_source = root.join("tests/ffi/test_strlen.bn");
    let temp_dir = TestDir::new("bnffi-sys-smoke").expect("create test dir");

    // Interpreter
    let mut bni_cmd = bni();
    bni_cmd.arg("run").arg(&bn_source);
    let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(bni_out.status.success());
    let bni_stdout = String::from_utf8_lossy(&bni_out.stdout);
    assert!(bni_stdout.contains("LENGTH:  32"));

    // Native
    let native_bin = compile_native(&bn_source, &temp_dir);
    let mut native_cmd = Command::new(&native_bin);
    let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run native");
    assert!(bnc_out.status.success());
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

    let program_src = "\
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
";
    let program_path = temp_dir.join("uar_region.bn");
    fs::write(&program_path, program_src).expect("write uar_region.bn");

    let mut bni_cmd = bni();
    bni_cmd
        .arg("run")
        .arg("--ffi-path")
        .arg(temp_dir.path())
        .arg(&program_path);
    let bni_out = run(&mut bni_cmd, None, PROCESS_TIMEOUT).expect("run bni");
    assert!(
        !bni_out.status.success(),
        "bni must reject use-after-release on region passed to C.Int64Array"
    );
    let bni_stderr = String::from_utf8_lossy(&bni_out.stderr);
    assert!(
        bni_stderr.contains("USE_AFTER_RELEASE"),
        "expected USE_AFTER_RELEASE from bni, got: {bni_stderr}"
    );

    let native_bin = compile_native(&program_path, &temp_dir);
    let mut native_cmd = Command::new(&native_bin);
    native_cmd.env("BN_LIBRARY_PATH", temp_dir.path());
    let bnc_out = run(&mut native_cmd, None, PROCESS_TIMEOUT).expect("run native");
    assert!(
        !bnc_out.status.success(),
        "bnc binary must reject use-after-release on region passed to C.Int64Array"
    );
    let bnc_stderr = String::from_utf8_lossy(&bnc_out.stderr);
    assert!(
        bnc_stderr.contains("USE_AFTER_RELEASE"),
        "expected USE_AFTER_RELEASE from bnc binary, got: {bnc_stderr}"
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
