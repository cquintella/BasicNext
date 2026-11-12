//! Programs and cross-backend parity tests.

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
#[cfg(unix)]
#[path = "support/resident.rs"]
mod resident;
#[path = "support/mod.rs"]
pub(crate) mod support;

#[test]
fn migrated_programs_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("programs");
}

#[path = "programs/bnmath_angles.rs"]
mod bnmath_angles;
#[path = "programs/cli_snapshots.rs"]
mod cli_snapshots;
#[path = "programs/compiler_parity.rs"]
mod compiler_parity;
#[path = "programs/compound_assignment.rs"]
mod compound_assignment;
#[path = "programs/compress_module.rs"]
mod compress_module;
#[path = "programs/compress_policy.rs"]
mod compress_policy;
#[path = "programs/conformance.rs"]
mod conformance;
#[path = "programs/crypto_handle_release.rs"]
mod crypto_handle_release;
#[path = "programs/crypto_limits.rs"]
mod crypto_limits;
#[path = "programs/crypto_tour.rs"]
mod crypto_tour;
#[path = "programs/destructuring.rs"]
mod destructuring;
#[path = "programs/exec_signal_child.rs"]
mod exec_signal_child;
#[path = "programs/fs_directories_policy.rs"]
mod fs_directories_policy;
#[path = "programs/host_handle_release.rs"]
mod host_handle_release;
#[path = "programs/https_trust_roots.rs"]
mod https_trust_roots;
#[path = "programs/library_handle_release.rs"]
mod library_handle_release;
#[path = "programs/loop_scope.rs"]
mod loop_scope;
#[path = "programs/math_expressions.rs"]
mod math_expressions;
#[path = "programs/narrowing.rs"]
mod narrowing;
#[path = "programs/net_quota.rs"]
mod net_quota;
#[path = "programs/parity.rs"]
mod parity;
#[path = "programs/parity_regressions.rs"]
mod parity_regressions;
#[path = "programs/runtime_locks.rs"]
mod runtime_locks;
#[path = "programs/signed_zero.rs"]
mod signed_zero;
#[path = "programs/spec_0_6_5_s13.rs"]
mod spec_0_6_5_s13;
#[path = "programs/struct_vector_literals.rs"]
mod struct_vector_literals;
#[path = "programs/web_client_tour.rs"]
mod web_client_tour;
#[path = "programs/web_crypto_bytes.rs"]
mod web_crypto_bytes;
#[path = "programs/web_egress_policy_bounds.rs"]
mod web_egress_policy_bounds;
#[path = "programs/web_error_fields.rs"]
mod web_error_fields;
#[path = "programs/web_narrowed_handles.rs"]
mod web_narrowed_handles;
#[path = "programs/web_refused_connection.rs"]
mod web_refused_connection;
#[path = "programs/web_static_policy.rs"]
mod web_static_policy;
