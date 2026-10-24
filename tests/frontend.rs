//! Shared frontend and validated IR regression tests.

#[path = "support/migration_target.rs"]
mod migration_target;

#[test]
fn migrated_frontend_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("frontend");
}

#[path = "frontend/ir.rs"]
mod ir;
#[path = "frontend/ir_names.rs"]
mod ir_names;
#[path = "frontend/lexer.rs"]
mod lexer;
#[path = "frontend/module_graph.rs"]
mod module_graph;
#[path = "frontend/parser.rs"]
mod parser;
#[path = "frontend/semantic.rs"]
mod semantic;
#[path = "frontend/u1.rs"]
mod u1;
#[path = "frontend/validated_ir.rs"]
mod validated_ir;
