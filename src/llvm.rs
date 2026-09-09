//! Compatibility facade for the extracted LLVM backend.

pub use bn_llvm::{
    CompiledPolicy, Target, lower_module, lower_module_for_target,
    lower_validated_module_for_target, lower_validated_module_for_target_with_policy, validate_for,
};
