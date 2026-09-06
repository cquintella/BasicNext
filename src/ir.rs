//! Compatibility facade for the IR model and the frontend lowering boundary.

pub use bn_frontend::lowering::{lower, lower_graph, lower_graph_validated, lower_validated};
pub use bn_ir::{
    BasicBlock, BlockId, Constant, Function, Instruction, Module, ModuleId, SymbolId, Terminator,
    ValidatedModule, ValueId, validate, validate_module,
};
pub use bn_types::{FloatType, IntegerType, PointerLength, Type};
