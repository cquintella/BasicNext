//! The IR model only. Lowering (frontend → IR) lives in [`crate::lowering`];
//! keeping it out of this module means a backend that imports `ir` cannot
//! re-lower from the AST (W3).

pub use bn_ir::names;
pub use bn_ir::{
    BasicBlock, BlockId, Constant, Function, Instruction, Module, ModuleId, SymbolId, Terminator,
    ValidatedModule, ValueId, validate, validate_module,
};
pub use bn_types::{FloatType, IntegerType, PointerLength, Type};
