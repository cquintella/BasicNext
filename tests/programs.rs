//! Programs and cross-backend parity tests.

#![allow(clippy::uninlined_format_args)]

#[path = "support/migration_target.rs"]
mod migration_target;
#[path = "support/mod.rs"]
pub(crate) mod support;

#[test]
fn migrated_programs_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("programs");
}

#[path = "programs/cli_snapshots.rs"]
mod cli_snapshots;
#[path = "programs/compiler_parity.rs"]
mod compiler_parity;
#[path = "programs/destructuring.rs"]
mod destructuring;
#[path = "programs/parity.rs"]
mod parity;
#[path = "programs/parity_regressions.rs"]
mod parity_regressions;
#[path = "programs/spec_0_6_5_s13.rs"]
mod spec_0_6_5_s13;
