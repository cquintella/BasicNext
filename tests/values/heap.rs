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

use super::support;

use std::{fs, process::Command};
use support::{TestDir, compile_native, run};

/// General alternative ownership (T.6.C2): strings and objects in an alternative
/// undergo exactly one release on reassignment or scope end, with no leak or double free.
#[test]
fn general_alternative_releases_strings_and_objects_cleanly() {
    let directory = TestDir::new("heap-alt-ownership").expect("create directory");
    let path = directory.join("alt.bn");
    fs::write(
        &path,
        r#"CLASS Node
    PUBLIC name AS STRING = ""
    PUBLIC FUNCTION CONSTRUCTOR(n AS STRING)
        SELF.name = n
    END FUNCTION
    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "free", SELF.name
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET a AS Node OR STRING = NEW Node("node1")
    a = "string_payload"
    PRINT "reassigned to string"

    LET b AS Node OR STRING = "initial_string"
    b = NEW Node("node2")
    PRINT "reassigned to object"
END FUNCTION
"#,
    )
    .expect("write source");
    let artifact = compile_native(&path, &directory);
    let output = run(
        &mut Command::new(&artifact),
        None,
        std::time::Duration::from_secs(30),
    )
    .expect("run process");
    assert_eq!(
        output.status.code(),
        Some(0),
        "execution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected = "free node1\nreassigned to string\nreassigned to object\nfree node2\n";
    assert_eq!(stdout, expected);
}

/// K89: Destroying an object releases the element buffer of its fixed-vector fields.
#[test]
fn object_destruction_releases_fixed_vector_element_buffer() {
    let directory = TestDir::new("heap-k89-vector-field").expect("create directory");
    let path = directory.join("vec_field.bn");
    fs::write(
        &path,
        r#"CLASS Container
    PUBLIC cells AS INTEGER[1000]
    PUBLIC FUNCTION CONSTRUCTOR(val AS INTEGER)
        SELF.cells[0] = val
        SELF.cells[999] = val * 2
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    FOR i AS INTEGER = 1 TO 2000
        LET c AS Container = NEW Container(i)
        IF c.cells[0] <> i OR c.cells[999] <> i * 2 THEN
            PRINT "mismatch at", i
            RETURN
        END IF
    END FOR
    PRINT "ok"
END FUNCTION
"#,
    )
    .expect("write source");
    let artifact = compile_native(&path, &directory);
    let output = run(
        &mut Command::new(&artifact),
        None,
        std::time::Duration::from_secs(30),
    )
    .expect("run process");
    assert_eq!(
        output.status.code(),
        Some(0),
        "execution failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "ok");
}
