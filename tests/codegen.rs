//! Codegen tests and compiler capabilities.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "integration tests and test helpers"
)]

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
