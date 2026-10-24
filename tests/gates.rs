//! Repository gates and their in-process regression checks.

#[path = "support/migration_target.rs"]
mod migration_target;
mod support;

#[path = "gates/catalog_keys.rs"]
mod catalog_keys;
#[path = "gates/dependency_gate.rs"]
mod dependency_gate;
#[path = "gates/migration_ledger.rs"]
mod migration_ledger;
#[path = "gates/shared_cores.rs"]
mod shared_cores;
#[path = "gates/shared_logic.rs"]
mod shared_logic;
#[path = "gates/source_size.rs"]
mod source_size;
#[path = "gates/suppressions.rs"]
mod suppressions;
#[path = "gates/test_support.rs"]
mod test_support;
#[path = "gates/text_literals.rs"]
mod text_literals;
