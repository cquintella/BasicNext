//! HOST conformance and ABI export tests.

#![allow(clippy::uninlined_format_args)]

#[path = "support/migration_target.rs"]
mod migration_target;
#[path = "support/mod.rs"]
pub(crate) mod support;

#[test]
fn migrated_host_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("host");
}

#[path = "host/abi_exports.rs"]
mod abi_exports;
#[path = "host/host_conformance.rs"]
mod host_conformance;
