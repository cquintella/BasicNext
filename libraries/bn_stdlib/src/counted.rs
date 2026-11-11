// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Values behind reference-counted handles, with each count kept beside its
//! value. A table is `Mutex<Counted<V>>`: a retain, a release that reaches
//! zero and a read are one critical section, so no read sees a value its
//! release already ended. This is the protocol of `Arc`: the release that
//! reaches zero owns the value alone. One type for every native handle table
//! that counts holders (`Bytes`, keys, secrets, compressed buffers, archives).

use std::collections::HashMap;

/// What a release did.
#[derive(Debug, PartialEq, Eq)]
pub enum Release<V> {
    /// The handle is unknown or already released.
    Unknown,
    /// Another holder remains; the value stays.
    Held,
    /// That was the last holder: the value left the table and is the caller's.
    Last(V),
}

#[derive(Debug)]
pub struct Counted<V> {
    pub values: HashMap<u64, V>,
    pub holders: HashMap<u64, usize>,
}

impl<V> Default for Counted<V> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
            holders: HashMap::new(),
        }
    }
}

impl<V> Counted<V> {
    /// Stores `value` under `handle` with one holder.
    pub fn insert(&mut self, handle: u64, value: V) {
        self.values.insert(handle, value);
        self.holders.insert(handle, 1);
    }

    /// Adds a holder; `false` when `handle` is unknown or released.
    pub fn retain(&mut self, handle: u64) -> bool {
        self.holders
            .get_mut(&handle)
            .map(|count| *count += 1)
            .is_some()
    }

    /// Drops one holder. At zero the value leaves the table in this same
    /// critical section and is returned.
    pub fn release(&mut self, handle: u64) -> Release<V> {
        let Some(count) = self.holders.get_mut(&handle) else {
            return Release::Unknown;
        };
        *count -= 1;
        if *count > 0 {
            return Release::Held;
        }
        self.holders.remove(&handle);
        self.values
            .remove(&handle)
            .map_or(Release::Unknown, Release::Last)
    }
}

#[cfg(test)]
mod tests {
    use super::{Counted, Release};

    #[test]
    fn the_last_release_takes_the_value_out_in_one_step() {
        let mut table = Counted::default();
        table.insert(1, "key");
        assert!(table.retain(1));
        assert_eq!(table.release(1), Release::Held);
        assert_eq!(table.values.get(&1), Some(&"key"), "one holder remains");
        assert_eq!(table.release(1), Release::Last("key"));
        assert_eq!(table.values.get(&1), None);
        assert_eq!(table.release(1), Release::Unknown);
        assert!(!table.retain(1));
    }
}
