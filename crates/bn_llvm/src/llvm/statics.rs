// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `STATIC` storage of a native program: the field globals, the class
//! initialization flags, and the reset that gives each `ASYNC` task its own
//! copy (known issue K44; `0.6.md`, "Isolation and conformance").
//!
//! Every global here is `thread_local`, so concurrent tasks never share a
//! field. Queues reuse worker threads, so the trampoline also calls the
//! reset before each task, as `bni` starts each task on a fresh module copy.
//! The reset, not the initializer, stores a value that holds the address of
//! another global (vector rows, a `STRUCT` block): a thread-local address is
//! per thread, not a link-time constant.
#![allow(clippy::wildcard_imports)]
use super::*;
use crate::ir::{LlvmFunction, LlvmGlobal, LlvmInst as I, LlvmOperand as O, LlvmType as T};
use std::collections::BTreeMap;

pub(crate) const STATICS_RESET: &str = "bn_statics_reset";

/// Each `STATIC` global (name without `@`), its type, and its value before
/// `Start`, ordered by field so the emitted text is deterministic.
pub(crate) struct StaticState(Vec<(String, T, O)>);

impl StaticState {
    /// One scan of every function of `module`, shared by the preamble (which
    /// defines the reset) and `Start` (which calls it).
    pub(crate) fn collect(module: &Module) -> Self {
        let mut fields = BTreeMap::new();
        for instruction in module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.instructions)
        {
            match instruction {
                Instruction::LoadStatic {
                    class, field, ty, ..
                }
                | Instruction::StoreStatic {
                    class, field, ty, ..
                } => {
                    if let Some(globals) = field_globals(module, class, field, ty) {
                        fields.insert((class.clone(), field.clone()), globals);
                    }
                }
                Instruction::EnsureClass { class, .. } => {
                    let flag = class_init_flag(class).trim_start_matches('@').to_owned();
                    let globals = vec![(flag, T::I1, O::zero_initializer())];
                    fields.insert((class.clone(), String::new()), globals);
                }
                _ => {}
            }
        }
        Self(fields.into_values().flatten().collect())
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Defines the zeroed thread-local globals and `bn_statics_reset`.
    pub(crate) fn emit(&self, text: &mut crate::ir::LlvmModule) {
        if self.is_empty() {
            return;
        }
        let mut reset = LlvmFunction::new_definition(STATICS_RESET, T::Void, Vec::new());
        reset.set_linkage("private");
        let entry = reset.add_block("entry");
        for (name, ty, initial) in &self.0 {
            let global = LlvmGlobal::variable(name, ty.clone(), O::zero_initializer());
            text.global(
                global
                    .with_linkage(crate::ir::Linkage::Internal)
                    .thread_local(),
            );
            entry.push(I::store(ty.clone(), initial.clone(), O::global(name)));
        }
        entry.terminate(I::Ret { val: None });
        text.blank();
        text.function(reset);
    }
}

pub(crate) fn reset_call() -> I {
    I::call(T::Void, STATICS_RESET, Vec::new())
}

/// The globals of one field: its value `bn_st_…`; a fixed vector adds its
/// elements `bn_sv_…` and the copy of replaced elements `bn_svp_…`; a
/// `STRUCT` adds its block `bn_sv_…`. `None` for a type with no native
/// storage, which `validate_for` already refused.
fn field_globals(
    module: &Module,
    class: &str,
    field: &str,
    ty: &Type,
) -> Option<Vec<(String, T, O)>> {
    let value_ty = llvm_type(ty)?;
    let slot = static_global_name(class, field)
        .trim_start_matches('@')
        .to_owned();
    let suffix = slot.trim_start_matches("bn_st_").to_owned();
    let (storage, kept) = (format!("bn_sv_{suffix}"), format!("bn_svp_{suffix}"));
    let zero = O::zero_initializer;
    if let Some((array, count)) = vectors::fixed_vector_array(ty) {
        let rows = vectors::nest(ty).map_or_else(zero, |nest| nest.linked_constant(&storage));
        let fat = O::ConstStruct(vec![
            (T::Ptr, O::global(&storage)),
            (T::I32, O::uint(count)),
        ]);
        return Some(vec![
            (storage, array.clone(), rows),
            (kept, array, zero()),
            (slot, crate::layout::vector_ty(), fat),
        ]);
    }
    if let Some(owner) = struct_owner(module, ty) {
        // A block too large for the host's `usize` gets no global, so the
        // build stops at the undefined symbol instead of storing less.
        let bytes = usize::try_from(class_instance_bytes(module, &owner)).ok()?;
        let block = O::global(&storage);
        return Some(vec![
            (storage, T::Array(bytes, Box::new(T::I8)), zero()),
            (slot, T::Ptr, block),
        ]);
    }
    Some(vec![(slot, value_ty, zero())])
}
