// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Pointer restoration rules between `POINTER TO VOID` and typed pointers/regions.

/// Possible outcomes when restoring a typed pointer or region from `POINTER TO VOID`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RestoreCheck {
    /// Types and lengths match; restore succeeds.
    Ok,
    /// Destination element type does not match the original element type.
    TypeMismatch,
    /// Destination fixed length does not match the original length.
    LengthMismatch,
}

/// Validates whether a `POINTER TO VOID` carrying `from_code` element code and `from_len` length
/// can be restored to a pointer/region expecting `to_code` and `to_len`.
///
/// If `to_len` is `None` (dynamic/unbounded region or single pointer without fixed constraint),
/// any non-negative `from_len` is accepted as long as element types match.
/// If `to_len` is `Some(expected)`, `from_len` must equal `expected`.
#[must_use]
pub fn check_restore(
    from_code: u32,
    from_len: u64,
    to_code: u32,
    to_len: Option<u64>,
) -> RestoreCheck {
    if from_code != to_code {
        return RestoreCheck::TypeMismatch;
    }
    if let Some(expected) = to_len
        && from_len != expected
    {
        return RestoreCheck::LengthMismatch;
    }
    RestoreCheck::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_matching_code_and_length() {
        assert_eq!(check_restore(0x04, 3, 0x04, Some(3)), RestoreCheck::Ok);
        assert_eq!(check_restore(0x0A, 5, 0x0A, None), RestoreCheck::Ok);
    }

    #[test]
    fn restore_different_code() {
        assert_eq!(
            check_restore(0x0A, 3, 0x04, Some(3)),
            RestoreCheck::TypeMismatch
        );
        assert_eq!(
            check_restore(0x01, 10, 0x02, None),
            RestoreCheck::TypeMismatch
        );
    }

    #[test]
    fn restore_different_length() {
        assert_eq!(
            check_restore(0x04, 3, 0x04, Some(4)),
            RestoreCheck::LengthMismatch
        );
        assert_eq!(
            check_restore(0x04, 0, 0x04, Some(1)),
            RestoreCheck::LengthMismatch
        );
    }

    #[test]
    fn restore_single_pointer() {
        assert_eq!(check_restore(0x04, 1, 0x04, Some(1)), RestoreCheck::Ok);
        assert_eq!(check_restore(0x04, 1, 0x04, None), RestoreCheck::Ok);
        assert_eq!(
            check_restore(0x04, 1, 0x04, Some(2)),
            RestoreCheck::LengthMismatch
        );
    }
}
