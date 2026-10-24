//! The migration ledger keeps every baseline identity accounted for.

use std::{collections::BTreeSet, fs, path::Path};

use sha2::{Digest, Sha256};

const LEDGER: &str = "tests/migration-ledger.csv";
const S0_BASELINE_SHA256: &str = "77e8c470fd5c6b23fa5d01dedfcb9dae3149ed587bfa718b1b96f7cebbc17255";

fn fixture_paths(directory: &Path, paths: &mut BTreeSet<String>, root: &Path) {
    for entry in fs::read_dir(directory).expect("grammar fixture directory") {
        let path = entry.expect("grammar fixture entry").path();
        if path.is_dir() {
            fixture_paths(&path, paths, root);
        } else if path.extension().is_some_and(|extension| extension == "bn") {
            let relative = path.strip_prefix(root).expect("fixture under workspace");
            paths.insert(format!(
                "fixture::{}",
                relative.to_string_lossy().replace('\\', "/")
            ));
        }
    }
}

#[test]
fn migrated_gate_tests_and_grammar_fixtures_match_the_ledger() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ledger = fs::read_to_string(root.join(LEDGER)).expect("migration ledger");
    let mut old_ids = BTreeSet::new();
    let mut new_ids = BTreeSet::new();
    let mut fixtures = BTreeSet::new();
    let mut cells = BTreeSet::new();
    for line in ledger.lines().skip(1) {
        // The first seven CSV columns are quoted. Cell names may contain
        // commas, so split only at quoted field boundaries.
        let fields = line.split("\",\"").collect::<Vec<_>>();
        assert!(fields.len() >= 7, "malformed ledger row: {line}");
        let old_id = fields[0].trim_start_matches('"');
        let new_id = fields[5];
        let status = fields[6].trim_end_matches('"');
        assert!(
            old_ids.insert(old_id.to_owned()),
            "duplicate old ID: {old_id}"
        );
        if old_id.starts_with("fixture::") {
            fixtures.insert(old_id.to_owned());
        }
        if old_id.starts_with("cell::") {
            cells.insert(old_id.to_owned());
        }
        assert_eq!(
            status, "migrated",
            "unmigrated identity in ledger: {old_id}"
        );
        assert!(!new_id.is_empty(), "missing destination: {old_id}");
        assert!(
            new_ids.insert(new_id.to_owned()),
            "duplicate destination: {new_id}"
        );
    }
    assert!(!cells.is_empty(), "matrix-cell inventory is empty");
    let configuration: serde_json::Value =
        serde_json::from_str(include_str!("../../scripts/test-inventory.json"))
            .expect("inventory configuration");
    let added = configuration["added_tests"]
        .as_array()
        .expect("added test identities")
        .iter()
        .map(|item| item.as_str().expect("added test identity").to_owned())
        .collect::<BTreeSet<_>>();
    assert!(
        added.is_subset(&old_ids),
        "added-test list has stale identities"
    );
    let mut baseline_hash = Sha256::new();
    for old_id in old_ids.difference(&added) {
        baseline_hash.update(old_id.as_bytes());
        baseline_hash.update(b"\n");
    }
    assert_eq!(
        format!("{:x}", baseline_hash.finalize()),
        S0_BASELINE_SHA256,
        "S0 baseline identity set changed"
    );
    super::migration_target::assert_matches_ledger("gates");
    let mut live_fixtures = BTreeSet::new();
    fixture_paths(&root.join("tests/grammar"), &mut live_fixtures, root);
    assert_eq!(fixtures, live_fixtures, "grammar fixture identity drift");
}
