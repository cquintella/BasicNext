// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The storage of one call frame: the SSA values, indexed by their dense
//! `ValueId`, and the bindings, keyed by `SymbolId` with a hasher made for
//! compiler-assigned integer ids (bucket 0.6.10 F.2: every instruction reads
//! or writes them, and `SipHash` was the largest cost of the interpreter).

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

use bn_ir::{SymbolId, ValueId};
use bn_value::Value;

/// The SSA values of a frame. `ValueId`s are dense from zero within a
/// function (the lowering counts them), so a vector indexed by the id holds
/// them; a slot is `None` until its instruction runs.
#[derive(Debug, Default)]
pub(crate) struct ValueSlots {
    slots: Vec<Option<Value>>,
}

impl ValueSlots {
    fn slot(id: ValueId) -> usize {
        usize::try_from(id.0).expect("a value id fits usize")
    }

    pub(crate) fn get(&self, id: ValueId) -> Option<&Value> {
        self.slots.get(Self::slot(id)).and_then(Option::as_ref)
    }

    pub(crate) fn get_mut(&mut self, id: ValueId) -> Option<&mut Value> {
        self.slots.get_mut(Self::slot(id)).and_then(Option::as_mut)
    }

    /// Stores `value` in `id`'s slot, returning what the slot held.
    pub(crate) fn insert(&mut self, id: ValueId, value: Value) -> Option<Value> {
        let slot = Self::slot(id);
        if slot >= self.slots.len() {
            self.slots.resize_with(slot + 1, || None);
        }
        self.slots[slot].replace(value)
    }

    /// Empties every slot and keeps the capacity, for the next call.
    pub(crate) fn clear(&mut self) {
        self.slots.clear();
    }

    /// The values that are set, with their ids.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (ValueId, &Value)> {
        self.slots.iter().enumerate().filter_map(|(slot, value)| {
            value.as_ref().map(|value| {
                (
                    ValueId(u32::try_from(slot).expect("a slot came from a value id")),
                    value,
                )
            })
        })
    }
}

/// The multiplier of `IdHasher`: the odd 64-bit constant nearest to
/// 2^64 / φ, which spreads consecutive ids over the hash range.
const ID_HASH_MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

/// A hasher for compiler-assigned integer ids. The keys never come from a
/// program's input, so the chosen-key resistance of `SipHash` buys nothing here.
#[derive(Debug, Default)]
pub(crate) struct IdHasher {
    hash: u64,
}

impl IdHasher {
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(ID_HASH_MULTIPLIER);
    }
}

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.add(u64::from(*byte));
        }
    }

    fn write_u32(&mut self, word: u32) {
        self.add(u64::from(word));
    }

    fn write_u64(&mut self, word: u64) {
        self.add(word);
    }
}

/// A map keyed by compiler-assigned ids (`SymbolId`).
pub(crate) type IdMap<K, V> = HashMap<K, V, BuildHasherDefault<IdHasher>>;

/// A set of compiler-assigned ids (`SymbolId`).
pub(crate) type IdSet<K> = HashSet<K, BuildHasherDefault<IdHasher>>;

/// The bindings and values of one call.
type FrameStorage = (IdMap<SymbolId, Value>, ValueSlots);

/// How many returned frames the pool keeps: enough for the call depth of
/// ordinary programs, without holding a deep recursion's memory afterwards.
const FRAME_POOL_CAPACITY: usize = 64;

/// Frames of returned calls, reused so a call does not allocate its maps
/// again (bucket 0.6.10 F.5). A call takes a frame for itself, so nested
/// calls never share one, and a frame comes back empty: a reused slot must
/// never show a value of an earlier call.
#[derive(Debug, Default)]
pub(crate) struct FramePool {
    frames: Vec<FrameStorage>,
}

impl FramePool {
    pub(crate) fn take(&mut self) -> FrameStorage {
        self.frames.pop().unwrap_or_default()
    }

    pub(crate) fn give_back(
        &mut self,
        mut symbols: IdMap<SymbolId, Value>,
        mut values: ValueSlots,
    ) {
        if self.frames.len() < FRAME_POOL_CAPACITY {
            symbols.clear();
            values.clear();
            self.frames.push((symbols, values));
        }
    }
}
