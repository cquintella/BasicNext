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

fn scan_crate_tests_in_dir(
    directory: &Path,
    crate_name: &str,
    found: &mut BTreeSet<(String, String)>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.is_dir() {
            scan_crate_tests_in_dir(&path, crate_name, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let mut pending_test = false;
            let mut paren_bracket_depth: usize = 0;
            for line in content.lines() {
                let mut trimmed = line.trim();
                if let Some(after) = trimmed.strip_prefix("#[test]") {
                    pending_test = true;
                    trimmed = after.trim();
                } else if trimmed.contains("#[test]") {
                    pending_test = true;
                }
                if pending_test {
                    for c in trimmed.chars() {
                        if c == '[' || c == '(' {
                            paren_bracket_depth += 1;
                        } else if c == ']' || c == ')' {
                            paren_bracket_depth = paren_bracket_depth.saturating_sub(1);
                        }
                    }
                    if paren_bracket_depth > 0
                        || trimmed.starts_with('#')
                        || trimmed.starts_with(']')
                        || trimmed.starts_with(')')
                        || trimmed.ends_with(']')
                    {
                        continue;
                    }
                    if let Some(rest) = trimmed
                        .strip_prefix("fn ")
                        .or_else(|| trimmed.strip_prefix("async fn "))
                        .or_else(|| trimmed.strip_prefix("pub fn "))
                        .or_else(|| trimmed.strip_prefix("pub(crate) fn "))
                    {
                        let fn_name = rest
                            .split(|c: char| !c.is_alphanumeric() && c != '_')
                            .next()
                            .unwrap_or_default();
                        if !fn_name.is_empty() && fn_name != "refuse" {
                            found.insert((crate_name.to_owned(), fn_name.to_owned()));
                        }
                        pending_test = false;
                    } else if !trimmed.is_empty()
                        && !trimmed.starts_with("//")
                        && !trimmed.starts_with("/*")
                        && !trimmed.starts_with('*')
                    {
                        pending_test = false;
                    }
                }
            }
        }
    }
}

#[test]
fn crate_tests_match_the_ledger() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ledger = fs::read_to_string(root.join(LEDGER)).expect("migration ledger");
    let mut ledger_crate_tests = BTreeSet::new();
    for line in ledger.lines().skip(1) {
        let fields = line.split("\",\"").collect::<Vec<_>>();
        assert!(fields.len() >= 7, "malformed ledger row: {line}");
        let harness = fields[2];
        if harness != "crate" {
            continue;
        }
        let new_id = fields[5];
        let parts = new_id.split("::").collect::<Vec<_>>();
        assert!(parts.len() >= 2, "malformed crate test new_id: {new_id}");
        let package = parts[0];
        let fn_name = parts[parts.len() - 1].trim_end_matches('"');
        ledger_crate_tests.insert((package.to_owned(), fn_name.to_owned()));
    }

    let mut source_crate_tests = BTreeSet::new();
    for root_dir in ["crates", "libraries"] {
        let Ok(entries) = fs::read_dir(root.join(root_dir)) else {
            continue;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };
            let path = entry.path();
            if path.is_dir() {
                let crate_name = path
                    .file_name()
                    .expect("crate directory name")
                    .to_string_lossy()
                    .to_string();
                scan_crate_tests_in_dir(&path, &crate_name, &mut source_crate_tests);
            }
        }
    }

    let unmapped = source_crate_tests
        .difference(&ledger_crate_tests)
        .collect::<Vec<_>>();
    assert!(
        unmapped.is_empty(),
        "unmapped crate tests found in source: {unmapped:?}"
    );

    let stale = ledger_crate_tests
        .difference(&source_crate_tests)
        .collect::<Vec<_>>();
    assert!(
        stale.is_empty(),
        "stale crate test destinations in ledger not in source: {stale:?}"
    );
}
