// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `MEAN`, `VARIANCE`, `STDEV` and `RANGE` read their values in place and
//! allocate nothing (math.md, "Order, summation and overflow"; bucket 0.6.10
//! acceptance B). A counting global allocator proves it; its `unsafe` is the
//! test-only use Carlos approved on 2026-10-26 (Q-ALLOC, AGENTS.md).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use bn_core_math::{Statistic, reduce, reduce_f64};

thread_local! {
    /// Allocations made by this thread. A `const` initializer never
    /// allocates, so the allocator can use it.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn count_allocation() {
    // During thread teardown the slot may be gone; such an allocation is
    // not one of a reduction under test.
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

/// Forwards every call to `System` and counts the allocations of the
/// calling thread.
struct CountingAllocator;

// SAFETY: every method forwards its arguments unchanged to `System`, whose
// `GlobalAlloc` contract is the one the caller relies on; the count is a
// thread-local `Cell` with a `const` initializer, which never allocates or
// re-enters the allocator.
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: the caller's layout, under the caller's contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: the caller's layout, under the caller's contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: `block` came from this allocator (that is, from `System`)
        // with `layout`, as the caller guarantees.
        unsafe { System.realloc(block, layout, new_size) }
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: `block` came from this allocator (that is, from `System`)
        // with `layout`, as the caller guarantees.
        unsafe { System.dealloc(block, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// How many allocations `work` makes on this thread.
fn allocations_of(work: impl FnOnce()) -> usize {
    let before = ALLOCATIONS.with(Cell::get);
    work();
    ALLOCATIONS.with(Cell::get) - before
}

const ONE_PASS: [Statistic; 4] = [
    Statistic::Mean,
    Statistic::Variance,
    Statistic::Stdev,
    Statistic::Range,
];

#[test]
fn the_reductions_without_a_sort_allocate_nothing() {
    const LENGTH: i32 = 1000;
    let floats = (0..LENGTH).map(f64::from).collect::<Vec<_>>();
    let integers = (0..LENGTH).collect::<Vec<_>>();
    let overflowing = [f64::MAX, f64::MAX, 1.0];
    for statistic in ONE_PASS {
        assert_eq!(
            allocations_of(|| {
                black_box(reduce_f64(statistic, &floats));
            }),
            0,
            "{statistic:?} of FLOAT"
        );
        assert_eq!(
            allocations_of(|| {
                black_box(reduce(statistic, &integers));
            }),
            0,
            "{statistic:?} of INTEGER"
        );
        assert_eq!(
            allocations_of(|| {
                black_box(reduce_f64(statistic, &overflowing));
            }),
            0,
            "{statistic:?} on the overflow path"
        );
    }
}

/// The counter is live: an order statistic sorts a copy, which allocates.
#[test]
fn the_counter_sees_the_copy_of_an_order_statistic() {
    let floats = [3.0, 1.0, 2.0];
    assert!(
        allocations_of(|| {
            black_box(reduce_f64(Statistic::Median, &floats));
        }) > 0
    );
}
