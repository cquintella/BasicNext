//! Integration tests of the `bnc` executable (Clang-like surface).
//! Relocated from the root `tests/bnc.rs` (bucket 0.6.0, 2.2/2.4); tests
//! of the deleted wrapper profiles (`-c`, `--check`, default interpret)
//! were retired with those flags (D-060-07).
use std::process::Command;

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn bnc() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bnc"));
    command.current_dir(WORKSPACE);
    command
}

#[test]
fn bnc_help_succeeds() {
    let output = bnc().arg("--help").output().expect("run bnc --help");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("usage: bnc [compile-options] <entry.bn>"));
    assert!(stdout.contains("--target native|wasm32"));
    assert!(
        !stdout.contains("-c, --compile"),
        "the wrapper profile flags are gone"
    );
}

#[test]
fn bnc_help_after_entry_reports_help() {
    let output = bnc()
        .args(["entry.bn", "--help"])
        .output()
        .expect("run bnc entry.bn --help");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("help is available via --help"));
    assert!(!stderr.contains("bni --help"));
}

#[test]
fn bnc_version_advertises_package_version() {
    let output = bnc().arg("--version").output().expect("run bnc --version");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        concat!("bnc ", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn bnc_missing_entry_fails() {
    let output = bnc().output().expect("run bnc without args");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn bnc_rejects_removed_wrapper_flags() {
    for flag in ["-c", "--check"] {
        let output = bnc()
            .args([flag, "tests/grammar/valid/print-integer.bn"])
            .output()
            .expect("run bnc with a removed flag");
        assert_eq!(output.status.code(), Some(2), "{flag}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("unknown or repeated option"));
    }
}

#[test]
fn bnc_compiles_wasm32() {
    let output = bnc()
        .args([
            "--emit",
            "llvm",
            "--target",
            "wasm32",
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bnc --emit llvm --target wasm32");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("define i32 @main"));
}

#[test]
fn bnc_defaults_to_compiling_native_executable_when_output_is_omitted() {
    let binary_name = if cfg!(windows) {
        "print-integer.exe"
    } else {
        "print-integer"
    };
    // The artifact lands in the working directory: a scratch one, not the
    // workspace, so a failed run leaves nothing in the source tree.
    let directory = std::env::temp_dir().join(format!("bnc-default-out-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create test directory");
    let binary_path = directory.join(binary_name);
    let built = bnc()
        .current_dir(&directory)
        .arg(std::path::Path::new(WORKSPACE).join("tests/grammar/valid/print-integer.bn"))
        .output()
        .expect("run bnc without -o");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        binary_path.is_file(),
        "expected binary at {}",
        binary_path.display()
    );
    let ran = Command::new(&binary_path)
        .output()
        .expect("run the default artifact");
    let _ = std::fs::remove_dir_all(&directory);
    assert!(ran.status.success());
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "42");
}

#[test]
fn bnc_builds_and_runs_a_native_executable() {
    let artifact = std::env::temp_dir().join(format!("bnc-native-{}", std::process::id()));
    let built = bnc()
        .args(["tests/grammar/valid/print-integer.bn", "-o"])
        .arg(&artifact)
        .output()
        .expect("run bnc -o");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let ran = Command::new(&artifact).output().expect("run the artifact");
    let _ = std::fs::remove_file(&artifact);
    assert!(ran.status.success());
    assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "42");
}

#[test]
fn bnc_emit_ir_prints_validated_bn_ir_without_compiling() {
    let output = bnc()
        .args(["--emit", "ir", "tests/grammar/valid/print-integer.bn"])
        .output()
        .expect("run bnc --emit ir");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Print"), "BN IR, not LLVM: {stdout}");
    assert!(!stdout.contains("define i32 @main"));
}

#[test]
fn bnc_language_error_exits_one_with_a_diagnostic() {
    let output = bnc()
        .args(["tests/grammar/invalid/cross-type-equality.bn"])
        .output()
        .expect("run bnc on an invalid program");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error["));
}

/// A `-g` program: `Greet` sits on line 1 and its `PRINT` on line 2.
const DEBUG_PROGRAM: &str = "FUNCTION Greet() AS VOID\n    PRINT \"hi\"\nEND FUNCTION\n\nFUNCTION Start() AS VOID\n    Greet()\nEND FUNCTION\n";

/// Where the linker leaves the debug information of `program`: the
/// executable (ELF), its dSYM bundle (macOS, written by dsymutil) or its
/// PDB (Windows).
fn debug_file(program: &std::path::Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        let name = program.file_name().expect("program name");
        program
            .with_extension("dSYM")
            .join("Contents/Resources/DWARF")
            .join(name)
    } else if cfg!(windows) {
        program.with_extension("pdb")
    } else {
        program.to_path_buf()
    }
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

#[test]
fn bnc_g_emits_debug_metadata_only_when_requested() {
    let directory = std::env::temp_dir().join(format!("bnc-g-ir-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create test directory");
    let source = directory.join("dbgprog.bn");
    std::fs::write(&source, DEBUG_PROGRAM).expect("write program");
    let emit = |debug: bool| {
        let mut command = bnc();
        if debug {
            command.arg("-g");
        }
        let output = command
            .args(["--emit", "llvm"])
            .arg(&source)
            .output()
            .expect("run bnc --emit llvm");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let plain = emit(false);
    let debug = emit(true);
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        !plain.contains("!dbg") && !plain.contains("llvm.dbg.cu"),
        "{plain}"
    );
    assert!(!debug.contains(";bn.dbg"), "markers must not leak: {debug}");
    assert!(
        debug.contains("!DIFile(filename: \"dbgprog.bn\""),
        "{debug}"
    );
    assert!(debug.contains("!DISubprogram(name: \"Greet\""), "{debug}");
    assert!(debug.contains("!DILocation(line: 2, column: 5"), "{debug}");
    assert!(debug.contains("!llvm.dbg.cu = !{"), "{debug}");
}

#[test]
fn bnc_g_writes_debug_information_into_the_native_artifact() {
    let directory = std::env::temp_dir().join(format!("bnc-g-native-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create test directory");
    let source = directory.join("dbgprog.bn");
    std::fs::write(&source, DEBUG_PROGRAM).expect("write program");
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    let build = |debug: bool, name: &str| {
        let program = directory.join(format!("{name}{suffix}"));
        let mut command = bnc();
        if debug {
            command.arg("-g");
        }
        let built = command
            .arg(&source)
            .arg("-o")
            .arg(&program)
            .output()
            .expect("run bnc");
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let ran = Command::new(&program).output().expect("run the artifact");
        assert_eq!(String::from_utf8_lossy(&ran.stdout).trim(), "hi");
        program
    };
    let with = debug_file(&build(true, "with"));
    let without = debug_file(&build(false, "without"));
    let with_bytes = std::fs::read(&with);
    let without_bytes = std::fs::read(&without).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&directory);
    let with_bytes = with_bytes.unwrap_or_else(|error| panic!("{}: {error}", with.display()));
    assert!(
        contains(&with_bytes, "dbgprog.bn"),
        "-g describes the source file"
    );
    if !cfg!(windows) {
        assert!(
            contains(&with_bytes, "debug_line"),
            "-g writes a line table"
        );
    }
    assert!(
        !contains(&without_bytes, "dbgprog.bn"),
        "without -g the artifact names no Basic Next source"
    );
}

/// The backend walks hash maps; it must sort what it emits. These programs
/// exercised every unordered loop (slots, owned objects, weak symbols).
#[test]
fn bnc_emit_llvm_is_deterministic() {
    for program in [
        "examples/linear_collections.bn",
        "examples/language-tour.bn",
        "tests/grammar/valid/arc-weak-survives-unrelated-allocation.bn",
    ] {
        let emit = || {
            let output = bnc()
                .args(["--emit", "llvm", program])
                .output()
                .expect("run bnc --emit llvm");
            assert!(output.status.success(), "{program}");
            output.stdout
        };
        let first = emit();
        for _ in 0..4 {
            assert!(emit() == first, "{program}: LLVM text differs between runs");
        }
    }
}

/// Inspection of the ARC core from a native debugger (proposal
/// `arc-shared-core-0.6.5`): in a `bnc -g` program, `bn_rt_arc_dump()` lists
/// the live objects. `lldb` on macOS, `gdb` on Linux (CI installs it).
#[cfg(unix)]
#[test]
fn a_native_debugger_reads_the_arc_core() {
    let directory = std::env::temp_dir().join(format!("bnc-arc-debugger-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create test directory");
    let source = directory.join("arcdbg.bn");
    std::fs::write(
        &source,
        "CLASS Box\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\n\nFUNCTION Start() AS VOID\n    LET b AS Box = NEW Box()\n    PRINT \"made\"\nEND FUNCTION\n",
    )
    .expect("write program");
    let program = directory.join("arcdbg");
    let built = bnc()
        .args(["-g"])
        .arg(&source)
        .arg("-o")
        .arg(&program)
        .output()
        .expect("run bnc");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    // Stop where the program releases its object, then ask the core.
    let session = if cfg!(target_os = "macos") {
        Command::new("lldb")
            .args([
                "--batch",
                "-o",
                "breakpoint set -n bn_rt_arc_release",
                "-o",
                "run",
                "-o",
                "expr (void)bn_rt_arc_dump()",
                "-o",
                "kill",
            ])
            .arg(&program)
            .output()
            .expect("run lldb (part of the Xcode command line tools)")
    } else {
        Command::new("gdb")
            .args([
                "-batch",
                "-ex",
                "break bn_rt_arc_release",
                "-ex",
                "run",
                // The breakpoint stops in a Rust frame, where `(void)` is not
                // an expression; the call is written in C.
                "-ex",
                "set language c",
                "-ex",
                "call (void)bn_rt_arc_dump()",
                "-ex",
                "kill",
            ])
            .arg(&program)
            .output()
            .expect("run gdb")
    };
    let transcript = format!(
        "{}{}",
        String::from_utf8_lossy(&session.stdout),
        String::from_utf8_lossy(&session.stderr)
    );
    assert!(
        transcript.contains("arc: 1 live object(s)") && transcript.contains("Box#0 strong=1"),
        "{transcript}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn bnc_cpu_option_validates_and_compiles() {
    let directory = std::env::temp_dir().join(format!("bnc-cpu-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&directory);
    let output_native = directory.join("test_cpu_native");

    // Compiling with --cpu native works for native target
    let compile_native = bnc()
        .args([
            "--cpu",
            "native",
            "-o",
            output_native.to_str().unwrap(),
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bnc --cpu native");
    assert!(
        compile_native.status.success(),
        "{}",
        String::from_utf8_lossy(&compile_native.stderr)
    );

    // Compiling with --cpu generic works for native target
    let output_generic = directory.join("test_cpu_generic");
    let compile_generic = bnc()
        .args([
            "--cpu",
            "generic",
            "-o",
            output_generic.to_str().unwrap(),
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bnc --cpu generic");
    assert!(
        compile_generic.status.success(),
        "{}",
        String::from_utf8_lossy(&compile_generic.stderr)
    );

    // Compiling with --cpu native and --target wasm32 is rejected with CONFIG_INVALID
    let output_wasm = directory.join("test_cpu.wasm");
    let compile_wasm = bnc()
        .args([
            "--target",
            "wasm32",
            "--cpu",
            "native",
            "-o",
            output_wasm.to_str().unwrap(),
            "tests/grammar/valid/print-integer.bn",
        ])
        .output()
        .expect("run bnc --target wasm32 --cpu native");
    assert!(!compile_wasm.status.success());
    let stderr = String::from_utf8_lossy(&compile_wasm.stderr);
    assert!(
        stderr.contains("--cpu native is not supported when targeting wasm32"),
        "stderr: {stderr}"
    );

    let _ = std::fs::remove_dir_all(&directory);
}
