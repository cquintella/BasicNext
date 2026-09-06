//! Versioned execution-policy ceiling for compiled HOST calls.
#![allow(unsafe_code)] // C ABI exports are the native policy boundary.

use std::sync::atomic::{AtomicU64, Ordering};

pub const POLICY_CLOCK: u64 = 1 << 0;
pub const POLICY_CONSOLE: u64 = 1 << 1;
pub const POLICY_FILESYSTEM: u64 = 1 << 2;
pub const POLICY_NET: u64 = 1 << 3;
pub const POLICY_DISPATCH: u64 = 1 << 4;
pub const POLICY_RANDOM: u64 = 1 << 5;
pub const POLICY_ALL: u64 = POLICY_CLOCK
    | POLICY_CONSOLE
    | POLICY_FILESYSTEM
    | POLICY_NET
    | POLICY_DISPATCH
    | POLICY_RANDOM;
pub const POLICY_VERSION: u32 = 1;
pub const POLICY_OK: i32 = 0;
pub const POLICY_INVALID: i32 = 2;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PolicyState {
    ceiling: u64,
    effective: u64,
}

#[cfg(test)]
impl PolicyState {
    const fn new() -> Self {
        Self {
            ceiling: POLICY_ALL,
            effective: POLICY_ALL,
        }
    }

    const fn install_ceiling(self, ceiling: u64) -> Self {
        let ceiling = self.ceiling & ceiling;
        Self {
            ceiling,
            effective: self.effective & ceiling,
        }
    }

    const fn restrict(self, mask: u64) -> Self {
        Self {
            ceiling: self.ceiling,
            effective: self.effective & mask & self.ceiling,
        }
    }
}

static CEILING: AtomicU64 = AtomicU64::new(POLICY_ALL);
static EFFECTIVE: AtomicU64 = AtomicU64::new(POLICY_ALL);

#[cfg(test)]
pub(crate) fn reset_for_tests() {
    CEILING.store(POLICY_ALL, Ordering::Release);
    EFFECTIVE.store(POLICY_ALL, Ordering::Release);
}

#[must_use]
pub(crate) fn allows(capability: u64) -> bool {
    EFFECTIVE.load(Ordering::Acquire) & capability == capability
}

/// Installs or narrows the artifact ceiling. Repeated calls can never widen it.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_init(version: u32, ceiling: u64) -> i32 {
    if version != POLICY_VERSION || ceiling & !POLICY_ALL != 0 {
        return POLICY_INVALID;
    }
    CEILING.fetch_and(ceiling, Ordering::AcqRel);
    let installed = CEILING.load(Ordering::Acquire);
    EFFECTIVE.fetch_and(installed, Ordering::AcqRel);
    POLICY_OK
}

/// Restricts the effective policy; bits outside the artifact ceiling are ignored.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_policy_restrict(mask: u64) -> i32 {
    if mask & !POLICY_ALL != 0 {
        return POLICY_INVALID;
    }
    let ceiling = CEILING.load(Ordering::Acquire);
    EFFECTIVE.fetch_and(mask & ceiling, Ordering::AcqRel);
    POLICY_OK
}

#[cfg(test)]
mod tests {
    use super::{POLICY_ALL, POLICY_CONSOLE, POLICY_INVALID, POLICY_VERSION, PolicyState};

    #[test]
    fn policy_masks_are_versioned_and_bounded() {
        assert_eq!(POLICY_VERSION, 1);
        assert_ne!(POLICY_ALL & POLICY_CONSOLE, 0);
        assert_eq!(
            super::bn_rt_policy_init(POLICY_VERSION, 1 << 63),
            POLICY_INVALID
        );
        assert_eq!(super::bn_rt_policy_restrict(1 << 63), POLICY_INVALID);
    }

    #[test]
    fn policy_intersection_cannot_widen_an_existing_restriction() {
        let state = PolicyState::new()
            .restrict(POLICY_CONSOLE)
            .install_ceiling(POLICY_ALL);
        assert_eq!(state.ceiling, POLICY_ALL);
        assert_eq!(state.effective, POLICY_CONSOLE);
        assert_eq!(state.restrict(POLICY_ALL).effective, POLICY_CONSOLE);
    }
}
