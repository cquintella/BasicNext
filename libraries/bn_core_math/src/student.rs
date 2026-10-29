// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Student's t (math.md, "Student's t"): the cumulative distribution, the
//! two-tailed p-value and the quantile.
//!
//! Every step is `+`, `−`, `×`, `÷` or a square root. Nothing here calls
//! the platform `libm`, so both backends and every IEEE-754 platform
//! produce the same bits. The cumulative value is the regularized
//! incomplete beta `I_x(df / 2, 1 / 2)` with `x = df / (df + t²)`.

// Sentinels (0, 1/2, 1, 2) are exact values. Loop indices stay below 2^53.
// The stored exponent is checked before it is written into a `FLOAT`.
#![allow(clippy::float_cmp)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]
#![allow(clippy::excessive_precision)]

use crate::binary64;

/// Odd powers of the artanh series of `ln`. `|z| ≤ 0.172`, and the wide sum
/// keeps terms through `z^61`, below the low word.
const LN_SERIES_TERMS: usize = 32;

/// Terms of `exp` on an argument already reduced to `|r| ≤ ln(2) / 2`.
const EXP_SERIES_TERMS: usize = 25;

/// Taylor terms of `erf` on `[0, 2]`.
const ERF_TAYLOR_TERMS: usize = 40;

/// Taylor terms of `sin(π z)` on `[0, π / 2]`.
const SIN_PI_TERMS: usize = 16;

/// Lentz steps of the incomplete-beta continued fraction (Numerical Recipes).
const BETA_FRACTION_STEPS: usize = 200;

/// Lentz steps of the `erfc` continued fraction for `x ≥ 2`.
const ERFC_FRACTION_STEPS: i32 = 60;

/// A Lentz step whose factor is this close to 1 has converged.
const FRACTION_TOLERANCE: f64 = 3.0 * f64::EPSILON;

/// A series term below this fraction of the sum, scaled by the distance of
/// the argument from 1, no longer changes the wide sum.
const SERIES_TOLERANCE: f64 = f64::EPSILON / 8.0;

/// The hypergeometric series is used while its argument is at most this.
/// Closer to one, the continued fraction converges and the series does not.
const SERIES_ARGUMENT_LIMIT: f64 = 0.975;

/// Cap on hypergeometric terms. Past this the fraction is used.
const SERIES_TERM_LIMIT: usize = 2_000;

/// `ln Γ(z)` lifts `z` to at least this argument before Stirling's series.
const STIRLING_ARGUMENT: f64 = 16.0;

/// Bisection steps of the quantile. Eighty halvings locate a `FLOAT`.
const QUANTILE_BISECTIONS: usize = 80;

/// `2^27 + 1`. Splits a factor into two `FLOAT`s whose product is exact.
const WIDE_SPLIT: f64 = 134_217_729.0;

/// The fraction treats a denominator this small as this value, so a step
/// cannot divide by zero.
const FRACTION_FLOOR: f64 = 1.0e-300;

/// `exp` at this argument overflows `FLOAT`.
const EXP_OVERFLOW: f64 = 709.782_712_893_384;

/// `exp` at this argument underflows to zero.
const EXP_UNDERFLOW: f64 = -745.133_219_101_941_2;

/// Cody–Waite / fdlibm pair for `ln(2)`. The high word ends in twelve zero
/// bits, so `n * LN_2_HIGH` is exact for every power `exp` uses, and the
/// sum of the pair is `ln(2)` past double precision.
const LN_2_HIGH: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
const LN_2_LOW: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);

/// `B_{2k} / (2k (2k − 1))` for `k = 1..=10`, Stirling's series for `ln Γ`.
const STIRLING_COEFFICIENTS: [f64; 10] = [
    1.0 / 12.0,
    -1.0 / 360.0,
    1.0 / 1_260.0,
    -1.0 / 1_680.0,
    1.0 / 1_188.0,
    -691.0 / 360_360.0,
    1.0 / 156.0,
    -3_617.0 / 122_400.0,
    43_867.0 / 244_188.0,
    -174_611.0 / 125_400.0,
];

/// `1 / ln(2)`, used only to choose the integer power in `exp`.
const INV_LN_2: f64 = std::f64::consts::LOG2_E;

/// Central and tail coefficients of Acklam's inverse normal. The
/// approximation starts the normal quantile; Halley then finishes it.
const NORMAL_CENTER_P: f64 = 0.02425;
const NORMAL_CENTER_NUMERATOR: [f64; 6] = [
    -3.969_683_028_665_376e1,
    2.209_460_984_245_205e2,
    -2.759_285_104_469_687e2,
    1.383_577_518_672_690e2,
    -3.066_479_806_614_716e1,
    2.506_628_277_459_239,
];
const NORMAL_CENTER_DENOMINATOR: [f64; 5] = [
    -5.447_609_879_822_406e1,
    1.615_858_368_580_409e2,
    -1.556_989_798_598_866e2,
    6.680_131_188_771_972e1,
    -1.328_068_155_288_572e1,
];
const NORMAL_TAIL_NUMERATOR: [f64; 6] = [
    -7.784_894_002_430_293e-3,
    -3.223_964_580_411_365e-1,
    -2.400_758_277_161_838,
    -2.549_732_539_343_734,
    4.374_664_141_464_968,
    2.938_163_982_698_783,
];
const NORMAL_TAIL_DENOMINATOR: [f64; 4] = [
    7.784_695_709_041_462e-3,
    3.224_671_290_700_398e-1,
    2.445_134_137_142_996,
    3.754_408_661_907_416,
];

fn pi() -> f64 {
    std::f64::consts::PI
}

/// A sum `high + low`. The low word holds bits a single `FLOAT` drops.
/// Subtracting `ln Γ` values needs those bits; a rounded `ln Γ` is not
/// enough for one unit in the last place of the beta.
#[derive(Clone, Copy)]
struct Wide {
    high: f64,
    low: f64,
}

fn wide(high: f64) -> Wide {
    Wide { high, low: 0.0 }
}

fn two_sum(left: f64, right: f64) -> Wide {
    let sum = left + right;
    let virtual_right = sum - left;
    let virtual_left = sum - virtual_right;
    Wide {
        high: sum,
        low: (left - virtual_left) + (right - virtual_right),
    }
}

fn split_factor(value: f64) -> (f64, f64) {
    let scaled = value * WIDE_SPLIT;
    let high = scaled - (scaled - value);
    (high, value - high)
}

fn exact_product(left: f64, right: f64) -> Wide {
    let product = left * right;
    if !product.is_finite() {
        return wide(product);
    }
    let (left_high, left_low) = split_factor(left);
    let (right_high, right_low) = split_factor(right);
    let error = left_high * right_high - product
        + left_high * right_low
        + left_low * right_high
        + left_low * right_low;
    Wide {
        high: product,
        low: error,
    }
}

fn add_wide(left: Wide, right: Wide) -> Wide {
    let sum = two_sum(left.high, right.high);
    let extra = two_sum(left.low, right.low);
    let merged = two_sum(sum.high, sum.low + extra.high);
    two_sum(merged.high, merged.low + extra.low)
}

fn sub_wide(left: Wide, right: Wide) -> Wide {
    add_wide(
        left,
        Wide {
            high: -right.high,
            low: -right.low,
        },
    )
}

fn mul_f64(value: Wide, factor: f64) -> Wide {
    let product = exact_product(value.high, factor);
    two_sum(product.high, product.low + value.low * factor)
}

fn div_f64(value: Wide, divisor: f64) -> Wide {
    let quotient = value.high / divisor;
    let residual = sub_wide(value, mul_f64(wide(quotient), divisor));
    two_sum(quotient, residual.high / divisor + residual.low / divisor)
}

fn mul_wide(left: Wide, right: Wide) -> Wide {
    let product = exact_product(left.high, right.high);
    two_sum(
        product.high,
        product.low + (left.high * right.low + left.low * right.high),
    )
}

fn rounded(value: Wide) -> f64 {
    two_sum(value.high, value.low).high
}

/// `ln`, reduced to the artanh series on `(√2 / 2, √2]`.
fn ln(value: f64) -> f64 {
    if value.is_nan() || value < 0.0 {
        return f64::NAN;
    }
    if value == 0.0 {
        return f64::NEG_INFINITY;
    }
    if value.is_infinite() {
        return f64::INFINITY;
    }
    rounded(ln_wide(wide(value)))
}

/// `ln(value)` with a low word. `value.high` is positive and finite.
fn ln_wide(value: Wide) -> Wide {
    let main = ln_reduced(value.high);
    if value.low == 0.0 {
        return main;
    }
    let slope = value.low / value.high;
    add_wide(
        main,
        Wide {
            high: slope,
            low: -0.5 * slope * slope,
        },
    )
}

fn ln_reduced(value: f64) -> Wide {
    let (mut magnitude, mut exponent) = binary64::decompose(value);
    if magnitude > std::f64::consts::SQRT_2 {
        magnitude *= 0.5;
        exponent += 1;
    }
    // `magnitude` is in `(1/√2, √2]`, so both factors are exact.
    let ratio = div_f64(wide(magnitude - 1.0), magnitude + 1.0);
    let square = mul_wide(ratio, ratio);
    let mut power = ratio;
    let mut sum = ratio;
    for term in 1..LN_SERIES_TERMS {
        power = mul_wide(power, square);
        sum = add_wide(sum, div_f64(power, (2 * term + 1) as f64));
    }
    let doubled = mul_f64(sum, 2.0);
    let power = exponent as f64;
    let scaled_ln2 = add_wide(wide(power * LN_2_HIGH), mul_f64(wide(LN_2_LOW), power));
    add_wide(doubled, scaled_ln2)
}

/// `exp`, reduced by the Cody–Waite split of `ln(2)`.
fn exp(value: f64) -> f64 {
    rounded(exp_wide(wide(value)))
}

fn exp_wide(value: Wide) -> Wide {
    if value.high.is_nan() {
        return wide(value.high);
    }
    if value.high > EXP_OVERFLOW {
        return wide(f64::INFINITY);
    }
    if value.high < EXP_UNDERFLOW {
        return wide(0.0);
    }
    let power = (value.high * INV_LN_2).round_ties_even();
    let scaled_ln2 = add_wide(wide(power * LN_2_HIGH), mul_f64(wide(LN_2_LOW), power));
    let reduced = sub_wide(value, scaled_ln2);
    let mut term = wide(1.0);
    let mut sum = term;
    for index in 1..EXP_SERIES_TERMS {
        term = div_f64(mul_wide(term, reduced), index as f64);
        sum = add_wide(sum, term);
    }
    scale_wide(sum, power as i64)
}

fn scale_wide(value: Wide, power: i64) -> Wide {
    two_sum(scale_pow2(value.high, power), scale_pow2(value.low, power))
}

/// `value · 2^power`, rounded once below the normal range.
fn scale_pow2(value: f64, power: i64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    let (magnitude, own_power) = binary64::decompose(value.abs());
    binary64::compose(magnitude, own_power + power).copysign(value)
}

/// `sin(π · fraction)` for the reflection formula of `ln Γ`.
fn sin_pi_wide(fraction: f64) -> Wide {
    let mut turns = fraction.rem_euclid(2.0);
    let mut negative = false;
    if turns > 1.0 {
        turns -= 1.0;
    }
    if turns > 0.5 {
        turns = 1.0 - turns;
        negative = !negative;
    }
    let angle = mul_f64(wide(pi()), turns);
    let square = mul_wide(angle, angle);
    let mut power = angle;
    let mut sum = angle;
    let mut factorial = 1.0;
    for index in 1..SIN_PI_TERMS {
        factorial *= (2 * index) as f64 * (2 * index + 1) as f64;
        power = mul_wide(power, square);
        power = Wide {
            high: -power.high,
            low: -power.low,
        };
        sum = add_wide(sum, div_f64(power, factorial));
    }
    if negative {
        Wide {
            high: -sum.high,
            low: -sum.low,
        }
    } else {
        sum
    }
}

/// `ln Γ(z)` for `z > 0`. Small arguments use the reflection formula or a
/// recurrence up to [`STIRLING_ARGUMENT`]; the series then runs in wide
/// precision so the incomplete-beta prefix keeps a low word.
fn ln_gamma_wide(z: f64) -> Wide {
    if z == 1.0 || z == 2.0 {
        return wide(0.0);
    }
    if z < 0.5 {
        let sine = sin_pi_wide(z);
        let reflected = sub_wide(ln_reduced(pi()), ln_wide(sine));
        return sub_wide(reflected, ln_gamma_wide(1.0 - z));
    }
    let mut dropped = wide(0.0);
    let mut current = z;
    while current < STIRLING_ARGUMENT {
        dropped = sub_wide(dropped, ln_reduced(current));
        current += 1.0;
    }
    add_wide(stirling(current), dropped)
}

fn stirling(z: f64) -> Wide {
    let ln_z = ln_reduced(z);
    let mut total = sub_wide(mul_f64(ln_z, z - 0.5), wide(z));
    total = add_wide(total, mul_f64(ln_reduced(2.0 * pi()), 0.5));
    let inverse = div_f64(wide(1.0), z);
    let inverse_square = mul_wide(inverse, inverse);
    let mut inverse_power = inverse;
    let mut terms = [wide(0.0); STIRLING_COEFFICIENTS.len()];
    for (index, coefficient) in STIRLING_COEFFICIENTS.iter().enumerate() {
        terms[index] = mul_f64(inverse_power, *coefficient);
        inverse_power = mul_wide(inverse_power, inverse_square);
    }
    let mut series = wide(0.0);
    for term in terms.iter().rev() {
        series = add_wide(series, *term);
    }
    add_wide(total, series)
}

fn guarded_denominator(value: f64) -> f64 {
    if value.abs() < FRACTION_FLOOR {
        FRACTION_FLOOR.copysign(value)
    } else {
        value
    }
}

/// Continued fraction for the incomplete beta (Lentz, Numerical Recipes).
fn beta_fraction(a: f64, b: f64, x: f64) -> f64 {
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut convergent = 1.0;
    let mut denominator = guarded_denominator(1.0 - qab * x / qap);
    denominator = 1.0 / denominator;
    let mut approximant = denominator;
    for step in 1..=BETA_FRACTION_STEPS {
        let twice = 2.0 * step as f64;
        let first = step as f64 * (b - step as f64) * x / ((qam + twice) * (a + twice));
        denominator = guarded_denominator(1.0 + first * denominator);
        convergent = guarded_denominator(1.0 + first / convergent);
        denominator = 1.0 / denominator;
        approximant *= denominator * convergent;
        let second = -(a + step as f64) * (qab + step as f64) * x / ((a + twice) * (qap + twice));
        denominator = guarded_denominator(1.0 + second * denominator);
        convergent = guarded_denominator(1.0 + second / convergent);
        denominator = 1.0 / denominator;
        let delta = denominator * convergent;
        approximant *= delta;
        if (delta - 1.0).abs() < FRACTION_TOLERANCE {
            break;
        }
    }
    approximant
}

/// Regularized incomplete beta `I_x(a, b)`.
///
/// The direct side is the hypergeometric series in wide precision. The
/// other side is `1` minus that series at `1 − x`, which is the smaller
/// argument. Very close to one, the continued fraction replaces the series.
fn regularized_beta(a: f64, b: f64, x: f64) -> f64 {
    if x.is_nan() || a <= 0.0 || b <= 0.0 || a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let direct = x < (a + 1.0) / (a + b + 2.0);
    let (left, right, argument) = if direct { (a, b, x) } else { (b, a, 1.0 - x) };
    let value = if argument <= SERIES_ARGUMENT_LIMIT {
        beta_series(left, right, argument)
    } else {
        beta_by_fraction(left, right, argument)
    };
    if !value.is_finite() {
        return f64::NAN;
    }
    if direct { value } else { 1.0 - value }
}

/// `I_x(a, b) = exp(a ln x − ln a + ln Γ(a+b) − ln Γ(a) − ln Γ(b)) · ₂F₁`.
fn beta_series(a: f64, b: f64, x: f64) -> f64 {
    beta_series_at(a, b, ln_reduced(x), x)
}

/// [`beta_series`] from `ln x`, for an `x` too small to hold its own bits.
fn beta_series_at(a: f64, b: f64, ln_x: Wide, x: f64) -> f64 {
    let mut prefix = mul_f64(ln_x, a);
    prefix = sub_wide(prefix, ln_reduced(a));
    prefix = add_wide(prefix, ln_gamma_wide(a + b));
    prefix = sub_wide(prefix, ln_gamma_wide(a));
    prefix = sub_wide(prefix, ln_gamma_wide(b));
    if !prefix.high.is_finite() {
        return f64::NAN;
    }
    rounded(mul_wide(
        exp_wide(prefix),
        hypergeometric(a, 1.0 - b, a + 1.0, x),
    ))
}

/// `₂F₁(a, b; c; x)`, term by term, each factor applied in wide precision.
fn hypergeometric(a: f64, b: f64, c: f64, x: f64) -> Wide {
    let mut term = wide(1.0);
    let mut sum = term;
    for step in 0..SERIES_TERM_LIMIT {
        let index = step as f64;
        term = mul_f64(term, a + index);
        term = mul_f64(term, b + index);
        term = mul_f64(term, x);
        term = div_f64(term, c + index);
        term = div_f64(term, index + 1.0);
        sum = add_wide(sum, term);
        let gap = (1.0 - x).abs().max(f64::EPSILON);
        if term.high.abs() <= sum.high.abs() * SERIES_TOLERANCE * gap {
            break;
        }
    }
    sum
}

fn beta_by_fraction(a: f64, b: f64, x: f64) -> f64 {
    let mut prefix = ln_gamma_wide(a + b);
    prefix = sub_wide(prefix, ln_gamma_wide(a));
    prefix = sub_wide(prefix, ln_gamma_wide(b));
    prefix = add_wide(prefix, mul_f64(ln_reduced(x), a));
    prefix = add_wide(prefix, mul_f64(ln_reduced(1.0 - x), b));
    if !prefix.high.is_finite() {
        return f64::NAN;
    }
    let front = div_f64(exp_wide(prefix), a);
    rounded(mul_f64(front, beta_fraction(a, b, x)))
}

/// `erf` by Taylor on `[0, 2]` and the complementary continued fraction
/// beyond that.
fn erf(value: f64) -> f64 {
    if value < 0.0 {
        return -erf(-value);
    }
    if value.is_infinite() {
        return 1.0;
    }
    if value < 2.0 {
        let square = value * value;
        let mut term = value;
        let mut sum = 0.0;
        for index in 0..ERF_TAYLOR_TERMS {
            sum += term / (2 * index + 1) as f64;
            term *= -square / (index + 1) as f64;
        }
        return sum * 2.0 / pi().sqrt();
    }
    1.0 - erfc_large(value)
}

/// `erfc` for `x ≥ 2`: `exp(-x²) / (x √π)` times Lentz's fraction
/// `1/(1 + v/(1 + 2v/(1 + …)))`, `v = 1/(2x²)`.
fn erfc_large(value: f64) -> f64 {
    let reciprocal_square = 0.5 / (value * value);
    let mut convergent = 1.0;
    let mut denominator = 1.0;
    let mut approximant = 1.0;
    for index in 1..ERFC_FRACTION_STEPS {
        let coefficient = f64::from(index) * reciprocal_square;
        denominator = guarded_denominator(1.0 + coefficient * denominator);
        convergent = guarded_denominator(1.0 + coefficient / convergent);
        denominator = 1.0 / denominator;
        let delta = denominator * convergent;
        approximant *= delta;
        if (delta - 1.0).abs() < FRACTION_TOLERANCE {
            break;
        }
    }
    exp(-value * value) / (value * pi().sqrt()) * (1.0 / approximant)
}

/// Standard normal cumulative value.
fn normal_cdf(value: f64) -> f64 {
    if value.is_nan() {
        return f64::NAN;
    }
    if value.is_infinite() {
        return if value.is_sign_positive() { 1.0 } else { 0.0 };
    }
    f64::midpoint(1.0, erf(value / std::f64::consts::SQRT_2))
}

fn horner(coefficients: &[f64], value: f64) -> f64 {
    coefficients
        .iter()
        .fold(0.0, |sum, coefficient| sum * value + coefficient)
}

/// Acklam's inverse normal, then one Halley step against [`normal_cdf`].
fn normal_quantile(probability: f64) -> f64 {
    let rough = if !(0.0..=1.0).contains(&probability) {
        f64::NAN
    } else if probability == 0.0 {
        f64::NEG_INFINITY
    } else if probability == 1.0 {
        f64::INFINITY
    } else if probability < NORMAL_CENTER_P {
        let q = (-2.0 * ln(probability)).sqrt();
        horner(&NORMAL_TAIL_NUMERATOR, q) / (horner(&NORMAL_TAIL_DENOMINATOR, q) * q + 1.0)
    } else if probability <= 1.0 - NORMAL_CENTER_P {
        let q = probability - 0.5;
        let radius = q * q;
        horner(&NORMAL_CENTER_NUMERATOR, radius) * q
            / (horner(&NORMAL_CENTER_DENOMINATOR, radius) * radius + 1.0)
    } else {
        let q = (-2.0 * ln(1.0 - probability)).sqrt();
        -(horner(&NORMAL_TAIL_NUMERATOR, q) / (horner(&NORMAL_TAIL_DENOMINATOR, q) * q + 1.0))
    };
    if !rough.is_finite() {
        return rough;
    }
    let error = normal_cdf(rough) - probability;
    let density = (1.0 / (2.0 * pi()).sqrt()) * exp(-0.5 * rough * rough);
    if density == 0.0 {
        return rough;
    }
    let u = error / density;
    rough - u / (1.0 + rough * u / 2.0)
}

fn normal_limit(df: f64) -> bool {
    df.is_infinite() && df.is_sign_positive()
}

/// Two-tailed p-value. `NAN` when `df` is not positive, except a positive
/// infinite `df`, which is the normal tail.
#[must_use]
pub fn student_t_pvalue(t: f64, df: f64) -> f64 {
    if t.is_nan() || df.is_nan() || (df <= 0.0 && !normal_limit(df)) {
        return f64::NAN;
    }
    if normal_limit(df) {
        return 2.0 * normal_cdf(-t.abs());
    }
    if t.is_infinite() {
        return 0.0;
    }
    let square = t * t;
    let x = df / (df + square);
    if !square.is_finite() || x < f64::MIN_POSITIVE {
        return far_tail_pvalue(t, df);
    }
    // `t²` has fallen off the magnitude of `df`, so the beta argument
    // rounds to 1 and the distribution is the normal limit (math.md).
    if x >= 1.0 {
        return if t == 0.0 {
            1.0
        } else {
            2.0 * normal_cdf(-t.abs())
        };
    }
    or_normal_limit(regularized_beta(df * 0.5, 0.5, x), t)
}

/// The p-value when `x = df / (df + t²)` is below the normal range or `t²`
/// overflows: `ln x = ln df − 2 ln|t| − ln(1 + df / t²)`, without forming
/// `t²`. A rounded `x` there would be `0` or lose its bits, and the p-value
/// is not `0` (for `df = 1` it is about `2 / (π |t|)`).
fn far_tail_pvalue(t: f64, df: f64) -> f64 {
    let magnitude = t.abs();
    let ratio = (df / magnitude) / magnitude;
    let ln_x = sub_wide(
        sub_wide(ln_reduced(df), mul_f64(ln_reduced(magnitude), 2.0)),
        ln_wide(two_sum(1.0, ratio)),
    );
    let x = rounded(exp_wide(ln_x));
    let a = df * 0.5;
    let beta = if x >= f64::MIN_POSITIVE {
        regularized_beta(a, 0.5, x)
    } else {
        beta_series_at(a, 0.5, ln_x, x)
    };
    or_normal_limit(beta, t)
}

/// A beta whose terms overflow is the normal limit of the distribution
/// (math.md, "Student's t").
fn or_normal_limit(beta: f64, t: f64) -> f64 {
    if beta.is_nan() {
        2.0 * normal_cdf(-t.abs())
    } else {
        beta
    }
}

/// Lower cumulative value, R's `pt(t, df)`.
#[must_use]
pub fn student_t_cdf(t: f64, df: f64) -> f64 {
    if t.is_nan() || df.is_nan() || (df <= 0.0 && !normal_limit(df)) {
        return f64::NAN;
    }
    if t.is_infinite() && df > 0.0 {
        return if t.is_sign_positive() { 1.0 } else { 0.0 };
    }
    if normal_limit(df) {
        return normal_cdf(t);
    }
    if t == 0.0 {
        return 0.5;
    }
    let tail = 0.5 * student_t_pvalue(t, df);
    if t > 0.0 { 1.0 - tail } else { tail }
}

/// Inverse of [`student_t_cdf`] on `(0, 1)`. `0` is `-INF`, `1` is `INF`,
/// `0.5` is `0.0` (math.md).
#[must_use]
pub fn student_t_quantile(probability: f64, df: f64) -> f64 {
    if probability.is_nan() || df.is_nan() || (df <= 0.0 && !normal_limit(df)) {
        return f64::NAN;
    }
    if !(0.0..=1.0).contains(&probability) {
        return f64::NAN;
    }
    if probability == 0.0 {
        return f64::NEG_INFINITY;
    }
    if probability == 1.0 {
        return f64::INFINITY;
    }
    if probability == 0.5 {
        return 0.0;
    }
    let upper = probability > 0.5;
    let target = if upper {
        probability
    } else {
        1.0 - probability
    };
    let magnitude = if normal_limit(df) {
        normal_quantile(target)
    } else {
        positive_quantile(target, df)
    };
    if upper { magnitude } else { -magnitude }
}

/// The positive `t` whose cumulative value is `target` in `(0.5, 1)`.
fn positive_quantile(target: f64, df: f64) -> f64 {
    // Grow until the bracket holds the target. With a small `df` the
    // quantile can exceed any fixed number of doublings; from at least 1 the
    // bound reaches `INF` after at most 1024 of them, and a quantile past the
    // largest `FLOAT` is `INF`.
    let mut high = normal_quantile(target).abs().max(1.0);
    while student_t_cdf(high, df) < target {
        high *= 2.0;
        if !high.is_finite() {
            return f64::INFINITY;
        }
    }
    let mut low = 0.0_f64;
    for _ in 0..QUANTILE_BISECTIONS {
        let mid = low.midpoint(high);
        if student_t_cdf(mid, df) < target {
            low = mid;
        } else {
            high = mid;
        }
    }
    high
}

#[cfg(test)]
mod elementary_agrees_with_the_platform_libm {
    //! The platform `libm` is only a check that the private elementary
    //! functions are accurate. Production code does not call it.

    use super::{erf, exp, ln, student_t_cdf};

    fn ln_gamma(z: f64) -> f64 {
        super::rounded(super::ln_gamma_wide(z))
    }

    fn ulps(left: f64, right: f64) -> i64 {
        if left.is_nan() && right.is_nan() {
            return 0;
        }
        if left.to_bits() == right.to_bits() {
            return 0;
        }
        i64::try_from(left.to_bits().abs_diff(right.to_bits())).unwrap_or(i64::MAX)
    }

    #[test]
    fn beta_identities_and_the_cauchy_closed_form() {
        // I_x(1, 1) = x, I_x(2, 1) = x², I_x(1, 2) = 2x − x².
        let cases = [
            (1.0, 1.0, 0.3, 0.3),
            (2.0, 1.0, 0.3, 0.09),
            (1.0, 2.0, 0.3, 2.0 * 0.3 - 0.3 * 0.3),
        ];
        for (a, b, x, expected) in cases {
            let actual = super::regularized_beta(a, b, x);
            assert!(ulps(actual, expected) <= 1, "I_{x}({a},{b})");
        }
        let x = 0.3_f64;
        let mut term = super::wide(1.0);
        let mut sum = term;
        for _ in 0..80 {
            term = super::mul_f64(term, x);
            sum = super::add_wide(sum, term);
        }
        assert_eq!(super::rounded(sum).to_bits(), (1.0 / (1.0 - x)).to_bits());
        // df = 1 is the Cauchy distribution: F(t) = 1/2 + arctan(t) / π.
        for t in [
            0.5_f64,
            1.0,
            3.077_683_537_175_253_4,
            12.706_204_736_174_705,
        ] {
            let reference = 0.5 + t.atan() / std::f64::consts::PI;
            assert!(ulps(student_t_cdf(t, 1.0), reference) <= 1, "cauchy {t}");
        }
    }

    #[test]
    fn ln_and_exp_stay_near_libm() {
        for index in 1..40 {
            let value = f64::from(index) * 0.17;
            assert!(ulps(ln(value), value.ln()) <= 1, "ln({value})");
            let argument = (f64::from(index) - 20.0) * 0.4;
            assert!(ulps(exp(argument), argument.exp()) <= 1, "exp({argument})");
        }
    }

    #[test]
    fn ln_gamma_matches_known_values() {
        assert!(ln_gamma(1.0).abs() < 1e-12, "{}", ln_gamma(1.0));
        assert_eq!(ln_gamma(2.0), 0.0);
        assert!(
            ulps(ln_gamma(0.5), std::f64::consts::PI.sqrt().ln()) <= 1,
            "{} vs {}",
            ln_gamma(0.5),
            std::f64::consts::PI.sqrt().ln()
        );
        assert!(ulps(erf(0.0), 0.0) < 2);
        assert!(ulps(erf(1.0), 0.842_700_792_949_714_8) < 16, "{}", erf(1.0));
    }
}
