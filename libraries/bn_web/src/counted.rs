// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Values behind reference-counted handles for `BNWeb`.

use std::collections::HashMap;

/// What a release did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Release<V> {
    /// The handle is unknown or already released.
    Unknown,
    /// Another holder remains; the value stays.
    Held,
    /// That was the last holder: the value left the table and is the caller's.
    Last(V),
}

#[derive(Debug)]
pub(crate) struct Counted<V> {
    pub(crate) values: HashMap<u64, V>,
    holders: HashMap<u64, usize>,
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
    pub(crate) fn insert(&mut self, handle: u64, value: V) {
        self.values.insert(handle, value);
        self.holders.insert(handle, 1);
    }

    /// Adds a holder; `false` when `handle` is unknown or released.
    pub(crate) fn retain(&mut self, handle: u64) -> bool {
        self.holders
            .get_mut(&handle)
            .map(|count| *count += 1)
            .is_some()
    }

    /// Drops one holder. At zero the value leaves the table in this same
    /// critical section and is returned.
    pub(crate) fn release(&mut self, handle: u64) -> Release<V> {
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

    #[expect(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn get(&self, handle: &u64) -> Option<&V> {
        self.values.get(handle)
    }

    #[expect(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn get_mut(&mut self, handle: &u64) -> Option<&mut V> {
        self.values.get_mut(handle)
    }

    #[expect(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn remove(&mut self, handle: &u64) -> Option<V> {
        self.holders.remove(handle);
        self.values.remove(handle)
    }
}
