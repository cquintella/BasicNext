#![allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn host_random_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let seed = u64::try_from(nanos).unwrap_or(u64::MAX) ^ u64::from(std::process::id());
    seed.max(1)
}
