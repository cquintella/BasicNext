#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::string_slice,
    clippy::let_underscore_must_use,
    clippy::allow_attributes_without_reason,
    unreachable_pub,
    reason = "DEC-55 tier 3"
)]

pub use bn_diag as diagnostic;
pub use bn_source as source;
pub use bn_types as types;

pub mod ast;
pub mod frontend_session;
mod host_spec;
pub use host_spec::{host_member_names, host_operation_names};
pub mod keyword_registry;
pub mod lexer;
pub mod module_graph;
pub mod parser;
pub mod prepare;
pub mod semantic;
pub mod session;
pub use session::{
    ClassifiedToken, ClassifiedTokenKind, PreparedSubmission, SessionAccumulator,
    SubmissionCompleteness, SubmissionKind, classify_submission,
};
pub mod token;

#[path = "lowering.rs"]
pub mod lowering;
