//! Values, ownership, and ARC tests.

#![allow(
    dead_code,
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
#[path = "support/mod.rs"]
pub(crate) mod support;

#[test]
fn migrated_values_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("values");
}

#[path = "values/alternatives_matrix.rs"]
mod alternatives_matrix;
#[path = "values/arc_differential.rs"]
mod arc_differential;
#[path = "values/heap.rs"]
mod heap;
#[path = "values/ownership_matrix.rs"]
mod ownership_matrix;
#[path = "values/value_matrix.rs"]
mod value_matrix;
