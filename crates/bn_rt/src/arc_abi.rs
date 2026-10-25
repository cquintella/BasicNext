// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI of the ARC core (`arc`) for `bnc`: one core per module copy, as in
//! `bni` — the program's on its main thread, and a fresh one for each `ASYNC`
//! task ([`run_in_fresh_core`]). A copy shares no object with another
//! (`0.6.md`, "Isolation and conformance"), so every operation on an object
//! runs on the thread whose core counts it, and no lock is needed; an
//! operation from another thread finds no live object and reports a broken
//! invariant. Every function takes and returns plain integers (an object id
//! is one `u64`), so none dereferences a pointer except the class name
//! `bn_rt_arc_register` reads. `bn_rt_arc_dump` and `bn_rt_arc_info` are for
//! debuggers (`expr bn_rt_arc_dump()` in `lldb`, `call bn_rt_arc_dump()` in
//! `gdb`) and show the core of the thread they run on.

use std::cell::RefCell;
use std::ffi::{CStr, c_char, c_void};
use std::io::Write as _;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::arc::{ArcCore, ArcError, ObjectId, ObjectInfo, Site, trace_enabled};
use crate::trap_abi::{RuntimeFailure, record_failure};

thread_local! {
    /// The core of the module copy that runs on this thread.
    static CORE: RefCell<ArcCore> = RefCell::new(new_core());
}

/// How far apart the first generations of consecutive cores start (the
/// golden ratio of 2^32), so ids of two cores do not coincide in practice.
const GENERATION_SPREAD: u32 = 0x9E37_79B9;

/// An empty core that writes the `BN_ARC_TRACE` lines when asked to. Its
/// slots start at a generation no other core of the process starts at.
fn new_core() -> ArcCore {
    static TRACE: OnceLock<bool> = OnceLock::new();
    static CORES: AtomicU32 = AtomicU32::new(0);
    let first = CORES
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_mul(GENERATION_SPREAD)
        .wrapping_add(1);
    let mut core = ArcCore::with_first_generation(first);
    core.set_trace(
        *TRACE.get_or_init(|| trace_enabled(std::env::var("BN_ARC_TRACE").ok().as_deref())),
    );
    core
}

/// Runs `operation` on this thread's core.
fn with_core<R>(operation: impl FnOnce(&mut ArcCore) -> R) -> R {
    CORE.with_borrow_mut(operation)
}

fn site(line: u32, column: u32) -> Site {
    Site { line, column }
}

/// A broken ARC invariant is a toolchain defect: the calling site reports it.
fn invariant(error: ArcError) {
    record_failure(RuntimeFailure {
        code: "INVALID_IR",
        facts: vec![("detail", error.to_string())],
    });
}

/// Registers a new object of class `class` (a NUL-terminated name) at
/// `object` with one strong reference; returns its id. The core keeps the
/// address only to hand it back to a weak binding (`bn_rt_arc_address`); it
/// never reads or writes the object.
#[allow(unsafe_code)] // C ABI export; reads the class name constant.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_register(
    class: *const c_char,
    object: *mut c_void,
    line: u32,
    column: u32,
) -> u64 {
    let class = if class.is_null() {
        String::new()
    } else {
        // SAFETY: emitted code passes a NUL-terminated class-name constant.
        unsafe { CStr::from_ptr(class) }
            .to_string_lossy()
            .into_owned()
    };
    let address = u64::try_from(object.expose_provenance()).expect("addresses fit 64 bits");
    with_core(|core| core.register(&class, address, site(line, column))).bits()
}

/// What a weak binding holding `id` reads: the object while it is alive,
/// else null (it is gone, or its slot holds a newer object).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_address(id: u64) -> *mut c_void {
    let id = ObjectId::from_bits(id);
    let address = with_core(|core| core.address(id));
    std::ptr::with_exposed_provenance_mut(
        usize::try_from(address).expect("a registered address fits usize"),
    )
}

/// One more strong reference: `0`, or `-1` after recording the failure.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_retain(id: u64, line: u32, column: u32) -> i32 {
    let (id, at) = (ObjectId::from_bits(id), site(line, column));
    match with_core(|core| core.retain(id, at)) {
        Ok(_) => 0,
        Err(error) => {
            invariant(error);
            -1
        }
    }
}

/// One strong reference fewer: `1` when it was the last (destroy the
/// object), `0` otherwise, `-1` after recording the failure.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_release(id: u64, line: u32, column: u32) -> i32 {
    let (id, at) = (ObjectId::from_bits(id), site(line, column));
    match with_core(|core| core.release(id, at)) {
        Ok(last) => i32::from(last),
        Err(error) => {
            invariant(error);
            -1
        }
    }
}

/// Ends the destruction `bn_rt_arc_release` started (it returned `1`):
/// `0`, or `-1` after recording the failure.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_finish_destroy(id: u64) -> i32 {
    let id = ObjectId::from_bits(id);
    match with_core(|core| core.finish_destroy(id)) {
        Ok(()) => 0,
        Err(error) => {
            invariant(error);
            -1
        }
    }
}

/// Runs `task`, an `ASYNC` task's trampoline call, on a fresh core: its
/// module copy's objects. When it returns, the end of the task is the end of
/// that program (`0.6.md`, "Isolation and conformance"): what the copy still
/// holds — through its `STATIC` fields or a cycle — ceases to exist with no
/// `DESTRUCTOR`, and its memory is freed, as `bni` drops the copy's core.
/// The worker's own core comes back afterwards.
pub fn run_in_fresh_core<R>(task: impl FnOnce() -> R) -> R {
    let outer = CORE.replace(new_core());
    let result = task();
    let finished = CORE.replace(outer);
    for address in finished.live_addresses() {
        free_object(address);
    }
    result
}

/// Frees the memory of an object of a finished copy.
#[allow(unsafe_code)] // Releases a C allocation; no safe API frees it.
fn free_object(address: u64) {
    // Registered from a `usize` (`bn_rt_arc_register`), so it always fits.
    let Ok(address) = usize::try_from(address) else {
        return;
    };
    let object = std::ptr::with_exposed_provenance_mut::<c_void>(address);
    // SAFETY: `address` is what emitted code registered for an object it
    // allocated with the C allocator (`calloc`), the pointer its destruction
    // passes to `free`. Its core was taken off the thread before this call
    // and is dropped after it, so no release or weak read reaches the object
    // again, and the task whose copy held it has returned, so no live
    // reference remains (an `ASYNC` task shares no object with other code).
    unsafe { libc::free(object) };
}

fn info_line(info: &ObjectInfo) -> String {
    format!("{}{} strong={}", info.class, info.id, info.strong)
}

// The two debugger aids hold no closure: Rust files a closure in a
// namespace named after its function, and `lldb` then finds two
// `bn_rt_arc_dump` (the namespace and the function) and refuses
// `expr bn_rt_arc_dump()` as ambiguous. Their closures live in these.

fn live_objects() -> Vec<ObjectInfo> {
    with_core(|core| core.snapshot())
}

fn object_line(id: ObjectId) -> String {
    with_core(|core| core.info(id)).map_or_else(
        || format!("arc: {id} is not alive"),
        |info| format!("arc: {}", info_line(&info)),
    )
}

/// Debugger aid: prints every live object of this thread's copy to
/// `stderr`.
#[allow(unsafe_code)] // C ABI export for debuggers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_dump() {
    let snapshot = live_objects();
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "arc: {} live object(s)", snapshot.len());
    for info in &snapshot {
        let _ = writeln!(stderr, "  {}", info_line(info));
    }
}

/// Debugger aid: prints the object `id` names, or that it is not alive.
#[allow(unsafe_code)] // C ABI export for debuggers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_info(id: u64) {
    let line = object_line(ObjectId::from_bits(id));
    let _ = writeln!(std::io::stderr().lock(), "{line}");
}

/// Checks whether `id` names a currently live object or region in this thread's ARC core.
#[must_use]
pub fn is_object_alive(id: u64) -> bool {
    if id == 0 {
        return false;
    }
    with_core(|core| core.alive(ObjectId::from_bits(id)))
}

/// Reports a `USE_AFTER_RELEASE` failure and terminates the process when a native
/// wrapper attempts to access a region whose last strong reference was released.
pub fn abort_use_after_release() -> ! {
    record_failure(RuntimeFailure {
        code: "USE_AFTER_RELEASE",
        facts: vec![(
            "detail",
            "allocation handle refers to released memory".to_string(),
        )],
    });
    crate::trap_abi::bn_rt_trap_report_failure(std::ptr::null());
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_c_abi_counts_and_reports_a_broken_invariant() {
        let class = c"Box";
        let mut object = 0_u8;
        let address = (&raw mut object).cast::<c_void>();
        let id = bn_rt_arc_register(class.as_ptr(), address, 1, 1);
        assert_eq!(
            bn_rt_arc_address(id),
            address,
            "a weak binding reads the live object"
        );
        assert_ne!(id, 0);
        assert!(with_core(|core| core.alive(ObjectId::from_bits(id))));
        assert_eq!(bn_rt_arc_retain(id, 1, 1), 0);
        assert_eq!(bn_rt_arc_release(id, 1, 1), 0);
        assert_eq!(bn_rt_arc_release(id, 1, 1), 1);
        assert!(
            !with_core(|core| core.alive(ObjectId::from_bits(id))),
            "a weak binding reads NULL"
        );
        assert!(bn_rt_arc_address(id).is_null(), "from the destructor on");
        assert_eq!(
            bn_rt_arc_release(id, 1, 1),
            0,
            "frozen while the destructor runs"
        );
        assert_eq!(bn_rt_arc_finish_destroy(id), 0);
        assert_eq!(
            bn_rt_arc_release(id, 1, 1),
            -1,
            "a dead object is an invariant failure"
        );
    }

    /// Each module copy has its own core: an object of the program's core is
    /// not alive in a worker's, so an operation from the wrong thread is an
    /// invariant failure, never a count on another copy's object.
    #[test]
    fn each_thread_counts_only_its_own_objects() {
        let class = c"Box";
        let mut object = 0_u8;
        let id = bn_rt_arc_register(class.as_ptr(), (&raw mut object).cast(), 1, 1);
        std::thread::scope(|scope| {
            scope.spawn(|| assert_eq!(bn_rt_arc_retain(id, 1, 1), -1));
        });
        assert_eq!(bn_rt_arc_retain(id, 1, 1), 0);
        assert_eq!(bn_rt_arc_release(id, 1, 1), 0);
    }

    /// A task runs on a fresh core; what it still holds at its end is freed
    /// (K88), and the thread's own core and objects come back untouched.
    #[test]
    #[allow(unsafe_code)] // Allocates the block the task leaves behind.
    fn a_task_core_frees_what_the_task_left_and_restores_the_outer_core() {
        let class = c"Box";
        let mut outer_object = 0_u8;
        let outer = bn_rt_arc_register(class.as_ptr(), (&raw mut outer_object).cast(), 1, 1);
        let inner = run_in_fresh_core(|| {
            // SAFETY: a fresh 16-byte block, as emitted code allocates one.
            let block = unsafe { libc::calloc(1, 16) };
            assert!(!block.is_null());
            let id = bn_rt_arc_register(class.as_ptr(), block, 1, 1);
            assert!(with_core(|core| core.alive(ObjectId::from_bits(id))));
            assert!(!with_core(|core| core.alive(ObjectId::from_bits(outer))));
            id
        });
        assert!(with_core(|core| core.alive(ObjectId::from_bits(outer))));
        assert_eq!(
            bn_rt_arc_retain(inner, 1, 1),
            -1,
            "the task's object is gone"
        );
    }
}
