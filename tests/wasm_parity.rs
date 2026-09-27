mod support;

use std::{path::Path, process::Command, time::Duration};

use support::{ProcessOutput, TestDir, bnc, bni, run, workspace_root};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

fn execute(command: &mut Command, input: Option<&[u8]>) -> ProcessOutput {
    run(command, input, PROCESS_TIMEOUT).expect("run child process")
}

fn compile_wasm(source: &Path, directory: &TestDir) -> std::path::PathBuf {
    let artifact = directory.join("program.wasm");
    let output = execute(
        bnc()
            .args(["--target", "wasm32"])
            .arg(source)
            .arg("-o")
            .arg(&artifact),
        None,
    );
    assert!(
        output.status.success(),
        "bnc wasm32 failed for {}: {}",
        source.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    artifact
}

fn assert_wasm_parity(source: &Path, artifact: &Path, input: &[u8]) {
    let interpreted = execute(bni().arg("run").arg(source), Some(input));
    let compiled = execute(
        Command::new("node")
            .arg(workspace_root().join("bin/bn-wasm"))
            .arg(artifact),
        Some(input),
    );
    assert_eq!(
        compiled.status.code(),
        interpreted.status.code(),
        "{}",
        source.display()
    );
    assert_eq!(compiled.stdout, interpreted.stdout, "{}", source.display());
}

#[test]
fn non_tty_subset_matches_interpreter() {
    for fixture in [
        "empty-start.bn",
        "print-integer.bn",
        "print-float.bn",
        "print-string.bn",
        "host-random.bn",
        "host-random-twice.bn",
        "print-args-length.bn",
    ] {
        let source = workspace_root().join("tests/grammar/valid").join(fixture);
        let directory = TestDir::new("wasm-parity").expect("create Wasm directory");
        let artifact = compile_wasm(&source, &directory);
        assert_wasm_parity(&source, &artifact, b"");
    }
}

#[test]
fn input_matches_interpreter() {
    let source = workspace_root().join("tests/grammar/valid/build-input.bn");
    let directory = TestDir::new("wasm-input").expect("create Wasm input directory");
    let artifact = compile_wasm(&source, &directory);
    assert_wasm_parity(&source, &artifact, b"hello\r\n");
    let long_input = [vec![b'x'; 5_000], b"\n".to_vec()].concat();
    assert_wasm_parity(&source, &artifact, &long_input);
}
