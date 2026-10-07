// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The ARC verifier (bucket typed-llvm-emitter, Sprint 8, phase A): when a
//! program ends, every object still alive must have exactly as many strong
//! references as the program still holds — in `STATIC`s, in the strong
//! fields of live objects, and in the elements of live regions. More is a
//! leak the toolchain caused; fewer would have been a use after release.
//! A strong cycle without `WEAK` keeps its counts equal to its references,
//! so it passes: it is a leak of the program, not of the toolchain.

#![allow(clippy::wildcard_imports)]
use super::ownership::{handle_of, id_of};
use super::*;

impl Executor<'_, '_> {
    /// Checks the counts of every live object against the references the
    /// program still holds; `INVALID_IR` names the first mismatch.
    pub(crate) fn verify_arc(&self, span: Span) -> Result<(), Diagnostic> {
        let mut held = HashMap::<u64, u64>::new();
        for value in self.statics.values() {
            count_references(value, &mut held);
        }
        let live = self.arc.snapshot();
        for info in &live {
            let handle = handle_of(info.id);
            if let Ok(instance) = self.objects.get(handle, 0, span) {
                let layout = self.module.field_layouts.get(&info.class);
                for (slot, field) in instance.fields.iter().enumerate() {
                    let weak = layout
                        .and_then(|layout| layout.fields.get(slot))
                        .is_some_and(|entry| entry.weak);
                    if !weak {
                        count_references(field, &mut held);
                    }
                }
            } else if let Ok(length) = self.memory.len(handle, span) {
                for index in 0..length {
                    count_references(self.memory.get(handle, index, span)?, &mut held);
                }
            }
        }
        for info in live {
            // A library class (`BNWeb`, `BNSqlite`) is held by its provider,
            // which the program cannot see.
            let library_object = self.objects.get(handle_of(info.id), 0, span).is_ok()
                && !self.module.field_layouts.contains_key(&info.class);
            let references = held.get(&info.id.bits()).copied().unwrap_or(0);
            if !library_object && references != info.strong {
                return Err(runtime_error(
                    bn_diag::DiagId::INVALID_IR,
                    format!(
                        "ARC verifier: {}{} has strong count {} but the program holds {references} strong reference(s)",
                        info.class, info.id, info.strong
                    ),
                    span,
                ));
            }
        }
        Ok(())
    }
}

/// Adds one for each object or region `value` holds.
fn count_references(value: &Value, held: &mut HashMap<u64, u64>) {
    match value {
        Value::Object { handle, .. } | Value::Pointer { handle } => {
            *held.entry(id_of(*handle).bits()).or_insert(0) += 1;
        }
        Value::Vector(values) => {
            for value in values {
                count_references(value, held);
            }
        }
        Value::Record { record } => {
            for value in record.iter() {
                count_references(value, held);
            }
        }
        _ => {}
    }
}
