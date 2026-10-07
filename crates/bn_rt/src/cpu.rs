// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! CPU-specialized kernels for integer reduction and search operations.
//!
//! Conforming to AGENTS.md:
//! 1. No kernel changes an observable result. Only exact, order-independent
//!    operations qualify (integers, minimum/maximum, searches). Floating-point
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

/// Scalar fallback for linear search in a 32-bit signed integer slice.
#[inline]
#[must_use]
pub fn find_i32_scalar(values: &[i32], target: i32) -> Option<usize> {
    values.iter().position(|&v| v == target)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn vmin_i32_avx2(values: &[i32]) -> i32 {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let mut i = 0;
    let len = values.len();
    let mut min_vec = _mm256_set1_epi32(i32::MAX);

    while i + 8 <= len {
        let chunk = _mm256_loadu_si256(values.as_ptr().add(i).cast());
        min_vec = _mm256_min_epi32(min_vec, chunk);
        i += 8;
    }

    let mut buf = [0i32; 8];
    _mm256_storeu_si256(buf.as_mut_ptr().cast(), min_vec);
    let mut acc = buf.iter().copied().min().unwrap_or(i32::MAX);

    while i < len {
        acc = acc.min(values[i]);
        i += 1;
    }

    acc
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn vmax_i32_avx2(values: &[i32]) -> i32 {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    let mut i = 0;
    let len = values.len();
    let mut max_vec = _mm256_set1_epi32(i32::MIN);

    while i + 8 <= len {
        let chunk = _mm256_loadu_si256(values.as_ptr().add(i).cast());
        max_vec = _mm256_max_epi32(max_vec, chunk);
        i += 8;
    }

    let mut buf = [0i32; 8];
    _mm256_storeu_si256(buf.as_mut_ptr().cast(), max_vec);
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

/// Finds the index of the target element in a 32-bit signed integer slice.
#[must_use]
pub fn find_i32(values: &[i32], target: i32) -> Option<usize> {
    find_i32_scalar(values, target)
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

    #[test]
    fn find_i32_behavior() {
        let data = [10, 20, 30, 40, 50];
        assert_eq!(find_i32(&data, 30), Some(2));
        assert_eq!(find_i32(&data, 99), None);
        assert_eq!(find_i32(&[], 10), None);
    }
}
