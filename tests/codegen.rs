//! Codegen tests and compiler capabilities.

#[path = "support/migration_target.rs"]
mod migration_target;
mod support;

#[test]
fn migrated_codegen_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("codegen");
}

#[path = "codegen/codegen_tests.rs"]
mod codegen_tests;
#[path = "codegen/compiler_capabilities.rs"]
mod compiler_capabilities;
