//! In-process execution tests (HostEnv).

#![allow(
    clippy::uninlined_format_args,
    clippy::doc_markdown,
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

#[allow(dead_code)]
#[path = "support/prerequisites.rs"]
mod prerequisites;

#[path = "execute/support.rs"]
pub(crate) mod support;

#[test]
fn migrated_execute_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("execute");
}

#[path = "execute/bn_data.rs"]
mod bn_data;
#[path = "execute/bn_json.rs"]
mod bn_json;
#[path = "execute/bn_log.rs"]
mod bn_log;
#[path = "execute/bn_math.rs"]
mod bn_math;
#[path = "execute/bn_sqlite.rs"]
mod bn_sqlite;
#[path = "execute/bn_web.rs"]
mod bn_web;
#[path = "execute/host_dispatch.rs"]
mod host_dispatch;
#[path = "execute/host_env.rs"]
mod host_env;
#[path = "execute/host_exec.rs"]
mod host_exec;
#[path = "execute/host_fs.rs"]
mod host_fs;
#[path = "execute/host_net.rs"]
mod host_net;
#[path = "execute/language.rs"]
mod language;
#[path = "execute/strings.rs"]
mod strings;
#[path = "execute/values.rs"]
mod values;
