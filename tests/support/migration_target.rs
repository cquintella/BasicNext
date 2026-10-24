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
