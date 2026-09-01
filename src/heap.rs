// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::{diagnostic::Diagnostic, source::Span};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Handle {
    slot: u32,
    generation: u32,
}

#[derive(Debug)]
struct Allocation<T> {
    generation: u32,
    declared_type: String,
    payload: Vec<T>,
    live: bool,
    destroying: bool,
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
                "INDEX_OUT_OF_BOUNDS",
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
                "INDEX_OUT_OF_BOUNDS",
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

    /// Marks an allocation deleted so a reentrant `DELETE` is `DOUBLE_DELETE`
    /// while a destructor may still read the payload.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles and repeated deletion.
    pub fn begin_delete(&mut self, handle: Handle, span: Span) -> Result<(), Diagnostic> {
        let allocation = self.slot_mut(handle, span)?;
        if allocation.generation != handle.generation {
            return Err(heap_error(
                "USE_AFTER_DELETE",
                "allocation handle is stale",
                span,
            ));
        }
        if !allocation.live || allocation.destroying {
            return Err(heap_error(
                "DOUBLE_DELETE",
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
                "USE_AFTER_DELETE",
                "allocation handle is stale",
                span,
            ));
        }
        allocation.payload.clear();
        allocation.destroying = false;
        allocation.live = false;
        Ok(())
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
        });
        Ok(Handle {
            slot,
            generation: 0,
        })
    }

    fn live(&self, handle: Handle, span: Span) -> Result<&Allocation<T>, Diagnostic> {
        let allocation = self
            .allocations
            .get(handle.slot as usize)
            .ok_or_else(|| heap_error("USE_AFTER_DELETE", "allocation handle is stale", span))?;
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
            .ok_or_else(|| heap_error("USE_AFTER_DELETE", "allocation handle is stale", span))
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
            "ALLOCATION_TOO_LARGE",
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
            "USE_AFTER_DELETE",
            "allocation handle is stale",
            span,
        ))
    } else if !allocation.live && !allocation.destroying {
        Err(heap_error(
            "USE_AFTER_DELETE",
            "allocation handle refers to deleted memory",
            span,
        ))
    } else {
        Ok(())
    }
}

fn too_large(span: Span) -> Diagnostic {
    heap_error(
        "ALLOCATION_TOO_LARGE",
        "allocation table exceeds the portable handle limit",
        span,
    )
}

fn heap_error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}
