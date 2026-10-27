//! Shared frontend and validated IR regression tests.

#[path = "support/migration_target.rs"]
mod migration_target;

#[test]
fn migrated_frontend_tests_match_the_ledger() {
    migration_target::assert_matches_ledger("frontend");
}

#[path = "frontend/compound_assignment.rs"]
mod compound_assignment;
#[path = "frontend/inexact_integer_literal.rs"]
mod inexact_integer_literal;
#[path = "frontend/ir.rs"]
mod ir;
#[path = "frontend/ir_names.rs"]
mod ir_names;
#[path = "frontend/lexer.rs"]
mod lexer;
#[path = "frontend/loop_scope.rs"]
mod loop_scope;
#[path = "frontend/module_graph.rs"]
mod module_graph;
#[path = "frontend/narrowing_scope.rs"]
mod narrowing_scope;
#[path = "frontend/parser.rs"]
mod parser;
#[path = "frontend/precedence.rs"]
mod precedence;
#[path = "frontend/semantic.rs"]
mod semantic;
#[path = "frontend/u1.rs"]
mod u1;
#[path = "frontend/validated_ir.rs"]
mod validated_ir;
