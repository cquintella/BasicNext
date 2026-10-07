// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_diag::Diagnostic;
use bn_source::Span;
pub use bn_value::Handle;

/// The payloads of live allocations, keyed by the handle the shared ARC core
/// (`bn_rt::arc::ArcCore`) issued: the core decides identity, counts and
/// liveness for both backends; the heap only stores what a handle names.
#[derive(Debug)]
pub struct Heap<T> {
    /// Indexed by the handle's slot; each entry keeps the generation of the
    /// handle that owns it, so a stale handle never reads a reused slot.
    allocations: Vec<Option<(u32, Vec<T>)>>,
}

impl<T> Default for Heap<T> {
    fn default() -> Self {
        Self {
            allocations: Vec::new(),
        }
    }
}

impl<T: Clone> Heap<T> {
    /// Stores the payload of the allocation `handle` names: `length` copies
    /// of `initial` (a zero-length region is valid).
    ///
    /// # Errors
    ///
    /// Returns `ALLOCATION_TOO_LARGE` if the payload cannot be reserved.
    ///
    /// # Panics
    ///
    /// When `handle` already names a payload: the core never issues a live
    /// handle twice.
    pub fn insert(
        &mut self,
        handle: Handle,
        length: usize,
        initial: T,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let payload = allocation_payload(length, initial, span)?;
        let slot = handle.slot as usize;
        if self.allocations.len() <= slot {
            self.allocations.resize_with(slot + 1, || None);
        }
        assert!(
            self.allocations[slot].is_none(),
            "the ARC core issued a handle whose slot is still in use"
        );
        self.allocations[slot] = Some((handle.generation, payload));
        Ok(())
    }

    /// Frees the payload of `handle`; returns it, or `None` when the handle
    /// names no payload.
    pub fn remove(&mut self, handle: Handle) -> Option<Vec<T>> {
        let entry = self.allocations.get_mut(handle.slot as usize)?;
        if entry
            .as_ref()
            .is_some_and(|(generation, _)| *generation == handle.generation)
        {
            entry.take().map(|(_, payload)| payload)
        } else {
            None
        }
    }

    /// Reads one element through a checked handle.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles and out-of-bounds indices.
    pub fn get(&self, handle: Handle, index: usize, span: Span) -> Result<&T, Diagnostic> {
        let payload = self.payload(handle, span)?;
        payload
            .get(index)
            .ok_or_else(|| region_index_error(index, payload.len(), span))
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
        let payload = self
            .allocations
            .get_mut(handle.slot as usize)
            .and_then(Option::as_mut)
            .filter(|(generation, _)| *generation == handle.generation)
            .map(|(_, payload)| payload)
            .ok_or_else(|| stale(span))?;
        let length = payload.len();
        payload
            .get_mut(index)
            .ok_or_else(|| region_index_error(index, length, span))
    }

    /// Returns the number of elements in an allocation.
    ///
    /// # Errors
    ///
    /// Diagnoses stale handles.
    pub fn len(&self, handle: Handle, span: Span) -> Result<usize, Diagnostic> {
        Ok(self.payload(handle, span)?.len())
    }

    fn payload(&self, handle: Handle, span: Span) -> Result<&Vec<T>, Diagnostic> {
        self.allocations
            .get(handle.slot as usize)
            .and_then(Option::as_ref)
            .filter(|(generation, _)| *generation == handle.generation)
            .map(|(_, payload)| payload)
            .ok_or_else(|| stale(span))
    }
}

fn stale(span: Span) -> Diagnostic {
    heap_error(
        bn_diag::DiagId::USE_AFTER_RELEASE,
        "allocation handle refers to released memory",
        span,
    )
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

/// `INDEX_OUT_OF_BOUNDS` for a region access, with its typed facts (the
/// native trap builds the same arguments).
fn region_index_error(index: usize, length: usize, span: Span) -> Diagnostic {
    Diagnostic::structured(
        bn_diag::DiagId::INDEX_OUT_OF_BOUNDS,
        vec![
            ("index".into(), index.to_string().into()),
            ("bound".into(), length.to_string().into()),
            ("context".into(), "region".into()),
        ],
        vec![bn_diag::Label {
            span,
            style: bn_diag::LabelStyle::Primary,
            text: None,
        }],
    )
    .expect("index-out-of-bounds diagnostic schema")
}

fn heap_error(id: bn_diag::DiagId, message: impl Into<String>, span: Span) -> Diagnostic {
    let message = message.into();
    let arguments = {
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
