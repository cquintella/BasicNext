//! Repository gates and their in-process regression checks.

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

#[path = "gates/bni_performance.rs"]
mod bni_performance;
#[path = "gates/callee_names.rs"]
mod callee_names;
#[path = "gates/catalog_keys.rs"]
mod catalog_keys;
#[path = "gates/crypto_surface.rs"]
mod crypto_surface;
#[path = "gates/dependency_gate.rs"]
mod dependency_gate;
#[path = "gates/migration_ledger.rs"]
mod migration_ledger;
#[path = "gates/no_python.rs"]
mod no_python;
#[path = "gates/pre_commit.rs"]
mod pre_commit;
#[path = "gates/public_references.rs"]
mod public_references;
#[path = "gates/shared_cores.rs"]
mod shared_cores;
#[path = "gates/shared_logic.rs"]
mod shared_logic;
#[path = "gates/source_size.rs"]
mod source_size;
#[path = "gates/suppressions.rs"]
mod suppressions;
#[path = "gates/test_prereqs.rs"]
mod test_prereqs;
#[path = "gates/test_support.rs"]
mod test_support;
#[path = "gates/text_literals.rs"]
mod text_literals;
#[path = "gates/web_failures.rs"]
mod web_failures;
#[path = "gates/web_surface.rs"]
mod web_surface;
