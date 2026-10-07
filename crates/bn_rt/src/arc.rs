// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The one ARC mechanism of Basic Next (proposal `arc-shared-core-0.6.5`,
//! accepted 2026-10-06): strong counts and liveness for every object, shared
//! by `bni` (this Rust API, one core per interpreter) and `bnc` (the C ABI in
//! `arc_abi`, one core per process). The rules — when to retain and release —
//! live in the validated IR; this core only counts.
//!
//! An object is named by an [`ObjectId`], the slot index and the slot's
//! generation in one `u64`. A weak binding keeps that id: once the object is
//! gone, or its slot holds a newer object (a later generation), the id is no
//! longer [`ArcCore::alive`], so the weak reads `NULL`. The core never reads
//! or writes object memory.

use std::fmt;
use std::io::Write as _;

/// An object's identity: slot index (high 32 bits) and generation (low 32
/// bits). Zero is never an id, so a zeroed native slot holds no object.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObjectId(u64);

impl ObjectId {
    /// The id of slot `index` at `generation` (the interpreter keeps the two
    /// halves in its object handle).
    #[must_use]
    pub const fn from_parts(index: u32, generation: u32) -> Self {
        Self(((index as u64) << 32) | generation as u64)
    }

    /// The slot index, the high half.
    #[must_use]
    pub const fn slot(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// The id as one `u64`, for the C ABI and for weak bindings.
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }

    /// The id a `u64` carries, as given by [`ObjectId::bits`].
    #[must_use]
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    fn index(self) -> usize {
        usize::try_from(self.0 >> 32).expect("32-bit slot index fits usize")
    }

    /// The slot's generation, the low half.
    #[must_use]
    pub const fn generation(self) -> u32 {
        // Truncation keeps the low 32 bits, the generation.
        #[allow(clippy::cast_possible_truncation)]
        let generation = self.0 as u32;
        generation
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.index())
    }
}

/// A broken ARC invariant: the lowering emitted a retain or release for an
/// object that is not alive. Never a program error the user can cause.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArcError {
    /// The id names no live object (already destroyed, or never registered).
    NotAlive(ObjectId),
}

impl fmt::Display for ArcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAlive(id) => write!(f, "ARC operation on object {id}, which is not alive"),
        }
    }
}

/// One live object, as debugging tools show it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectInfo {
    pub id: ObjectId,
    pub class: String,
    pub strong: u64,
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    /// Zero when the slot holds no live object.
    strong: u64,
    /// The last strong reference is gone and the destructor chain runs: the
    /// count is frozen (a retain or release from the destructor does
    /// nothing) and weak bindings already read `NULL`, until
    /// [`ArcCore::finish_destroy`].
    destroying: bool,
    class: String,
    /// The object's address in `bnc` (zero in `bni`), an opaque number the
    /// core never dereferences: a weak binding keeps only the id and asks
    /// [`ArcCore::address`] for the object.
    address: u64,
}

/// The source position of an ownership operation (its IR span), for the
/// `BN_ARC_TRACE` lines: `line:column`, which both backends know.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Site {
    pub line: u32,
    pub column: u32,
}

impl fmt::Display for Site {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// Strong counts and liveness for every object of one program run.
#[derive(Debug, Default)]
pub struct ArcCore {
    slots: Vec<Slot>,
    free: Vec<u32>,
    /// `BN_ARC_TRACE`: write each operation to standard error.
    trace: bool,
}

impl ArcCore {
    /// An empty core (no live object), not tracing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            trace: false,
        }
    }

    /// Turns the `BN_ARC_TRACE` lines on or off.
    pub const fn set_trace(&mut self, trace: bool) {
        self.trace = trace;
    }

    /// Writes one trace line when tracing (the same text on both backends).
    fn trace(&self, event: &str, id: ObjectId, at: Site) {
        if !self.trace {
            return;
        }
        let slot = &self.slots[id.index()];
        let line = trace_line(event, &slot.class, id, slot.strong, at);
        let _ = writeln!(std::io::stderr().lock(), "{line}");
    }

    /// Records a new object of `class` at `address` (zero when the backend
    /// has none, as `bni`) with one strong reference.
    ///
    /// # Panics
    ///
    /// When 2^32 objects are alive at once, the most a slot index holds.
    pub fn register(&mut self, class: &str, address: u64, at: Site) -> ObjectId {
        let id = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[usize::try_from(index).expect("slot index fits usize")];
            slot.strong = 1;
            slot.destroying = false;
            slot.address = address;
            class.clone_into(&mut slot.class);
            ObjectId::from_parts(index, slot.generation)
        } else {
            let index = u32::try_from(self.slots.len()).expect("fewer than 2^32 live objects");
            // Generation 1 for the first object in a slot, so no id is zero.
            self.slots.push(Slot {
                generation: 1,
                strong: 1,
                destroying: false,
                class: class.to_owned(),
                address,
            });
            ObjectId::from_parts(index, 1)
        };
        self.trace("new", id, at);
        id
    }

    /// The slot of a live object, or of one whose destructor is running.
    fn slot(&mut self, id: ObjectId) -> Result<&mut Slot, ArcError> {
        self.slots
            .get_mut(id.index())
            .filter(|slot| {
                slot.generation == id.generation() && (slot.strong > 0 || slot.destroying)
            })
            .ok_or(ArcError::NotAlive(id))
    }

    /// One more strong reference to `id`; returns the new count (zero while
    /// the destructor runs, when the count is frozen).
    ///
    /// # Errors
    ///
    /// [`ArcError::NotAlive`] when `id` is not a live object.
    pub fn retain(&mut self, id: ObjectId, at: Site) -> Result<u64, ArcError> {
        let slot = self.slot(id)?;
        if slot.destroying {
            return Ok(slot.strong);
        }
        slot.strong += 1;
        let strong = slot.strong;
        self.trace("retain", id, at);
        Ok(strong)
    }

    /// One strong reference fewer; `Ok(true)` when it was the last: the
    /// object is now being destroyed (weak bindings read `NULL`, the count is
    /// frozen) and the caller runs its destruction, then
    /// [`ArcCore::finish_destroy`]. A release while the destructor runs does
    /// nothing.
    ///
    /// # Errors
    ///
    /// [`ArcError::NotAlive`] when `id` is not a live object.
    pub fn release(&mut self, id: ObjectId, at: Site) -> Result<bool, ArcError> {
        let slot = self.slot(id)?;
        if slot.destroying {
            return Ok(false);
        }
        slot.strong -= 1;
        let last = slot.strong == 0;
        slot.destroying = last;
        self.trace("release", id, at);
        if last {
            self.trace("destroy", id, at);
        }
        Ok(last)
    }

    /// Ends the destruction of `id`: its slot is free for a later object,
    /// with a new generation, so no id of this object is ever alive again.
    ///
    /// # Errors
    ///
    /// [`ArcError::NotAlive`] when `id` is not being destroyed.
    ///
    /// # Panics
    ///
    /// Never for an id this core issued: its index fits 32 bits.
    pub fn finish_destroy(&mut self, id: ObjectId) -> Result<(), ArcError> {
        let slot = self.slot(id)?;
        if !slot.destroying {
            return Err(ArcError::NotAlive(id));
        }
        slot.destroying = false;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        slot.class.clear();
        self.free
            .push(u32::try_from(id.index()).expect("slot index fits u32"));
        Ok(())
    }

    /// Whether `id` still names a live object (a weak binding reads it).
    #[must_use]
    pub fn alive(&self, id: ObjectId) -> bool {
        self.slots
            .get(id.index())
            .is_some_and(|slot| slot.generation == id.generation() && slot.strong > 0)
    }

    /// The address registered for `id` while it is alive, else zero: what a
    /// weak binding reads (`NULL` once the object is gone, or its slot holds
    /// a newer object).
    #[must_use]
    pub fn address(&self, id: ObjectId) -> u64 {
        if self.alive(id) {
            self.slots[id.index()].address
        } else {
            0
        }
    }

    /// The live object `id` names, for debugging tools.
    #[must_use]
    pub fn info(&self, id: ObjectId) -> Option<ObjectInfo> {
        self.alive(id).then(|| {
            let slot = &self.slots[id.index()];
            ObjectInfo {
                id,
                class: slot.class.clone(),
                strong: slot.strong,
            }
        })
    }

    /// Every live object, by id, for debugging tools.
    ///
    /// # Panics
    ///
    /// Never: [`ArcCore::register`] keeps every slot index within 32 bits.
    #[must_use]
    pub fn snapshot(&self) -> Vec<ObjectInfo> {
        self.slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.strong > 0)
            .map(|(index, slot)| ObjectInfo {
                id: ObjectId::from_parts(
                    u32::try_from(index).expect("slot index fits u32"),
                    slot.generation,
                ),
                class: slot.class.clone(),
                strong: slot.strong,
            })
            .collect()
    }
}

/// The `BN_ARC_TRACE` line for one event (`retain`, `release`, `destroy`),
/// the same text on both backends.
#[must_use]
pub fn trace_line(event: &str, class: &str, id: ObjectId, strong: u64, at: Site) -> String {
    format!("arc {event} {class}{id} strong={strong} at {at}")
}

/// Whether `BN_ARC_TRACE` asks for the trace (any non-empty value but `0`).
#[must_use]
pub fn trace_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.is_empty() && value != "0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_object_dies_with_its_last_strong_reference() {
        let mut core = ArcCore::default();
        let id = core.register("Box", 0, Site::default());
        assert_eq!(core.retain(id, Site::default()), Ok(2));
        assert_eq!(core.release(id, Site::default()), Ok(false));
        assert!(core.alive(id));
        assert_eq!(core.release(id, Site::default()), Ok(true));
        assert!(!core.alive(id));
        assert_eq!(core.finish_destroy(id), Ok(()));
    }

    #[test]
    fn a_weak_id_reads_the_address_only_while_its_object_lives() {
        let mut core = ArcCore::default();
        let first = core.register("Box", 0x1000, Site::default());
        assert_eq!(core.address(first), 0x1000);
        assert_eq!(core.release(first, Site::default()), Ok(true));
        assert_eq!(core.address(first), 0, "NULL once the destructor starts");
        core.finish_destroy(first).expect("destroyed");
        let second = core.register("Box", 0x2000, Site::default());
        assert_eq!(core.address(second), 0x2000);
        assert_eq!(
            core.address(first),
            0,
            "a reused slot never answers an old id"
        );
    }

    #[test]
    fn the_count_is_frozen_while_the_destructor_runs() {
        let mut core = ArcCore::default();
        let id = core.register("Box", 0, Site::default());
        assert_eq!(
            core.release(id, Site::default()),
            Ok(true),
            "the last reference starts the destruction"
        );
        assert!(!core.alive(id), "a weak binding reads NULL from now on");
        assert_eq!(
            core.retain(id, Site::default()),
            Ok(0),
            "SELF passed around by the destructor"
        );
        assert_eq!(
            core.release(id, Site::default()),
            Ok(false),
            "is not destroyed twice"
        );
        assert_eq!(core.finish_destroy(id), Ok(()));
        assert_eq!(
            core.retain(id, Site::default()),
            Err(ArcError::NotAlive(id))
        );
        assert_eq!(core.finish_destroy(id), Err(ArcError::NotAlive(id)));
    }

    #[test]
    fn no_operation_reaches_a_dead_object() {
        let mut core = ArcCore::default();
        let id = core.register("Box", 0, Site::default());
        assert_eq!(core.release(id, Site::default()), Ok(true));
        core.finish_destroy(id).expect("destroyed");
        assert_eq!(
            core.retain(id, Site::default()),
            Err(ArcError::NotAlive(id))
        );
        assert_eq!(
            core.release(id, Site::default()),
            Err(ArcError::NotAlive(id))
        );
        assert_eq!(
            core.release(ObjectId::from_bits(0), Site::default()),
            Err(ArcError::NotAlive(ObjectId::from_bits(0)))
        );
    }

    #[test]
    fn a_reused_slot_does_not_revive_old_weak_ids() {
        let mut core = ArcCore::default();
        let first = core.register("Box", 0, Site::default());
        assert_eq!(core.release(first, Site::default()), Ok(true));
        core.finish_destroy(first).expect("destroyed");
        let second = core.register("Node", 0, Site::default());
        assert_ne!(first, second, "the slot comes back with a new generation");
        assert!(
            !core.alive(first),
            "a weak id of the first object reads NULL"
        );
        assert!(core.alive(second));
        assert_eq!(
            core.info(second).map(|info| info.class),
            Some("Node".into())
        );
    }

    #[test]
    fn ids_are_never_zero_and_round_trip_through_bits() {
        let mut core = ArcCore::default();
        let id = core.register("Box", 0, Site::default());
        assert_ne!(id.bits(), 0);
        assert_eq!(ObjectId::from_bits(id.bits()), id);
    }

    #[test]
    fn the_snapshot_lists_live_objects_with_their_counts() {
        let mut core = ArcCore::default();
        let a = core.register("A", 0, Site::default());
        let b = core.register("B", 0, Site::default());
        core.retain(b, Site::default()).expect("live");
        let c = core.register("C", 0, Site::default());
        assert_eq!(core.release(c, Site::default()), Ok(true));
        core.finish_destroy(c).expect("destroyed");
        assert_eq!(
            core.snapshot(),
            vec![
                ObjectInfo {
                    id: a,
                    class: "A".into(),
                    strong: 1
                },
                ObjectInfo {
                    id: b,
                    class: "B".into(),
                    strong: 2
                },
            ]
        );
    }

    #[test]
    fn the_trace_line_names_the_event_object_count_and_source() {
        let mut core = ArcCore::default();
        let id = core.register("Box", 0, Site::default());
        assert_eq!(
            trace_line(
                "release",
                "Box",
                id,
                0,
                Site {
                    line: 14,
                    column: 5
                }
            ),
            "arc release Box#0 strong=0 at 14:5"
        );
        assert!(trace_enabled(Some("1")));
        assert!(!trace_enabled(Some("0")));
        assert!(!trace_enabled(Some("")));
        assert!(!trace_enabled(None));
    }
}
