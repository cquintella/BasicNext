//! C ABI plumbing, not domain logic: the ids every native resource handle
//! carries across the C boundary (BDFL decision, 2026-11-06).
//!
//! A handle table whose ids carry a generation: `(generation << 32) | slot`.
//! Like the ARC `ObjectId`, one 64-bit id holds a slot and a generation (the
//! ARC puts the slot in the high half; here the generation is there, so ids
//! stay positive `i64` values). Removing a value bumps its slot's
//! generation, so an id kept after close or free never reaches a value stored
//! later in the same slot, and a second close finds nothing instead of freeing
//! memory twice (value-memory-abi.md: a stale handle returns a status, never
//! another object). One rule for every native resource handle: network
//! handles, `HOST.Exec` results, resolved address lists.

/// Bits of an id that hold the slot index; the generation and tag are above them.
pub const SLOT_BITS: u32 = 16;
pub const SLOT_MASK: i64 = (1 << SLOT_BITS) - 1;
/// Slots an id can name: an index above this would spill into the generation.
pub const SLOT_CAPACITY: usize = 1 << SLOT_BITS;
/// Bits of an id that hold the generation.
pub const GENERATION_BITS: u32 = 23;
pub const GENERATION_MASK: i64 = (1 << GENERATION_BITS) - 1;
/// The first generation of a slot. Never 0, so no valid id is 0.
pub const FIRST_GENERATION: i64 = 1;
/// A slot whose generation reaches this value is retired, never reused, so
/// ids stay positive `i64` values and no generation repeats.
pub const RETIRED_GENERATION: i64 = GENERATION_MASK;

/// Bits of an id that hold the copy or table tag.
pub const TAG_BITS: u32 = 24;
pub const TAG_MASK: i64 = (1 << TAG_BITS) - 1;
pub const TAG_MASK_U32: u32 = (1 << TAG_BITS) - 1;

struct Slot<T> {
    generation: i64,
    value: Option<T>,
}

pub struct GenerationTable<T> {
    tag: u32,
    slots: Vec<Slot<T>>,
    /// Empty slots that are not retired, reused last-in first-out, so an
    /// insert takes constant time however many slots the table has.
    free: Vec<usize>,
}

impl<T> std::fmt::Debug for GenerationTable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerationTable")
            .field("tag", &self.tag)
            .field("slots", &self.slots.len())
            .field("free", &self.free.len())
            .finish_non_exhaustive()
    }
}

impl<T> Default for GenerationTable<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> GenerationTable<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self::new_tagged(0)
    }

    #[must_use]
    pub const fn new_tagged(tag: u32) -> Self {
        Self {
            tag: tag & TAG_MASK_U32,
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    #[must_use]
    pub fn pack(index: usize, generation: i64, tag: u32) -> i64 {
        let tag_part = (i64::from(tag) & TAG_MASK) << (SLOT_BITS + GENERATION_BITS);
        let gen_part = (generation & GENERATION_MASK) << SLOT_BITS;
        let slot_part = i64::try_from(index).unwrap_or(SLOT_MASK) & SLOT_MASK;
        tag_part | gen_part | slot_part
    }

    /// The slot of `id` when its generation is the slot's current one and its tag matches.
    fn slot_of(&self, id: i64) -> Option<usize> {
        if id <= 0 {
            return None;
        }
        let tag = u32::try_from((id >> (SLOT_BITS + GENERATION_BITS)) & TAG_MASK).ok()?;
        if tag != self.tag {
            return None;
        }
        let generation = (id >> SLOT_BITS) & GENERATION_MASK;
        let index = usize::try_from(id & SLOT_MASK).ok()?;
        let slot = self.slots.get(index)?;
        (slot.generation == generation).then_some(index)
    }

    /// Stores `value` and returns its id; `None` when the table already holds
    /// `max` slots (or every slot an id can name) and none is free.
    pub fn insert(&mut self, value: T, max: usize) -> Option<i64> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index];
            slot.value = Some(value);
            return Some(Self::pack(index, slot.generation, self.tag));
        }
        if self.slots.len() >= max.min(SLOT_CAPACITY) {
            return None;
        }
        self.slots.push(Slot {
            generation: FIRST_GENERATION,
            value: Some(value),
        });
        Some(Self::pack(self.slots.len() - 1, FIRST_GENERATION, self.tag))
    }

    /// Takes the value of `id` out and moves its slot's generation on; a
    /// stale or unknown id is `None`.
    pub fn remove(&mut self, id: i64) -> Option<T> {
        let index = self.slot_of(id)?;
        let slot = &mut self.slots[index];
        let value = slot.value.take();
        if value.is_some() {
            slot.generation += 1;
            if slot.generation < RETIRED_GENERATION {
                self.free.push(index);
            }
        }
        value
    }

    #[must_use]
    pub fn get(&self, id: i64) -> Option<&T> {
        self.slot_of(id)
            .and_then(|index| self.slots[index].value.as_ref())
    }

    pub fn get_mut(&mut self, id: i64) -> Option<&mut T> {
        let index = self.slot_of(id)?;
        self.slots[index].value.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::GenerationTable;

    #[test]
    fn a_removed_id_never_reaches_the_value_that_reuses_its_slot() {
        let mut table = GenerationTable::new();
        let first = table.insert("first", 8).expect("insert");
        assert_eq!(table.remove(first), Some("first"));
        let second = table.insert("second", 8).expect("insert again");
        assert_ne!(first, second);
        assert_eq!(table.get(first), None);
        assert_eq!(table.remove(first), None, "a second close finds nothing");
        assert_eq!(table.get(second), Some(&"second"));
        assert!(first > 0 && second > 0);
    }

    /// A freed slot is reused before the table grows, and a slot whose
    /// generation reaches the retired value is never handed out again.
    #[test]
    fn freed_slots_are_reused_and_retired_slots_are_not() {
        let mut table = GenerationTable::new();
        let first = table.insert(1, 2).expect("first");
        let second = table.insert(2, 2).expect("second");
        assert_eq!(table.remove(first), Some(1));
        let third = table.insert(3, 2).expect("reuses the freed slot");
        assert_eq!(third & super::SLOT_MASK, first & super::SLOT_MASK);
        assert!(table.insert(4, 2).is_none(), "both slots are live");
        table.slots[0].generation = super::RETIRED_GENERATION - 1;
        let last = GenerationTable::<i32>::pack(0, super::RETIRED_GENERATION - 1, 0);
        assert_eq!(table.remove(last), Some(3));
        assert!(table.insert(5, 2).is_none(), "a retired slot is not reused");
        assert_eq!(table.get(second), Some(&2));
    }

    #[test]
    fn the_table_refuses_past_its_ceiling() {
        let mut table = GenerationTable::new();
        assert!(table.insert(1, 1).is_some());
        assert!(table.insert(2, 1).is_none());
    }
}
