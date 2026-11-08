//! HOST conformance and ABI export tests.

#![allow(
    clippy::uninlined_format_args,
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
fn migrated_host_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("host");
}

#[path = "host/abi_exports.rs"]
mod abi_exports;
#[path = "host/host_conformance.rs"]
mod host_conformance;
