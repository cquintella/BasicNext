//! A handle table whose ids carry a generation: `(generation << 32) | slot`.
//! Like the ARC `ObjectId`, one 64-bit id holds a slot and a generation (the
//! ARC puts the slot in the high half; here the generation is there, so ids
//! stay positive `i64` values). Removing a value bumps its slot's
//! generation, so an id kept after close or free never reaches a value stored
//! later in the same slot, and a second close finds nothing instead of freeing
//! memory twice (value-memory-abi.md: a stale handle returns a status, never
//! another object). One rule for every native resource handle: network
//! handles, `HOST.Exec` results, resolved address lists.

/// Bits of an id that hold the slot index; the generation is above them.
const SLOT_BITS: u32 = 32;
const SLOT_MASK: i64 = (1 << SLOT_BITS) - 1;
/// Slots an id can name: an index above this would spill into the generation.
const SLOT_CAPACITY: usize = 1 << SLOT_BITS;
/// The first generation of a slot. Never 0, so no valid id is 0.
const FIRST_GENERATION: i64 = 1;
/// A slot whose generation reaches this value is retired, never reused, so
/// ids stay positive `i64` values and no generation repeats.
const RETIRED_GENERATION: i64 = i32::MAX as i64;

struct Slot<T> {
    generation: i64,
    value: Option<T>,
}

pub struct GenerationTable<T> {
    slots: Vec<Slot<T>>,
}

impl<T> std::fmt::Debug for GenerationTable<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerationTable")
            .field("slots", &self.slots.len())
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
        Self { slots: Vec::new() }
    }

    fn pack(index: usize, generation: i64) -> i64 {
        (generation << SLOT_BITS) | i64::try_from(index).unwrap_or(SLOT_MASK)
    }

    /// The slot of `id` when its generation is the slot's current one.
    fn slot_of(&self, id: i64) -> Option<usize> {
        let index = usize::try_from(id & SLOT_MASK).ok()?;
        let generation = id >> SLOT_BITS;
        (self.slots.get(index)?.generation == generation).then_some(index)
    }

    /// Stores `value` and returns its id; `None` when the table already holds
    /// `max` slots (or every slot an id can name) and none is free.
    pub fn insert(&mut self, value: T, max: usize) -> Option<i64> {
        if let Some((index, slot)) = self
            .slots
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.value.is_none() && slot.generation < RETIRED_GENERATION)
        {
            slot.value = Some(value);
            return Some(Self::pack(index, slot.generation));
        }
        if self.slots.len() >= max.min(SLOT_CAPACITY) {
            return None;
        }
        self.slots.push(Slot {
            generation: FIRST_GENERATION,
            value: Some(value),
        });
        Some(Self::pack(self.slots.len() - 1, FIRST_GENERATION))
    }

    /// Takes the value of `id` out and moves its slot's generation on; a
    /// stale or unknown id is `None`.
    pub fn remove(&mut self, id: i64) -> Option<T> {
        let index = self.slot_of(id)?;
        let slot = &mut self.slots[index];
        let value = slot.value.take();
        if value.is_some() {
            slot.generation += 1;
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

    #[test]
    fn the_table_refuses_past_its_ceiling() {
        let mut table = GenerationTable::new();
        assert!(table.insert(1, 1).is_some());
        assert!(table.insert(2, 1).is_none());
    }
}
