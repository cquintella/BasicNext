use bn_runtime::{Handle, Heap};
use bn_source::{Position, SourceId, Span};

fn span() -> Span {
    let position = Position {
        source_id: SourceId(1),
        revision: bn_source::Revision(1),
        offset: 0,
        line: 1,
        column: 1,
    };
    Span {
        start: position,
        end: position,
    }
}

#[test]
fn runtime_heap_reuses_slots_without_accepting_stale_handles() {
    let mut heap = Heap::default();
    let first = Handle::new(0, 1);
    heap.insert(first, 1, 10_i32, span()).expect("allocate");
    assert_eq!(heap.remove(first), Some(vec![10]));
    let second = Handle::new(0, 2);
    heap.insert(second, 1, 20_i32, span()).expect("reuse slot");
    assert_eq!(*heap.get(second, 0, span()).expect("read live value"), 20);
    assert!(heap.get(first, 0, span()).is_err());
}
