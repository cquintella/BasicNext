// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! CPU-specialized kernels for integer reduction operations (minimum, maximum).
//!
//! Conforming to AGENTS.md:
//! 1. No kernel changes an observable result. Only exact, order-independent
//!    operations qualify (integers, minimum/maximum). Floating-point
//!    sums remain scalar.
//! 2. `unsafe` is strictly confined to calling `target_feature` functions guarded
//!    by runtime CPU detection (`is_x86_feature_detected!`).

/// Scalar fallback for minimum of 32-bit signed integers.
#[inline]
#[must_use]
pub fn vmin_i32_scalar(values: &[i32]) -> Option<i32> {
    values.iter().copied().min()
}

/// Scalar fallback for maximum of 32-bit signed integers.
#[inline]
#[must_use]
pub fn vmax_i32_scalar(values: &[i32]) -> Option<i32> {
    values.iter().copied().max()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[allow(unsafe_code)]
unsafe fn vmin_i32_avx2(values: &[i32]) -> i32 {
    use std::arch::x86_64::{
        _mm256_loadu_si256, _mm256_min_epi32, _mm256_set1_epi32, _mm256_storeu_si256,
    };

    let mut i = 0;
    let len = values.len();
    // SAFETY: guarded by runtime CPU detection of AVX2 in caller.
    let mut min_vec = _mm256_set1_epi32(i32::MAX);

    while i + 8 <= len {
        // SAFETY: pointer offset is inbounds (i + 8 <= len) and cast is aligned for loadu.
        let chunk = unsafe { _mm256_loadu_si256(values.as_ptr().add(i).cast()) };
        min_vec = _mm256_min_epi32(min_vec, chunk);
        i += 8;
    }

    let mut buf = [0i32; 8];
    // SAFETY: buf is 8 i32s, storeu handles unaligned store into 32-byte buffer.
    unsafe { _mm256_storeu_si256(buf.as_mut_ptr().cast(), min_vec) };
    let mut acc = buf.iter().copied().min().unwrap_or(i32::MAX);

    while i < len {
        acc = acc.min(values[i]);
        i += 1;
    }

    acc
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[allow(unsafe_code)]
unsafe fn vmax_i32_avx2(values: &[i32]) -> i32 {
    use std::arch::x86_64::{
        _mm256_loadu_si256, _mm256_max_epi32, _mm256_set1_epi32, _mm256_storeu_si256,
    };

    let mut i = 0;
    let len = values.len();
    let mut max_vec = _mm256_set1_epi32(i32::MIN);

    while i + 8 <= len {
        // SAFETY: pointer offset is inbounds (i + 8 <= len) and cast is aligned for loadu.
        let chunk = unsafe { _mm256_loadu_si256(values.as_ptr().add(i).cast()) };
        max_vec = _mm256_max_epi32(max_vec, chunk);
        i += 8;
    }

    let mut buf = [0i32; 8];
    // SAFETY: buf is 8 i32s, storeu handles unaligned store into 32-byte buffer.
    unsafe { _mm256_storeu_si256(buf.as_mut_ptr().cast(), max_vec) };
    let mut acc = buf.iter().copied().max().unwrap_or(i32::MIN);

    while i < len {
        acc = acc.max(values[i]);
        i += 1;
    }

    acc
}

/// Finds the minimum element in a 32-bit signed integer slice using
/// hardware-accelerated SIMD instructions when available on the host CPU.
#[must_use]
pub fn vmin_i32(values: &[i32]) -> Option<i32> {
    if values.is_empty() {
        return None;
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && values.len() >= 16 {
            // SAFETY: guarded by runtime detection of AVX2 support.
            #[allow(unsafe_code)]
            return Some(unsafe { vmin_i32_avx2(values) });
        }
    }

    vmin_i32_scalar(values)
}

/// Finds the maximum element in a 32-bit signed integer slice using
/// hardware-accelerated SIMD instructions when available on the host CPU.
#[must_use]
pub fn vmax_i32(values: &[i32]) -> Option<i32> {
    if values.is_empty() {
        return None;
    }

    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && values.len() >= 16 {
            // SAFETY: guarded by runtime detection of AVX2 support.
            #[allow(unsafe_code)]
            return Some(unsafe { vmax_i32_avx2(values) });
        }
    }

    vmax_i32_scalar(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmin_i32_empty_returns_none() {
        assert_eq!(vmin_i32(&[]), None);
        assert_eq!(vmin_i32_scalar(&[]), None);
    }

    #[test]
    fn vmax_i32_empty_returns_none() {
        assert_eq!(vmax_i32(&[]), None);
        assert_eq!(vmax_i32_scalar(&[]), None);
    }

    #[test]
    fn vmin_i32_single_element() {
        assert_eq!(vmin_i32(&[42]), Some(42));
        assert_eq!(vmin_i32_scalar(&[42]), Some(42));
    }

    #[test]
    fn vmax_i32_single_element() {
        assert_eq!(vmax_i32(&[42]), Some(42));
        assert_eq!(vmax_i32_scalar(&[42]), Some(42));
    }

    #[test]
    fn vmin_vmax_i32_across_vector_lengths() {
        for len in [1i32, 2, 7, 8, 9, 15, 16, 17, 31, 32, 33, 100, 255, 1024] {
            let data: Vec<i32> = (0..len)
                .map(|i| if i % 2 == 0 { -i * 3 } else { i * 5 })
                .collect();
            let scalar_min = vmin_i32_scalar(&data);
            let dispatch_min = vmin_i32(&data);
            assert_eq!(
                dispatch_min, scalar_min,
                "mismatch at length {len} for vmin"
            );

            let scalar_max = vmax_i32_scalar(&data);
            let dispatch_max = vmax_i32(&data);
            assert_eq!(
                dispatch_max, scalar_max,
                "mismatch at length {len} for vmax"
            );
        }
    }

    #[test]
    fn vmin_vmax_i32_extremes() {
        let data = [i32::MAX, 0, -1, i32::MIN, 42, -9999];
        assert_eq!(vmin_i32(&data), Some(i32::MIN));
        assert_eq!(vmax_i32(&data), Some(i32::MAX));
        assert_eq!(vmin_i32_scalar(&data), Some(i32::MIN));
        assert_eq!(vmax_i32_scalar(&data), Some(i32::MAX));
    }

    /// Deterministic pseudo-random inputs (a fixed LCG, so a failure
    /// reproduces): every length up to 300 and the full `i32` range. On an
    /// x86-64 host with AVX2 this compares the specialised kernel with the
    /// scalar one; elsewhere both paths are scalar.
    #[test]
    fn vmin_vmax_i32_match_scalar_on_random_inputs() {
        let mut state: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            i32::from_ne_bytes(state.to_ne_bytes()[4..8].try_into().expect("4 bytes"))
        };
        for len in 0..300 {
            let data: Vec<i32> = (0..len).map(|_| next()).collect();
            assert_eq!(
                vmin_i32(&data),
                vmin_i32_scalar(&data),
                "vmin, length {len}"
            );
            assert_eq!(
                vmax_i32(&data),
                vmax_i32_scalar(&data),
                "vmax, length {len}"
            );
        }
    }

    /// Benchmark (D1 of bucket 0.6.5d), not a correctness test. Run in
    /// release on the machine being measured:
    /// `cargo test --release -p bn_rt cpu::tests::bench -- --ignored --nocapture`.
    /// Prints the median of 21 runs per size for the scalar kernel and for the
    /// dispatching one (AVX2 on x86-64 when detected; scalar elsewhere).
    #[test]
    #[ignore = "benchmark; run explicitly in release"]
    fn bench_vmin_vmax_i32() {
        use std::{hint::black_box, time::Instant};

        fn median(mut samples: Vec<u128>) -> u128 {
            samples.sort_unstable();
            samples[samples.len() / 2]
        }
        fn time(data: &[i32], kernel: fn(&[i32]) -> Option<i32>) -> u128 {
            median(
                (0..21)
                    .map(|_| {
                        let start = Instant::now();
                        black_box(kernel(black_box(data)));
                        start.elapsed().as_nanos()
                    })
                    .collect(),
            )
        }
        #[cfg(target_arch = "x86_64")]
        let avx2 = is_x86_feature_detected!("avx2");
        #[cfg(not(target_arch = "x86_64"))]
        let avx2 = false;
        println!(
            "arch={} avx2={avx2} (median ns of 21 runs)",
            std::env::consts::ARCH
        );
        println!(
            "{:>9} {:>12} {:>12} {:>12} {:>12}",
            "len", "min scalar", "min dispatch", "max scalar", "max dispatch"
        );
        for len in [16, 1_000, 10_000, 100_000, 1_000_000] {
            let data: Vec<i32> = (0..len)
                .map(|i: i32| i.wrapping_mul(2_654_435_761_u32.cast_signed()))
                .collect();
            println!(
                "{len:>9} {:>12} {:>12} {:>12} {:>12}",
                time(&data, vmin_i32_scalar),
                time(&data, vmin_i32),
                time(&data, vmax_i32_scalar),
                time(&data, vmax_i32),
            );
        }
    }
}
