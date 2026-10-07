// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn_runtime::{Handle, Heap};
use bn_source::{Position, Span};

fn span() -> Span {
    Span {
        start: Position {
            source_id: Position::UNKNOWN_SOURCE,
            revision: Position::UNKNOWN_REVISION,
            offset: 0,
            line: 1,
            column: 1,
        },
        end: Position {
            source_id: Position::UNKNOWN_SOURCE,
            revision: Position::UNKNOWN_REVISION,
            offset: 1,
            line: 1,
            column: 2,
        },
    }
}

fn handle(slot: u32, generation: u32) -> Handle {
    Handle::new(slot, generation)
}

#[test]
fn heap_checks_bounds_removal_and_stale_handles() {
    let mut heap = Heap::default();
    let first = handle(0, 1);
    heap.insert(first, 2, 0_i64, span())
        .expect("allocate region");
    *heap.get_mut(first, 1, span()).expect("write region") = 7;
    assert_eq!(*heap.get(first, 1, span()).expect("read region"), 7);
    assert_eq!(
        heap.get(first, 2, span())
            .expect_err("bounds must fail")
            .code,
        "INDEX_OUT_OF_BOUNDS"
    );
    assert_eq!(heap.remove(first), Some(vec![0, 7]));
    assert_eq!(heap.remove(first), None, "a removed payload is gone");
    let replacement = handle(0, 2);
    heap.insert(replacement, 1, 0_i64, span())
        .expect("reuse slot");
    assert_eq!(
        heap.get(first, 0, span())
            .expect_err("old generation must fail")
            .code,
        "USE_AFTER_RELEASE"
    );
    assert_eq!(heap.remove(first), None, "a stale handle removes nothing");
    assert_eq!(*heap.get(replacement, 0, span()).expect("new handle"), 0);
}

#[test]
fn zero_length_regions_follow_the_contract() {
    let mut heap = Heap::default();
    let empty = handle(3, 1);
    heap.insert(empty, 0, 0_u8, span())
        .expect("zero length is valid");
    assert_eq!(
        heap.get(empty, 0, span())
            .expect_err("empty region has no element")
            .code,
        "INDEX_OUT_OF_BOUNDS"
    );
}

#[test]
fn impossible_region_reservation_is_a_diagnostic() {
    let mut heap = Heap::default();
    let error = heap
        .insert(handle(0, 1), usize::MAX, 0_u8, span())
        .expect_err("impossible allocation must fail without panicking");
    assert_eq!(error.code, "ALLOCATION_TOO_LARGE");

    let valid = handle(0, 1);
    heap.insert(valid, 1, 7_u8, span())
        .expect("failed reservation must not corrupt the heap");
    assert_eq!(*heap.get(valid, 0, span()).expect("valid payload"), 7);
}
