// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! ARC in the interpreter (proposal `arc-shared-core-0.6.5`): the lowering
//! decides every retain and release; the shared core (`bn_rt::arc`) counts;
//! this module only applies one operation to a value and runs a destruction.

#![allow(clippy::wildcard_imports)]
use super::*;
use bn_rt::arc::{ArcError, ObjectId, Site};

/// The core id an object or region handle carries.
pub(crate) const fn id_of(handle: Handle) -> ObjectId {
    ObjectId::from_parts(handle.slot, handle.generation)
}

/// The handle naming the core id `id`.
pub(crate) const fn handle_of(id: ObjectId) -> Handle {
    Handle::new(id.slot(), id.generation())
}

/// The source position of an operation, for the `BN_ARC_TRACE` lines (the
/// same `line:column` the native code passes).
fn site(span: Span) -> Site {
    Site {
        line: u32::try_from(span.start.line).unwrap_or(u32::MAX),
        column: u32::try_from(span.start.column).unwrap_or(u32::MAX),
    }
}

/// A broken ARC invariant: the validated IR retained or released an object
/// that is not alive.
fn arc_error(error: ArcError, span: Span) -> Diagnostic {
    runtime_error(bn_diag::DiagId::INVALID_IR, error.to_string(), span)
}

impl Executor<'_, '_> {
    /// Registers a new allocation in the core: one strong reference, held by
    /// the value `NEW` yields.
    pub fn register_allocation(&mut self, class: &str, span: Span) -> Handle {
        // The interpreter's handle is the id; it has no address to keep.
        handle_of(self.arc.register(class, 0, site(span)))
    }

    /// `Retain`: one more strong reference to every object and region
    /// `value` holds.
    pub fn retain_owned_value(&mut self, value: &Value, span: Span) -> Result<(), Diagnostic> {
        match value {
            Value::Object { handle, .. } | Value::Pointer { handle, .. } => self
                .arc
                .retain(id_of(*handle), site(span))
                .map(|_| ())
                .map_err(|error| arc_error(error, span)),
            Value::Vector(values) => {
                for value in values {
                    self.retain_owned_value(value, span)?;
                }
                Ok(())
            }
            Value::Record { record } => {
                for value in record.iter() {
                    self.retain_owned_value(value, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// `Release`: one strong reference fewer to every object and region
    /// `value` holds; the last one destroys it (0.6.md, "Zero strong →
    /// destructor"). A vector releases its elements in index order, a
    /// `STRUCT` its fields in reverse declaration order, as an object.
    pub fn release_owned_value(&mut self, value: Value, span: Span) -> Result<(), Diagnostic> {
        match value {
            Value::Object { handle, class } => {
                if self
                    .arc
                    .release(id_of(handle), site(span))
                    .map_err(|error| arc_error(error, span))?
                {
                    self.destroy_object(handle, &class, span)?;
                }
                Ok(())
            }
            Value::Pointer { handle, .. } => {
                if self
                    .arc
                    .release(id_of(handle), site(span))
                    .map_err(|error| arc_error(error, span))?
                {
                    // A region has no destructor: it is freed at once.
                    self.memory.remove(handle);
                    self.arc
                        .finish_destroy(id_of(handle))
                        .map_err(|error| arc_error(error, span))?;
                }
                Ok(())
            }
            Value::Vector(values) => {
                for value in values {
                    self.release_owned_value(value, span)?;
                }
                Ok(())
            }
            Value::Record { record } => {
                for value in record.into_fields().into_iter().rev() {
                    self.release_owned_value(value, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The destruction of an object whose last strong reference is gone:
    /// the destructor chain, then the field release the lowering generated
    /// (`FunctionKind::ReleaseFields`), then the allocation is freed. From
    /// the start, weak references read `NULL` (the core no longer reports the
    /// object alive).
    fn destroy_object(
        &mut self,
        handle: Handle,
        class: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let target = Value::Object {
            handle,
            class: shared_string(class),
        };
        let mut destroyed = Ok(());
        for kind in [
            bn_ir::FunctionKind::Destructor,
            bn_ir::FunctionKind::ReleaseFields,
        ] {
            if let Some(function) = self.module.function_of_kind(kind, class) {
                let name = function.name.clone();
                let result = self
                    .call_named(&name, vec![target.clone()], span)
                    .map(|_| ());
                destroyed = destroyed.and(result);
            }
            if kind == bn_ir::FunctionKind::Destructor {
                self.notify_object_destroyed(handle);
            }
        }
        self.objects.remove(handle);
        self.arc
            .finish_destroy(id_of(handle))
            .map_err(|error| arc_error(error, span))?;
        destroyed
    }

    /// The element at `indices` of `target`, which a write replaces (its
    /// `previous` content).
    pub fn element_at(
        &self,
        target: &Value,
        indices: &[i128],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        match (target, indices) {
            (Value::Null, _) => Ok(Value::Null),
            (Value::Pointer { .. }, [index]) => self.index_value(target, *index, span),
            _ => Ok(super::part10::indexed_value(target, indices, span)?.clone()),
        }
    }

    /// The value at the end of the field path `fields` from `target` (the
    /// field a `SetField` replaces).
    pub fn field_at(
        &self,
        target: &Value,
        fields: &[bn_ir::FieldRef],
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let mut current = target.clone();
        for field in fields {
            let slot = field.slot.value() as usize;
            current = match &current {
                Value::Record { record } => record.get(slot).cloned(),
                Value::Object { handle, .. } => self
                    .objects
                    .get(*handle, 0, span)?
                    .fields
                    .get(slot)
                    .cloned(),
                _ => None,
            }
            .ok_or_else(|| {
                runtime_error(
                    bn_diag::DiagId::INVALID_IR,
                    "field path slot is absent",
                    span,
                )
            })?;
        }
        Ok(current)
    }

    /// The value a weak binding or a weak field reads: `NULL` once its
    /// object is gone.
    pub fn weak_read(&self, value: Value) -> Value {
        match value {
            Value::Object { handle, .. } if !self.arc.alive(id_of(handle)) => Value::Null,
            value => value,
        }
    }
}
