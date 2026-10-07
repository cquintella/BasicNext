// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Wall-clock and monotonic time, shared by `HOST.Clock` on both backends.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Milliseconds since Unix epoch for an arbitrary `SystemTime`.
#[must_use]
pub fn timestamp_ms_from(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).unwrap_or(i64::MAX),
        Err(error) => i64::try_from(error.duration().as_millis()).map_or(i64::MIN, |value| -value),
    }
}

/// Milliseconds since Unix epoch for the current wall clock.
#[must_use]
pub fn timestamp_ms() -> i64 {
    timestamp_ms_from(SystemTime::now())
}

/// Nanoseconds since process start (saturating at `i64::MAX`).
#[must_use]
pub fn monotonic_ns() -> i64 {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let origin = *ORIGIN.get_or_init(Instant::now);
    i64::try_from(origin.elapsed().as_nanos()).unwrap_or(i64::MAX)
}
