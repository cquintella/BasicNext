// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `HOST.Random`'s generator, xorshift64* (host.md, "Random (0.2)"): the
//! one definition the interpreter and the native runtime both step.

/// The xorshift64* output multiplier.
const XORSHIFT64_STAR_MULTIPLIER: u64 = 0x2545_F491_4F6C_DD1D;

/// The bits of a `FLOAT` mantissa: the result is the top 53 bits of the
/// state over `2^53`, a value in `[0, 1)`.
const MANTISSA_BITS: u32 = 53;

/// The state `Seed(n)` sets: `n` as `INT64` reinterpreted as unsigned 64
/// bits, and `1` for `0` (the generator never leaves a zero state).
#[must_use]
pub const fn seed_state(seed: i64) -> u64 {
    let state = seed.cast_unsigned();
    if state == 0 { 1 } else { state }
}

/// The state after one step.
#[must_use]
pub const fn next_state(state: u64) -> u64 {
    let mut next = state;
    next ^= next >> 12;
    next ^= next << 25;
    next ^= next >> 27;
    next.wrapping_mul(XORSHIFT64_STAR_MULTIPLIER)
}

/// The `Random()` value of a stepped state: its top 53 bits over `2^53`.
#[must_use]
#[allow(clippy::cast_precision_loss)] // 53 bits are exact in a binary64 mantissa
pub fn unit_interval(state: u64) -> f64 {
    let top = state >> (u64::BITS - MANTISSA_BITS);
    top as f64 / (1_u64 << MANTISSA_BITS) as f64
}
