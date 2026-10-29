//! Compare one migrated Cargo test binary with its ledger destinations.

use std::{collections::BTreeSet, fs, path::Path, process::Command};

fn listed_tests(target: &str, ignored_only: bool) -> BTreeSet<String> {
    let mut command = Command::new(std::env::current_exe().expect("test executable"));
    command.args(["--list", "--format", "terse"]);
    if ignored_only {
        command.arg("--ignored");
    }
    let output = command.output().expect("list tests");
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .expect("UTF-8 test listing")
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .map(|name| format!("bn::test/{target}::{name}"))
        .collect()
}

pub fn assert_matches_ledger(target: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ledger =
        fs::read_to_string(root.join("tests/migration-ledger.csv")).expect("migration ledger");
    let mut mapped = BTreeSet::new();
    let mut ignored = BTreeSet::new();
    for line in ledger.lines().skip(1) {
        let fields = line.split("\",\"").collect::<Vec<_>>();
        assert!(fields.len() >= 7, "malformed ledger row: {line}");
        if fields[4] != target {
            continue;
        }
        if fields[6].trim_end_matches('"') != "migrated" {
            continue;
        }
        let destination = fields[5];
        assert!(
            mapped.insert(destination.to_owned()),
            "duplicate destination: {destination}"
        );
        if line.ends_with(",true") {
            ignored.insert(destination.to_owned());
        }
    }
    #[cfg(not(unix))]
    for unix_only in [
        "bn::test/execute::host_exec::host_exec_e11_signal_termination_negative_return_code",
        "bn::test/programs::compress_policy::bncompress_never_writes_through_a_symlink_on_both_backends",
        "bn::test/programs::fs_directories_policy::directory_operations_obey_every_policy_on_both_backends",
        "bn::test/programs::compiler_parity::env_value_not_utf8_is_invalid_utf8_on_both_backends",
        "bn::test/programs::parity::console_size_uses_stdout_when_stdin_is_piped",
        "bn::test/programs::parity_regressions::the_end_of_an_async_task_closes_its_files",
        "bn::test/values::ownership_matrix::native_ownership_runs_clean_under_address_sanitizer",
        "bn::test/values::ownership_matrix::native_strings_leave_no_buffer_behind",
        "bn::test/values::value_matrix::native_values_run_clean_under_address_sanitizer",
    ] {
        mapped.remove(unix_only);
        ignored.remove(unix_only);
    }
    assert!(!mapped.is_empty(), "no mapped tests for {target}");
    assert_eq!(
        mapped,
        listed_tests(target, false),
        "{target} identity drift"
    );
    assert_eq!(
        ignored,
        listed_tests(target, true),
        "{target} ignored identity drift"
    );
}
