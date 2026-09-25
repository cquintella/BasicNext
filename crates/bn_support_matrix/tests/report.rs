use std::path::PathBuf;

use bn_support_matrix::{build_report, constraint_matches};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("support-matrix crate lives under crates/")
        .to_owned()
}

#[test]
fn report_matches_the_existing_inventory_contract() {
    let report = build_report(&repository_root()).expect("build support-matrix report");
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.targets, ["interpret", "llvm-native", "wasm32"]);
    assert_eq!(report.instruction_count, 32);
    assert_eq!(report.type_count, 27);
    assert_eq!(report.inventory_count, 2_592);
    assert_eq!(report.covered_count, 134);
    assert_eq!(report.gap_count, 2_458);
    assert_eq!(
        report.inventory_count,
        report.instruction_count * report.type_count * report.targets.len()
    );
    let print_pointer = report
        .inventory
        .iter()
        .find(|entry| {
            entry.instruction == "Print"
                && entry.r#type == "Pointer"
                && entry.target == "llvm-native"
        })
        .expect("Print/Pointer/llvm-native inventory row");
    assert!(print_pointer.evidence.is_empty());
}

#[test]
fn catalog_constraints_map_to_structural_type_variants() {
    for integer in [
        "INTEGER", "BYTE", "INT8", "INT16", "INT64", "UINT16", "UINT32", "UINT64",
    ] {
        assert!(constraint_matches(integer, "Integer"), "{integer}");
    }
    assert!(constraint_matches("FLOAT64 ABI", "Float"));
    assert!(constraint_matches("[INTEGER]", "Vector"));
    assert!(constraint_matches("TIMESTAMP", "Named"));
    assert!(!constraint_matches("STRING", "Integer"));
    assert!(!constraint_matches("INTEGER", "Unknown"));
}
