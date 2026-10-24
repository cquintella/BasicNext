#[path = "../support/inventory_tests.rs"]
mod inventory_tests;

use std::{
    collections::HashSet,
    io::{Read, Write},
    process::Command,
    time::{Duration, Instant},
};

use super::support::{TestDir, run};

fn child_command(test_name: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command.args([
        "--ignored",
        "--exact",
        &format!("test_support::{test_name}"),
        "--nocapture",
    ]);
    command
}

#[test]
fn every_ignored_child_fixture_is_selected_exactly_once() {
    for name in [
        "support_success_child",
        "support_bytes_child",
        "support_input_child",
        "support_failure_child",
        "support_timeout_child",
    ] {
        let output = child_command(name)
            .arg("--list")
            .output()
            .expect("list child fixture");
        assert!(output.status.success(), "{name}: {output:?}");
        let listing = String::from_utf8(output.stdout).expect("UTF-8 test listing");
        assert_eq!(
            listing
                .lines()
                .filter(|line| line.ends_with(": test"))
                .count(),
            1,
            "{name}: {listing}"
        );
    }
}

#[test]
fn successful_child_fixture_runs() {
    let result = run(
        &mut child_command("support_success_child"),
        None,
        Duration::from_secs(5),
    )
    .expect("run successful child");
    assert!(result.status.success());
    assert!(
        result
            .stdout
            .windows(b"child-ok\n".len())
            .any(|part| part == b"child-ok\n")
    );
}

#[test]
#[ignore = "child fixture invoked by runner tests"]
fn support_success_child() {
    std::io::stdout()
        .write_all(b"child-ok\n")
        .expect("write child stdout");
}

#[test]
#[ignore = "child fixture invoked by runner tests"]
fn support_bytes_child() {
    std::io::stdout()
        .write_all(&[b'b', 0xff, 0x00, b'\n'])
        .expect("write non-UTF-8 child stdout");
}

#[test]
#[ignore = "child fixture invoked by runner tests"]
fn support_input_child() {
    let mut input = Vec::new();
    std::io::stdin()
        .read_to_end(&mut input)
        .expect("read child stdin");
    std::io::stdout()
        .write_all(&input)
        .expect("echo child stdin");
}

#[test]
#[ignore = "child fixture invoked by runner tests"]
fn support_failure_child() {
    std::io::stderr()
        .write_all(b"intentional failure\n")
        .expect("write child stderr");
    panic!("intentional child failure");
}

#[test]
#[ignore = "child fixture invoked by runner tests"]
fn support_timeout_child() {
    std::thread::sleep(Duration::from_secs(10));
}

#[test]
fn runner_preserves_bytes_and_optional_stdin() {
    let bytes = run(
        &mut child_command("support_bytes_child"),
        None,
        Duration::from_secs(5),
    )
    .expect("run byte-emitting child");
    assert!(bytes.status.success());
    assert!(!bytes.timed_out);
    assert!(
        bytes
            .stdout
            .windows(4)
            .any(|window| window == [b'b', 0xff, 0x00, b'\n'])
    );

    let input = b"caf\xc3\xa9\x00\xff\n";
    let echoed = run(
        &mut child_command("support_input_child"),
        Some(input),
        Duration::from_secs(5),
    )
    .expect("run stdin child");
    assert!(echoed.status.success());
    assert!(
        echoed
            .stdout
            .windows(input.len())
            .any(|window| window == input)
    );
}

#[test]
fn runner_retains_nonzero_failure_artifact() {
    let result = run(
        &mut child_command("support_failure_child"),
        None,
        Duration::from_secs(5),
    )
    .expect("run failing child");
    assert!(!result.status.success());
    assert!(!result.timed_out);
    assert!(
        result
            .stderr
            .windows(b"intentional failure".len())
            .any(|window| window == b"intentional failure")
    );
    let artifact = result.failure_artifact.expect("failure artifact path");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&artifact).expect("read failure artifact"))
            .expect("parse failure artifact");
    assert_eq!(report["status"], "failed");
    assert!(
        report["command"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    std::fs::remove_file(artifact).expect("remove failure artifact");
}

#[test]
fn runner_kills_and_reaps_a_timeout() {
    let started = Instant::now();
    let result = run(
        &mut child_command("support_timeout_child"),
        None,
        Duration::from_millis(100),
    )
    .expect("run timeout child");
    assert!(result.timed_out);
    assert!(started.elapsed() < Duration::from_secs(5));
    let artifact = result.failure_artifact.expect("timeout artifact path");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&artifact).expect("read timeout artifact"))
            .expect("parse timeout artifact");
    assert_eq!(report["status"], "timeout");
    assert_eq!(report["timeout_ms"], 100);
    std::fs::remove_file(artifact).expect("remove timeout artifact");
}

#[test]
fn temporary_paths_are_unique_and_removed_on_drop() {
    let directories: Vec<TestDir> = (0..16)
        .map(|_| TestDir::new("parallel").expect("create test directory"))
        .collect();
    let paths: HashSet<_> = directories
        .iter()
        .map(|dir| dir.path().to_owned())
        .collect();
    assert_eq!(paths.len(), directories.len());
    assert!(paths.iter().all(|path| path.is_dir()));
    drop(directories);
    assert!(paths.iter().all(|path| !path.exists()));
}
