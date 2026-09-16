// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_source::Span;
pub use bn_value::Handle;

#[derive(Debug)]
struct Allocation<T> {
    generation: u32,
    declared_type: String,
    payload: Vec<T>,
    live: bool,
    destroying: bool,
    strong_count: usize,
}

#[derive(Debug)]
pub struct Heap<T> {
    allocations: Vec<Allocation<T>>,
}

impl<T> Default for Heap<T> {
    fn default() -> Self {
        Self {
            allocations: Vec::new(),
        }
    }
}

impl<T: Clone> Heap<T> {
    /// Applies a mutation to every live allocation. Used by ARC bookkeeping
    /// to invalidate weak references when an object is destroyed.
    pub fn for_each_live_mut(&mut self, mut f: impl FnMut(&mut T)) {
        for allocation in &mut self.allocations {
            if allocation.live || allocation.destroying {
                for value in &mut allocation.payload {
                    f(value);
                }
            }
        }
    }

    /// Creates a live checked allocation, including valid zero-length regions.
    ///
    /// # Errors
    ///
    /// Returns `ALLOCATION_TOO_LARGE` if the slot index cannot be represented
    /// by a BN handle or the payload cannot be reserved.
    pub fn allocate(
        &mut self,
        declared_type: impl Into<String>,
        length: usize,
        initial: T,
        span: Span,
    ) -> Result<Handle, Diagnostic> {
        self.allocate_region(declared_type, length, initial, span)
    }

    /// Reads one element through a checked handle.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles and out-of-bounds indices.
    pub fn get(&self, handle: Handle, index: usize, span: Span) -> Result<&T, Diagnostic> {
        let allocation = self.live(handle, span)?;
        allocation.payload.get(index).ok_or_else(|| {
            heap_error(
                bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
                format!(
                    "index {index} is outside {} region length {}",
                    allocation.declared_type,
                    allocation.payload.len()
                ),
                span,
            )
        })
    }

    /// Mutably accesses one element through a checked handle.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles and out-of-bounds indices.
    pub fn get_mut(
        &mut self,
        handle: Handle,
        index: usize,
        span: Span,
    ) -> Result<&mut T, Diagnostic> {
        let allocation = self.live_mut(handle, span)?;
        let length = allocation.payload.len();
        allocation.payload.get_mut(index).ok_or_else(|| {
            heap_error(
                bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
                format!("index {index} is outside region length {length}"),
                span,
            )
        })
    }

    /// Returns the number of live elements in an allocation.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles.
    pub fn len(&self, handle: Handle, span: Span) -> Result<usize, Diagnostic> {
        Ok(self.live(handle, span)?.payload.len())
    }

    /// Deletes one live BN-owned allocation.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles and repeated deletion.
    pub fn delete(&mut self, handle: Handle, span: Span) -> Result<(), Diagnostic> {
        self.begin_delete(handle, span)?;
        self.finish_delete(handle, span)
    }

    /// Marks an allocation deleted so a reentrant `DELETE` is `DOUBLE_RELEASE`
    /// while a destructor may still read the payload.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles and repeated deletion.
    pub fn begin_delete(&mut self, handle: Handle, span: Span) -> Result<(), Diagnostic> {
        let allocation = self.slot_mut(handle, span)?;
        if allocation.generation != handle.generation {
            return Err(heap_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "allocation handle is stale",
                span,
            ));
        }
        if !allocation.live || allocation.destroying {
            return Err(heap_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                "allocation was already deleted",
                span,
            ));
        }
        allocation.live = false;
        allocation.destroying = true;
        Ok(())
    }

    /// Clears a payload after its destructor has finished.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles.
    pub fn finish_delete(&mut self, handle: Handle, span: Span) -> Result<(), Diagnostic> {
        let allocation = self.slot_mut(handle, span)?;
        if allocation.generation != handle.generation {
            return Err(heap_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "allocation handle is stale",
                span,
            ));
        }
        allocation.payload.clear();
        allocation.destroying = false;
        allocation.live = false;
        Ok(())
    }

    /// Increments the strong-reference count for a live allocation.
    ///
    /// # Errors
    ///
    /// Returns a stale-handle or retain-overflow diagnostic.
    pub fn retain(&mut self, handle: Handle, span: Span) -> Result<(), Diagnostic> {
        let allocation = self.live_mut(handle, span)?;
        allocation.strong_count = allocation.strong_count.checked_add(1).ok_or_else(|| {
            heap_error(
                bn_diag::DiagId::RETAIN_OVERFLOW,
                "strong-reference count overflowed",
                span,
            )
        })?;
        Ok(())
    }

    /// Decrements the strong-reference count and reports whether it reached zero.
    ///
    /// # Errors
    ///
    /// Returns a stale-handle or double-release diagnostic.
    pub fn release(&mut self, handle: Handle, span: Span) -> Result<bool, Diagnostic> {
        let allocation = self.live_mut(handle, span)?;
        if allocation.strong_count == 0 {
            return Err(heap_error(
                bn_diag::DiagId::DOUBLE_RELEASE,
                "allocation was already released",
                span,
            ));
        }
        allocation.strong_count -= 1;
        Ok(allocation.strong_count == 0)
    }

    /// Returns the current strong-reference count.
    ///
    /// # Errors
    ///
    /// Returns a stale-handle diagnostic.
    pub fn strong_count(&self, handle: Handle, span: Span) -> Result<usize, Diagnostic> {
        Ok(self.live(handle, span)?.strong_count)
    }

    /// Reports whether a handle still names its live allocation.
    #[must_use]
    pub fn is_live(&self, handle: Handle) -> bool {
        self.allocations
            .get(handle.slot as usize)
            .is_some_and(|allocation| allocation.generation == handle.generation && allocation.live)
    }

    fn allocate_region(
        &mut self,
        declared_type: impl Into<String>,
        length: usize,
        initial: T,
        span: Span,
    ) -> Result<Handle, Diagnostic> {
        let declared_type = declared_type.into();
        let payload = allocation_payload(length, initial, span)?;
        if let Some((slot, allocation)) =
            self.allocations
                .iter_mut()
                .enumerate()
                .find(|(_, allocation)| {
                    !allocation.live && !allocation.destroying && allocation.generation < u32::MAX
                })
        {
            allocation.generation += 1;
            allocation.declared_type = declared_type;
            allocation.payload = payload;
            allocation.live = true;
            allocation.destroying = false;
            allocation.strong_count = 1;
            return Ok(Handle {
                slot: u32::try_from(slot).map_err(|_| too_large(span))?,
                generation: allocation.generation,
            });
        }
        let slot = u32::try_from(self.allocations.len()).map_err(|_| too_large(span))?;
        self.allocations.push(Allocation {
            generation: 0,
            declared_type,
            payload,
            live: true,
            destroying: false,
            strong_count: 1,
        });
        Ok(Handle {
            slot,
            generation: 0,
        })
    }

    fn live(&self, handle: Handle, span: Span) -> Result<&Allocation<T>, Diagnostic> {
        let allocation = self.allocations.get(handle.slot as usize).ok_or_else(|| {
            heap_error(
                bn_diag::DiagId::USE_AFTER_RELEASE,
                "allocation handle is stale",
                span,
            )
        })?;
        validate_live(allocation, handle, span)?;
        Ok(allocation)
    }

    fn live_mut(&mut self, handle: Handle, span: Span) -> Result<&mut Allocation<T>, Diagnostic> {
        let allocation = self.slot_mut(handle, span)?;
        validate_live(allocation, handle, span)?;
        Ok(allocation)
    }

    fn slot_mut(&mut self, handle: Handle, span: Span) -> Result<&mut Allocation<T>, Diagnostic> {
        self.allocations
            .get_mut(handle.slot as usize)
            .ok_or_else(|| {
                heap_error(
                    bn_diag::DiagId::USE_AFTER_RELEASE,
                    "allocation handle is stale",
                    span,
                )
            })
    }
}

fn allocation_payload<T: Clone>(
    length: usize,
    initial: T,
    span: Span,
) -> Result<Vec<T>, Diagnostic> {
    let mut payload = Vec::new();
    payload.try_reserve_exact(length).map_err(|_| {
        heap_error(
            bn_diag::DiagId::ALLOCATION_TOO_LARGE,
            "allocation payload cannot be reserved",
            span,
        )
    })?;
    payload.resize(length, initial);
    Ok(payload)
}

fn validate_live<T>(
    allocation: &Allocation<T>,
    handle: Handle,
    span: Span,
) -> Result<(), Diagnostic> {
    if allocation.generation != handle.generation {
        Err(heap_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "allocation handle is stale",
            span,
        ))
    } else if !allocation.live && !allocation.destroying {
        Err(heap_error(
            bn_diag::DiagId::USE_AFTER_RELEASE,
            "allocation handle refers to deleted memory",
            span,
        ))
    } else {
        Ok(())
    }
}

fn too_large(span: Span) -> Diagnostic {
    heap_error(
        bn_diag::DiagId::ALLOCATION_TOO_LARGE,
        "allocation table exceeds the portable handle limit",
        span,
    )
}

fn heap_error(id: bn_diag::DiagId, message: impl Into<String>, span: Span) -> Diagnostic {
    let message = message.into();
    let arguments = if id == bn_diag::DiagId::INDEX_OUT_OF_BOUNDS {
        vec![
            (
                "index".into(),
                bn_diag::DiagnosticValue::Text("unknown".into()),
            ),
            (
                "bound".into(),
                bn_diag::DiagnosticValue::Text("unknown".into()),
            ),
            ("context".into(), bn_diag::DiagnosticValue::Text(message)),
        ]
    } else {
        match id.argument_schema() {
            [only] => vec![(only.name.into(), bn_diag::DiagnosticValue::Text(message))],
            schema => unreachable!(
                "{} needs an explicit argument mapping ({} arguments)",
                id.code(),
                schema.len()
            ),
        }
    };
    Diagnostic::structured(
        id,
        arguments,
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("runtime compatibility diagnostic schema")
}
