//! Rejection checks for incomplete test-migration inventories using the real tools.

use std::{fs, process::Command, time::Duration};

use serde_json::{Value, json};

use crate::support::{TestDir, run, workspace_root};

const INVENTORY_TIMEOUT: Duration = Duration::from_secs(30);
const CELLS_PER_DOMAIN: usize = 2;

fn configuration() -> Value {
    serde_json::from_str(include_str!("../../scripts/test-inventory.json"))
        .expect("inventory configuration is JSON")
}

fn complete_cells() -> Vec<Value> {
    let configuration = configuration();
    let mut domains = configuration["matrices"]
        .as_array()
        .expect("matrix definitions")
        .iter()
        .map(|matrix| matrix["target"].as_str().expect("matrix target").to_owned())
        .chain(std::iter::once("arc_differential".to_owned()))
        .collect::<Vec<_>>();
    domains.sort();
    let mut rows = Vec::new();
    for domain in &domains {
        for index in 0..CELLS_PER_DOMAIN {
            rows.push(json!({
                "old_id": format!("cell::bn::test/{domain}::case{index}"),
                "domain": domain,
            }));
        }
    }
    let counts = domains
        .iter()
        .map(|domain| json!({"domain": domain, "count": CELLS_PER_DOMAIN}))
        .collect::<Vec<_>>();
    rows.push(json!({"kind": "complete", "counts": counts}));
    rows
}

fn assert_rejected(rows: &[Value], exit_code_key: &str) {
    let directory = TestDir::new("migration-inventory").expect("inventory directory");
    let manifest = directory.join("artifacts.jsonl");
    let cells = directory.join("cells.jsonl");
    let ledger = directory.join("ledger.csv");
    fs::write(
        &manifest,
        json!({"reason": "build-finished", "success": true}).to_string(),
    )
    .expect("write completed manifest with missing artifacts");
    let inventory = rows
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&cells, inventory).expect("write inventory input");
    let mut result = run(
        Command::new("bash")
            .env(
                "PATH",
                std::env::var_os("PATH").expect("test tools require PATH"),
            )
            .current_dir(workspace_root())
            .arg(workspace_root().join("scripts/test-inventory.sh"))
            .arg(&manifest)
            .arg(&cells)
            .arg(&ledger),
        None,
        INVENTORY_TIMEOUT,
    )
    .expect("run real inventory checker");
    let expected = configuration()["exit_codes"][exit_code_key]
        .as_i64()
        .expect("inventory rejection exit code");
    assert!(!result.timed_out);
    assert_eq!(
        result.status.code().map(i64::from),
        Some(expected),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(!ledger.exists(), "rejected input must not publish a ledger");
    result.accept_expected_status();
}

#[test]
fn completed_manifest_with_missing_targets_is_rejected() {
    assert_rejected(&complete_cells(), "invalid_targets");
}

#[test]
fn truncated_cells_without_completion_are_rejected() {
    let mut rows = complete_cells();
    rows.pop();
    assert_rejected(&rows, "invalid_cells");
}

#[test]
fn removed_cell_with_retained_completion_is_rejected() {
    let mut rows = complete_cells();
    rows.remove(0);
    assert_rejected(&rows, "invalid_cells");
}

#[test]
fn duplicate_identity_with_unchanged_domain_counts_is_rejected() {
    let mut rows = complete_cells();
    rows[1]["old_id"] = rows[0]["old_id"].clone();
    assert_rejected(&rows, "invalid_cells");
}
