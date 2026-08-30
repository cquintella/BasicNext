// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use bn::{
    heap::Heap,
    source::{Position, Span},
};

fn span() -> Span {
    Span {
        start: Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: Position {
            offset: 1,
            line: 1,
            column: 2,
        },
    }
}

#[test]
fn heap_checks_bounds_deletion_and_stale_handles() {
    let mut heap = Heap::default();
    let first = heap
        .allocate("INTEGER", 2, 0_i64, span())
        .expect("allocate region");
    *heap.get_mut(first, 1, span()).expect("write region") = 7;
    assert_eq!(*heap.get(first, 1, span()).expect("read region"), 7);
    assert_eq!(
        heap.get(first, 2, span())
            .expect_err("bounds must fail")
            .code,
        "INDEX_OUT_OF_BOUNDS"
    );
    heap.delete(first, span()).expect("delete allocation");
    assert_eq!(
        heap.delete(first, span())
            .expect_err("second delete must fail")
            .code,
        "DOUBLE_DELETE"
    );
    let replacement = heap
        .allocate("INTEGER", 1, 0_i64, span())
        .expect("reuse slot");
    assert_eq!(
        heap.get(first, 0, span())
            .expect_err("old generation must fail")
            .code,
        "USE_AFTER_DELETE"
    );
    assert_eq!(*heap.get(replacement, 0, span()).expect("new handle"), 0);
}

#[test]
fn zero_length_regions_follow_the_contract() {
    let mut heap = Heap::default();
    let empty = heap
        .allocate("BYTE", 0, 0_u8, span())
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
        .allocate("BYTE", usize::MAX, 0_u8, span())
        .expect_err("impossible allocation must fail without panicking");
    assert_eq!(error.code, "ALLOCATION_TOO_LARGE");

    let valid = heap
        .allocate("BYTE", 1, 7_u8, span())
        .expect("failed reservation must not corrupt the heap");
    assert_eq!(*heap.get(valid, 0, span()).expect("valid payload"), 7);
}
