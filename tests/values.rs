//! Values, ownership, and ARC tests.

#![allow(dead_code)]

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
