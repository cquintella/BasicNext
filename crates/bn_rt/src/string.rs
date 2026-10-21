// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! C ABI for Basic Next native strings (Sprint 7, G7-D1).
//!
//! Every non-null STRING payload is preceded by a 16-byte header. Callers
//! read it directly: nothing guesses whether the header exists.
//! `[ ref_count: AtomicU64 ][ len: u64 ][ bytes... ][ 0 ]`
//! The value pointer is `base + 16`. `len` excludes the trailing NUL.
//! `ref_count == u64::MAX` is immortal (a static literal).

use std::ffi::{CStr, c_char};
use std::io::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{LibcStdout, trap_abi};

/// Sentinel reference count of a static literal. Retain and release leave it.
pub(crate) const IMMORTAL_REF_COUNT: u64 = u64::MAX;

/// Highest count a heap string may reach. The next retain traps (G7-D3).
pub(crate) const MAX_STRING_REF_COUNT: u64 = i64::MAX as u64;

/// Count a fresh heap string starts at.
const INITIAL_REF_COUNT: u64 = 1;

/// The header immediately preceding the text payload.
#[repr(C, align(8))]
pub(crate) struct StringHeader {
    pub ref_count: AtomicU64,
    pub len: u64,
}

pub(crate) const HEADER_SIZE: usize = std::mem::size_of::<StringHeader>();

/// Heap strings currently alive. Tests read it; it is not a HOST surface.
static LIVE_STRINGS: AtomicU64 = AtomicU64::new(0);

/// How many heap strings have not been freed yet. Test hook, not a HOST surface.
#[cfg(test)]
#[must_use]
pub(crate) fn live_string_count() -> u64 {
    LIVE_STRINGS.load(Ordering::Acquire)
}

/// The header of a STRING payload.
///
/// # Safety
///
/// G7-D1: `ptr` is null or points at a payload whose header occupies the
/// `HEADER_SIZE` bytes before it, aligned to 8.
#[allow(unsafe_code, clippy::cast_ptr_alignment)]
fn header_mut<'a>(ptr: *mut c_char) -> Option<&'a StringHeader> {
    if ptr.is_null() || !(ptr as usize).is_multiple_of(std::mem::align_of::<StringHeader>()) {
        return None;
    }
    // SAFETY: G7-D1. Every non-null STRING payload has this header.
    let header_ptr = unsafe { ptr.cast::<u8>().sub(HEADER_SIZE).cast::<StringHeader>() };
    let header = unsafe { &*header_ptr };
    let rc = header.ref_count.load(Ordering::Relaxed);
    if rc > MAX_STRING_REF_COUNT && rc != IMMORTAL_REF_COUNT {
        return None;
    }
    if header.len > i32::MAX as u64 {
        return None;
    }
    Some(header)
}

/// Ends the program. The sentence comes from the string catalog (G7-D3).
fn trap_count(key: &'static str) -> ! {
    let message = bn_catalog::text("string", key);
    let _ = writeln!(std::io::stderr(), "{message}");
    std::process::exit(1);
}

/// Increments the buffer count. An immortal literal is unchanged.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_retain(ptr: *const c_char) -> i32 {
    let Some(header) = header_mut(ptr.cast_mut()) else {
        return 0;
    };
    loop {
        let current = header.ref_count.load(Ordering::Relaxed);
        if current == IMMORTAL_REF_COUNT {
            return 0;
        }
        if current >= MAX_STRING_REF_COUNT {
            trap_count("retain-overflow");
        }
        if header
            .ref_count
            .compare_exchange_weak(current, current + 1, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
        {
            return 0;
        }
    }
}

/// Decrements the buffer count and frees it at the transition 1→0.
/// An immortal literal and a null pointer are unchanged. A count already 0 traps.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_release(ptr: *const c_char) -> i32 {
    let Some(header) = header_mut(ptr.cast_mut()) else {
        return 0;
    };
    loop {
        let current = header.ref_count.load(Ordering::Relaxed);
        if current == IMMORTAL_REF_COUNT {
            return 0;
        }
        if current == 0 {
            trap_count("release-of-zero");
        }
        if header
            .ref_count
            .compare_exchange_weak(current, current - 1, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            continue;
        }
        if current == 1 {
            std::sync::atomic::fence(Ordering::Acquire);
            LIVE_STRINGS.fetch_sub(1, Ordering::Release);
            // SAFETY: G7-D1: a non-null STRING is preceded by its header; the allocation is the header, the payload and a trailing NUL, and a null allocation is not read.
            let base = unsafe { ptr.cast::<u8>().sub(HEADER_SIZE) };
            // SAFETY: `base` is the block `c_string_from_bytes` allocated, and
            // this thread observed the count fall from 1 to 0.
            unsafe { libc::free(base.cast::<libc::c_void>().cast_mut()) };
        }
        return 0;
    }
}

/// Allocates one heap STRING: header, a copy of `bytes`, and a trailing NUL.
/// The returned pointer is the payload. Its count starts at 1.
#[allow(unsafe_code)]
#[allow(clippy::cast_ptr_alignment)]
pub(crate) fn c_string_from_bytes(bytes: &[u8]) -> *mut c_char {
    let len = bytes.len();
    let total_alloc = HEADER_SIZE
        .checked_add(len)
        .and_then(|size| size.checked_add(1))
        .unwrap_or_else(|| {
            panic!(
                "{}",
                bn_catalog::text("string", "msg.string_size_fits_address_space",)
            )
        });

    // SAFETY: the size is header + payload + NUL, and malloc returns that many
    // bytes or null.
    let base = unsafe { libc::malloc(total_alloc) }.cast::<u8>();
    if base.is_null() {
        return std::ptr::null_mut();
    }
    let len = u64::try_from(len).unwrap_or_else(|_| {
        panic!(
            "{}",
            bn_catalog::text("string", "msg.string_length_fits_u64")
        )
    });
    unsafe {
        base.cast::<StringHeader>().write(StringHeader {
            ref_count: AtomicU64::new(INITIAL_REF_COUNT),
            len,
        });
        let data_ptr = base.add(HEADER_SIZE);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), data_ptr, bytes.len());
        data_ptr.add(bytes.len()).write(0);
        LIVE_STRINGS.fetch_add(1, Ordering::Release);
        data_ptr.cast::<c_char>()
    }
}

/// Copies an operating-system C string (`argv`) into a STRING. The source is
/// not a STRING: it has no header, so the copy stops at its terminating NUL.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_cstr_copy(ptr: *const c_char) -> *mut c_char {
    if ptr.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: `ptr` is a NUL-terminated C string owned by the process image
    // (`main`'s argv), not a STRING payload.
    let bytes = unsafe { CStr::from_ptr(ptr) }.to_bytes();
    c_string_from_bytes(bytes)
}

/// The payload bytes of a STRING. The length is the header's, so a NUL inside
/// the value is data.
#[allow(unsafe_code)]
pub(crate) fn slice_from_raw<'a>(ptr: *const c_char) -> Option<&'a [u8]> {
    let header = header_mut(ptr.cast_mut())?;
    let count = usize::try_from(header.len).ok()?;
    // SAFETY: G7-D1. The header's `len` is the number of payload bytes this
    // constructor, or the compiler's static header, wrote.
    Some(unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), count) })
}

/// The payload as an owned `String`, including a U+0000 when `len` says it is there.
pub(crate) fn string_text(ptr: *const c_char) -> Option<String> {
    let bytes = slice_from_raw(ptr)?;
    String::from_utf8(bytes.to_vec()).ok()
}

/// Encodes one Unicode scalar into a 64-bit integer in native byte order.
pub(crate) fn pack_utf8(character: char) -> u64 {
    let mut encoded = [0_u8; 4];
    let text = character.encode_utf8(&mut encoded);
    let mut bytes = [0_u8; size_of::<u64>()];
    bytes[..text.len()].copy_from_slice(text.as_bytes());
    u64::from_ne_bytes(bytes)
}

/// Allocates a new string from a single Unicode scalar code point.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_from_char(code: i64) -> *mut c_char {
    let Some(character) = u32::try_from(code).ok().and_then(char::from_u32) else {
        return std::ptr::null_mut();
    };
    let mut buf = [0_u8; 4];
    let encoded = character.encode_utf8(&mut buf);
    c_string_from_bytes(encoded.as_bytes())
}

/// A STRING of one scalar packed by [`pack_utf8`]. Zero is U+0000, one byte,
/// because a packed scalar is never empty.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_from_packed(packed: u64) -> *mut c_char {
    if packed == 0 {
        return c_string_from_bytes(&[0]);
    }
    let bytes = packed.to_ne_bytes();
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    c_string_from_bytes(&bytes[..end])
}

/// Concatenates two native strings `(left_ptr, left_len)` and `(right_ptr, right_len)`.
/// Returns an owned buffer holding `left + right` with a trailing NUL byte.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_concat(
    left_ptr: *const c_char,
    _left_len: i64,
    right_ptr: *const c_char,
    _right_len: i64,
) -> *mut c_char {
    let left = slice_from_raw(left_ptr).unwrap_or(&[]);
    let right = slice_from_raw(right_ptr).unwrap_or(&[]);
    let mut combined = Vec::with_capacity(left.len() + right.len());
    combined.extend_from_slice(left);
    combined.extend_from_slice(right);
    c_string_from_bytes(&combined)
}

/// Orders two strings by Unicode scalar (`-1`, `0`, `1`). Valid UTF-8 byte
/// order is scalar order (0.6.md, STRING comparisons, A5).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_cmp(left: *const c_char, right: *const c_char) -> i32 {
    if left == right {
        return 0;
    }
    // `slice_from_raw` reads only the header length that precedes a STRING
    // payload (G7-D1). A null pointer is an empty string.
    let left = slice_from_raw(left).unwrap_or(&[]);
    let right = slice_from_raw(right).unwrap_or(&[]);
    match left.cmp(right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

/// Returns equality of two strings (1 if equal, 0 if not).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_eq(left: *const c_char, right: *const c_char) -> i32 {
    if left == right {
        return 1;
    }
    match (slice_from_raw(left), slice_from_raw(right)) {
        (Some(l), Some(r)) => i32::from(l == r),
        _ => 0,
    }
}

/// Returns the number of Unicode scalar values in the string.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_len(text: *const c_char) -> i32 {
    let Some(bytes) = slice_from_raw(text) else {
        return 0;
    };
    match std::str::from_utf8(bytes) {
        Ok(valid) => i32::try_from(valid.chars().count()).unwrap_or(i32::MAX),
        Err(_) => 0,
    }
}

/// Returns the Unicode scalar value of the first character, or -1 if empty or invalid.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_asc(text: *const c_char) -> i64 {
    let Some(bytes) = slice_from_raw(text) else {
        return -1;
    };
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.chars().next())
        .map_or(-1, |character| i64::from(u32::from(character)))
}

/// Unicode lowercase (full Unicode case mapping). Returns a freshly allocated
/// NUL-terminated UTF-8 string buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_to_lower(text: *const c_char) -> *mut c_char {
    let Some(bytes) = slice_from_raw(text) else {
        return c_string_from_bytes(b"");
    };
    let lowered = std::str::from_utf8(bytes).map_or_else(|_| String::new(), str::to_lowercase);
    c_string_from_bytes(lowered.as_bytes())
}

/// Unicode uppercase (full Unicode case mapping). Returns a freshly allocated
/// NUL-terminated UTF-8 string buffer.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_to_upper(text: *const c_char) -> *mut c_char {
    let Some(bytes) = slice_from_raw(text) else {
        return c_string_from_bytes(b"");
    };
    let uppered = std::str::from_utf8(bytes).map_or_else(|_| String::new(), str::to_uppercase);
    c_string_from_bytes(uppered.as_bytes())
}

/// Returns one UTF-8 scalar packed in native byte order. U+0000 is allowed.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_char_utf8(code: i64) -> u64 {
    let Some(character) = u32::try_from(code).ok().and_then(char::from_u32) else {
        return u64::MAX;
    };
    pack_utf8(character)
}

/// Returns `STRING[index]` as one NUL-terminated UTF-8 scalar packed in native byte order.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_str_index_utf8(
    text: *const c_char,
    index: i32,
    trap: *const c_char,
) -> u64 {
    let bytes = slice_from_raw(text).unwrap_or_default();
    let valid_text = std::str::from_utf8(bytes).unwrap_or_default();
    let character = usize::try_from(index)
        .ok()
        .and_then(|position| valid_text.chars().nth(position));
    let Some(character) = character else {
        let length = i128::try_from(valid_text.chars().count()).unwrap_or(i128::MAX);
        trap_abi::report(trap, [index.into(), length]);
        std::process::exit(1);
    };
    pack_utf8(character)
}

/// Emits the exact UTF-8 bytes of a string to stdout without terminating early at NUL.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_print_str(text: *const c_char, _len: i64) {
    if let Some(bytes) = slice_from_raw(text) {
        let _ = LibcStdout.write_all(bytes);
    }
}

/// One line from stdin, without its trailing newline. Null means end of input.
/// `reusable` is unused: the caller releases the previous buffer (G7-D5).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_input_line(_reusable: *mut c_char) -> *mut c_char {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => std::ptr::null_mut(),
        Ok(_) => {
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            c_string_from_bytes(line.as_bytes())
        }
    }
}

#[cfg(test)]
#[path = "string_tests.rs"]
mod reexec;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_operations_with_embedded_nul() {
        let ptr = c_string_from_bytes(b"abc\0");
        assert_eq!(bn_rt_str_len(ptr), 4);
        assert_eq!(bn_rt_str_asc(ptr), i64::from(u32::from('a')));

        let packed_zero = bn_rt_str_char_utf8(0);
        assert_ne!(packed_zero, u64::MAX);
        let zero = bn_rt_str_from_packed(0);
        assert_eq!(bn_rt_str_len(zero), 1);
        assert_eq!(bn_rt_str_release(zero), 0);
        let letter = bn_rt_str_from_packed(bn_rt_str_char_utf8(i64::from(u32::from('a'))));
        assert_eq!(bn_rt_str_len(letter), 1);
        assert_eq!(bn_rt_str_asc(letter), i64::from(u32::from('a')));
        assert_eq!(bn_rt_str_release(letter), 0);

        let concat_ptr = bn_rt_str_concat(ptr, 4, ptr, 4);
        assert_eq!(bn_rt_str_len(concat_ptr), 8);
        assert_eq!(bn_rt_str_eq(ptr, ptr), 1);
        assert_eq!(bn_rt_str_eq(ptr, concat_ptr), 0);
        assert_eq!(bn_rt_str_release(concat_ptr), 0);
        assert_eq!(bn_rt_str_release(ptr), 0);
    }

    #[test]
    fn a_payload_that_looks_like_the_old_magic_keeps_its_length() {
        let ptr = c_string_from_bytes(b"GNIRTSNB");
        assert_eq!(bn_rt_str_len(ptr), 8);
        assert_eq!(bn_rt_str_release(ptr), 0);
    }

    #[test]
    fn immortal_retain_and_release_do_not_free() {
        let ptr = c_string_from_bytes(b"static");
        let header = header_mut(ptr).expect("header");
        header
            .ref_count
            .store(IMMORTAL_REF_COUNT, Ordering::Relaxed);
        assert_eq!(bn_rt_str_retain(ptr), 0);
        assert_eq!(bn_rt_str_release(ptr), 0);
        assert_eq!(bn_rt_str_len(ptr), 6);
        header.ref_count.store(INITIAL_REF_COUNT, Ordering::Relaxed);
        assert_eq!(bn_rt_str_release(ptr), 0);
    }

    #[test]
    fn string_lifecycle_and_nul_retention() {
        let nul_str = b"a\0b";
        let ptr = c_string_from_bytes(nul_str);
        assert_eq!(bn_rt_str_len(ptr), 3);
        assert_eq!(bn_rt_str_retain(ptr), 0);
        assert_eq!(bn_rt_str_release(ptr), 0);
        // Still alive because of retain
        assert_eq!(bn_rt_str_len(ptr), 3);
        assert_eq!(bn_rt_str_release(ptr), 0);
        // Released now
    }

    #[test]
    fn stress_string_allocation_and_release_lifecycle() {
        for _ in 0..100_000 {
            let s = c_string_from_bytes(b"a\0b");
            assert_eq!(bn_rt_str_len(s), 3);
            assert_eq!(bn_rt_str_release(s), 0);
        }
    }

    #[test]
    fn release_to_zero_returns_the_live_count() {
        const CHILD: &str = "BN_STRING_LIVE_COUNT_CHILD";
        const NAME: &str = "string::tests::release_to_zero_returns_the_live_count";
        if std::env::var_os(CHILD).is_none() {
            let result = super::reexec::run_exact_child(NAME, CHILD);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let before = live_string_count();
        let ptr = c_string_from_bytes(b"z");
        assert_eq!(live_string_count(), before + 1);
        assert_eq!(bn_rt_str_release(ptr), 0);
        assert_eq!(live_string_count(), before);
    }

    #[test]
    fn a_hundred_thousand_copies_do_not_accumulate() {
        const CHILD: &str = "BN_STRING_STRESS_CHILD";
        const NAME: &str = "string::tests::a_hundred_thousand_copies_do_not_accumulate";
        if std::env::var_os(CHILD).is_none() {
            let result = super::reexec::run_exact_child(NAME, CHILD);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let before = live_string_count();
        for _ in 0..100_000 {
            let fresh = c_string_from_bytes(b"line!");
            assert_eq!(bn_rt_str_release(fresh), 0);
        }
        assert_eq!(live_string_count(), before);
    }

    #[test]
    fn concurrent_retain_and_release_ends_at_the_starting_count() {
        use std::sync::{Arc, Barrier};

        const CHILD: &str = "BN_STRING_CONCURRENT_CHILD";
        const NAME: &str =
            "string::tests::concurrent_retain_and_release_ends_at_the_starting_count";
        if std::env::var_os(CHILD).is_none() {
            let result = super::reexec::run_exact_child(NAME, CHILD);
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let before = live_string_count();
        let ptr = c_string_from_bytes(b"shared");
        // The payload address is shared. The header stays allocated until
        // every retain has a matching release, after both scopes join.
        let address = ptr as usize;
        let threads = 8;
        let barrier = Arc::new(Barrier::new(threads));
        std::thread::scope(|scope| {
            for _ in 0..threads {
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    assert_eq!(bn_rt_str_retain(address as *mut c_char), 0);
                });
            }
        });
        std::thread::scope(|scope| {
            for _ in 0..threads {
                scope.spawn(move || {
                    assert_eq!(bn_rt_str_release(address as *mut c_char), 0);
                });
            }
        });
        assert_eq!(bn_rt_str_len(ptr), 6);
        assert_eq!(bn_rt_str_release(ptr), 0);
        assert_eq!(live_string_count(), before);
    }

    #[test]
    fn release_of_a_zero_count_traps() {
        const CHILD: &str = "BN_STRING_RELEASE_ZERO_CHILD";
        const NAME: &str = "string::tests::release_of_a_zero_count_traps";
        if std::env::var_os(CHILD).is_none() {
            let result = super::reexec::run_exact_child(NAME, CHILD);
            assert!(!result.status.success());
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(
                stderr.contains("released after its count reached zero"),
                "{stderr}"
            );
            return;
        }
        let ptr = c_string_from_bytes(b"x");
        header_mut(ptr)
            .expect("header")
            .ref_count
            .store(0, Ordering::Relaxed);
        bn_rt_str_release(ptr);
    }

    #[test]
    fn retain_past_the_maximum_traps() {
        const CHILD: &str = "BN_STRING_RETAIN_OVERFLOW_CHILD";
        const NAME: &str = "string::tests::retain_past_the_maximum_traps";
        if std::env::var_os(CHILD).is_none() {
            let result = super::reexec::run_exact_child(NAME, CHILD);
            assert!(!result.status.success());
            let stderr = String::from_utf8_lossy(&result.stderr);
            assert!(
                stderr.contains("retain count exceeded the maximum"),
                "{stderr}"
            );
            return;
        }
        let ptr = c_string_from_bytes(b"x");
        header_mut(ptr)
            .expect("header")
            .ref_count
            .store(MAX_STRING_REF_COUNT, Ordering::Relaxed);
        bn_rt_str_retain(ptr);
    }
}
