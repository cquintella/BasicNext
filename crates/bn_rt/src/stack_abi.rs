// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The stack check of compiled programs (0.6.md, "Recursion depth"; known
//! issue K53): before each BN call, whether the caller's frame is close to
//! the end of the running thread's stack, so the program stops with
//! `STACK_OVERFLOW` instead of a crash. The bounds come from the operating
//! system, once per thread (`ASYNC` workers have stacks of their own).

use std::cell::Cell;

/// Stack left below the frame when the check trips: room for the
/// `STACK_OVERFLOW` report.
const STACK_MARGIN: usize = 256 * 1024;

thread_local! {
    /// The lowest frame address a BN call may start from on this thread;
    /// `0` until computed.
    static LIMIT: Cell<usize> = const { Cell::new(0) };
}

/// `1` when a BN call from `frame` would exhaust the stack, else `0`. The
/// stack grows down on every native target `bnc` emits for.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_stack_check(frame: *const u8) -> i32 {
    let limit = LIMIT.with(|limit| {
        if limit.get() == 0 {
            // `1` when the bounds are unknown: no frame is below it.
            limit.set(stack_low().map_or(1, |low| low.saturating_add(STACK_MARGIN)));
        }
        limit.get()
    });
    i32::from((frame as usize) < limit)
}

/// The lowest address of the running thread's stack.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // Operating-system calls on the current thread.
fn stack_low() -> Option<usize> {
    // SAFETY: both calls only read the attributes of the calling thread,
    // which is alive for their whole duration.
    let (top, size) = unsafe {
        let thread = libc::pthread_self();
        (
            libc::pthread_get_stackaddr_np(thread) as usize,
            libc::pthread_get_stacksize_np(thread),
        )
    };
    top.checked_sub(size)
}

/// The lowest address of the running thread's stack.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)] // Operating-system calls on the current thread.
fn stack_low() -> Option<usize> {
    let mut attributes = std::mem::MaybeUninit::<libc::pthread_attr_t>::uninit();
    let mut address = std::ptr::null_mut();
    let mut size = 0;
    // SAFETY: `pthread_getattr_np` initializes `attributes` for the calling
    // thread when it returns 0; only then are they read and destroyed.
    unsafe {
        if libc::pthread_getattr_np(libc::pthread_self(), attributes.as_mut_ptr()) != 0 {
            return None;
        }
        let read =
            libc::pthread_attr_getstack(attributes.as_ptr(), &raw mut address, &raw mut size);
        libc::pthread_attr_destroy(attributes.as_mut_ptr());
        (read == 0).then_some(address as usize)
    }
}

/// The lowest address of the running thread's stack.
#[cfg(windows)]
#[allow(unsafe_code)] // Operating-system call on the current thread.
fn stack_low() -> Option<usize> {
    // SAFETY: the foreign signature matches the C or system function this module calls.
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let (mut low, mut high) = (0_usize, 0_usize);
    // SAFETY: kernel32 writes the calling thread's stack bounds into the two
    // locals, which outlive the call.
    unsafe { GetCurrentThreadStackLimits(&raw mut low, &raw mut high) };
    (low != 0).then_some(low)
}

/// Unknown bounds (other targets): the check never trips.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn stack_low() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::bn_rt_stack_check;

    #[test]
    fn a_shallow_frame_passes_and_the_stack_end_trips() {
        let marker = 0_u8;
        assert_eq!(bn_rt_stack_check(std::ptr::addr_of!(marker)), 0);
        if super::stack_low().is_some() {
            assert_eq!(bn_rt_stack_check(std::ptr::null()), 1);
        }
    }
}
