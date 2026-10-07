// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI of the ARC core (`arc`) for `bnc`: one core per process. Every
//! function takes and returns plain integers (an object id is one `u64`), so
//! none dereferences a pointer except the class name `bn_rt_arc_register`
//! reads. `bn_rt_arc_dump` and `bn_rt_arc_info` are for debuggers
//! (`expr bn_rt_arc_dump()` in `lldb`, `call bn_rt_arc_dump()` in `gdb`).

use std::ffi::{CStr, c_char, c_void};
use std::io::Write as _;
use std::sync::{Mutex, MutexGuard};

use crate::arc::{ArcCore, ArcError, ObjectId, ObjectInfo, Site, trace_enabled};
use crate::trap_abi::{RuntimeFailure, record_failure};

static CORE: Mutex<ArcCore> = Mutex::new(ArcCore::new());

fn core() -> MutexGuard<'static, ArcCore> {
    static TRACE: std::sync::Once = std::sync::Once::new();
    // A panic while holding the lock leaves the counts as they were before
    // the panicking operation; keep using them.
    let mut core = CORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    TRACE.call_once(|| {
        core.set_trace(trace_enabled(std::env::var("BN_ARC_TRACE").ok().as_deref()));
    });
    core
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
    core().register(&class, address, site(line, column)).bits()
}

/// What a weak binding holding `id` reads: the object while it is alive,
/// else null (it is gone, or its slot holds a newer object).
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_address(id: u64) -> *mut c_void {
    let address = core().address(ObjectId::from_bits(id));
    std::ptr::with_exposed_provenance_mut(
        usize::try_from(address).expect("a registered address fits usize"),
    )
}

/// One more strong reference: `0`, or `-1` after recording the failure.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_retain(id: u64, line: u32, column: u32) -> i32 {
    match core().retain(ObjectId::from_bits(id), site(line, column)) {
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
    match core().release(ObjectId::from_bits(id), site(line, column)) {
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
    match core().finish_destroy(ObjectId::from_bits(id)) {
        Ok(()) => 0,
        Err(error) => {
            invariant(error);
            -1
        }
    }
}

/// `1` while `id` names a live object (a weak binding reads it), else `0`.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_alive(id: u64) -> i32 {
    i32::from(core().alive(ObjectId::from_bits(id)))
}

fn info_line(info: &ObjectInfo) -> String {
    format!("{}{} strong={}", info.class, info.id, info.strong)
}

/// Debugger aid: prints every live object to `stderr`.
#[allow(unsafe_code)] // C ABI export for debuggers.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_arc_dump() {
    let snapshot = core().snapshot();
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
    let id = ObjectId::from_bits(id);
    let line = core().info(id).map_or_else(
        || format!("arc: {id} is not alive"),
        |info| format!("arc: {}", info_line(&info)),
    );
    let _ = writeln!(std::io::stderr().lock(), "{line}");
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
        assert_eq!(bn_rt_arc_alive(id), 1);
        assert_eq!(bn_rt_arc_retain(id, 1, 1), 0);
        assert_eq!(bn_rt_arc_release(id, 1, 1), 0);
        assert_eq!(bn_rt_arc_release(id, 1, 1), 1);
        assert_eq!(bn_rt_arc_alive(id), 0, "a weak binding reads NULL");
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

    /// `BNDispatch` workers share the process's one core: retains and
    /// releases from many threads at once keep one exact count, and the
    /// object is destroyed exactly once, by the last release.
    #[test]
    fn concurrent_retains_and_releases_keep_one_count() {
        const THREADS: u32 = 8;
        const ROUNDS: u32 = 10_000;
        let class = c"Shared";
        let mut object = 0_u8;
        let id = bn_rt_arc_register(class.as_ptr(), (&raw mut object).cast(), 1, 1);
        // Each worker gets its own reference; the creator keeps one too.
        for _ in 0..THREADS {
            assert_eq!(bn_rt_arc_retain(id, 1, 1), 0);
        }
        let last = std::sync::atomic::AtomicU32::new(0);
        let release = || {
            if bn_rt_arc_release(id, 1, 1) == 1 {
                last.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        };
        std::thread::scope(|scope| {
            for _ in 0..THREADS {
                scope.spawn(|| {
                    for _ in 0..ROUNDS {
                        assert_eq!(bn_rt_arc_retain(id, 1, 1), 0);
                        assert_eq!(bn_rt_arc_release(id, 1, 1), 0, "its own still holds it");
                    }
                    release();
                });
            }
            scope.spawn(release);
        });
        assert_eq!(last.into_inner(), 1, "destroyed once, by the last release");
        assert_eq!(bn_rt_arc_finish_destroy(id), 0);
        assert_eq!(bn_rt_arc_alive(id), 0);
    }
}
