// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Pointer restoration rules between `POINTER TO VOID` and typed pointers/regions,
//! and the `IS POINTER TO T` test (0.6.md, BDFL decision 2026-10-16): the
//! allocation's element and, when the test names a length, that length.

use crate::{PointerLength, Type};

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

/// An `IS POINTER TO …` test. `void` is `IS POINTER TO VOID` (every non-`NULL`
/// pointer). Otherwise `element` and `length` are what the allocation must hold
/// for the test to be true, the same facts a conversion back from
/// `POINTER TO VOID` checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerIsTest {
    pub void: bool,
    pub element: Type,
    pub length: PointerLength,
}

/// Parses `test` as an `IS POINTER TO` name. Lowering spells brackets as
/// `LeftBracket` / `RightBracket` (`type_test_name` joins token texts).
#[must_use]
pub fn parse_pointer_is(test: &str) -> Option<PointerIsTest> {
    let compact = compact_pointer_test(test);
    let rest = compact
        .strip_prefix("POINTER")
        .and_then(|rest| rest.strip_prefix(" TO "))?;
    if rest.is_empty() {
        return None;
    }
    if rest == "VOID" {
        return Some(PointerIsTest {
            void: true,
            element: Type::Named("VOID".into()),
            length: PointerLength::Dynamic,
        });
    }
    let (name, length) = if let Some(name) = rest.strip_suffix("[]") {
        (name, PointerLength::Dynamic)
    } else if let Some((name, bracket)) = rest.rsplit_once('[')
        && let Some(number) = bracket.strip_suffix(']')
        && !name.is_empty()
        && !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
    {
        (name, PointerLength::Fixed(number.parse().ok()?))
    } else if rest.contains('[') {
        return None;
    } else {
        (rest, PointerLength::One)
    };
    if name.is_empty() {
        return None;
    }
    let element = crate::scalar_test_type(name).unwrap_or_else(|| Type::Named(name.to_string()));
    Some(PointerIsTest {
        void: false,
        element,
        length,
    })
}

/// `POINTER TO INTEGER LeftBracket 3 RightBracket` and
/// `POINTER TO INTEGER [ 3 ]` both become `POINTER TO INTEGER[3]`.
fn compact_pointer_test(test: &str) -> String {
    let spelled = test
        .replace("LeftBracket", "[")
        .replace("RightBracket", "]");
    let chars: Vec<char> = spelled.chars().collect();
    let mut compact = String::new();
    for (index, character) in chars.iter().copied().enumerate() {
        if character.is_whitespace() {
            let previous = chars[..index]
                .iter()
                .rev()
                .find(|item| !item.is_whitespace());
            let next = chars[index + 1..]
                .iter()
                .copied()
                .find(|item| !item.is_whitespace());
            if matches!(previous, Some('[' | ']')) || matches!(next, Some('[' | ']')) {
                continue;
            }
            if !compact.ends_with(' ') {
                compact.push(' ');
            }
            continue;
        }
        compact.push(character);
    }
    compact
}

/// Whether an allocation with `from_code` and `from_len` satisfies `test`.
/// `None` means `test` is not a pointer test. A `NULL` value is the caller's
/// concern: this answers only for a live allocation. `IS POINTER TO VOID` is
/// `Some(true)`.
#[must_use]
pub fn pointer_is_holds(from_code: u32, from_len: u64, test: &str) -> Option<bool> {
    let spec = parse_pointer_is(test)?;
    if spec.void {
        return Some(true);
    }
    let to_code = crate::alternatives::member_code(&spec.element).unwrap_or(u32::MAX);
    let to_len = match spec.length {
        PointerLength::Fixed(length) => Some(length),
        PointerLength::One => Some(1),
        PointerLength::Dynamic => None,
    };
    Some(check_restore(from_code, from_len, to_code, to_len) == RestoreCheck::Ok)
}

/// Whether `member` is the pointer type `test` names, including its length.
#[must_use]
pub fn pointer_member_matches(member: &Type, test: &str) -> bool {
    let Type::Pointer { element, length } = member else {
        return false;
    };
    let Some(spec) = parse_pointer_is(test) else {
        return false;
    };
    if spec.void {
        return matches!(element.as_ref(), Type::Named(name) if name == "VOID");
    }
    *length == spec.length && elements_match(element, &spec)
}

/// Whether `member`'s element is the element `test` names, ignoring length.
/// `IS POINTER TO VOID` is not an element match: it is every non-`NULL`
/// pointer, and the caller decides that.
#[must_use]
pub fn pointer_element_matches(member: &Type, test: &str) -> bool {
    let Type::Pointer { element, .. } = member else {
        return false;
    };
    let Some(spec) = parse_pointer_is(test) else {
        return false;
    };
    !spec.void && elements_match(element, &spec)
}

/// An imported element is tested by the name lowering emits (`Logger` or
/// `#<module>.Logger`), not by `Type` equality with `Named("Log.Logger")`.
fn elements_match(element: &Type, spec: &PointerIsTest) -> bool {
    if element == &spec.element {
        return true;
    }
    let test_name = match &spec.element {
        Type::Named(name) | Type::TypeName(name) => name.as_str(),
        _ => return false,
    };
    match element {
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            crate::import_test_matches(*module, name, test_name)
        }
        _ => false,
    }
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

    #[test]
    fn pointer_is_reads_the_allocation() {
        assert_eq!(pointer_is_holds(0x04, 3, "POINTER TO VOID"), Some(true));
        assert_eq!(
            pointer_is_holds(0x04, 3, "POINTER TO INTEGER LeftBracket RightBracket"),
            Some(true)
        );
        assert_eq!(
            pointer_is_holds(0x04, 3, "POINTER TO INTEGER LeftBracket 3 RightBracket"),
            Some(true)
        );
        assert_eq!(
            pointer_is_holds(0x04, 3, "POINTER TO INTEGER [ 3 ]"),
            Some(true)
        );
        assert_eq!(
            pointer_is_holds(0x0A, 3, "POINTER TO INTEGER[]"),
            Some(false)
        );
        assert_eq!(pointer_is_holds(0x04, 3, "POINTER TO INTEGER"), Some(false));
        assert_eq!(pointer_is_holds(0x04, 3, "INTEGER"), None);
    }

    #[test]
    fn imported_pointer_member_matches_the_emitted_name() {
        let member = Type::Pointer {
            element: Box::new(Type::ImportedNamed {
                module: crate::ModuleId(0),
                name: "Logger".into(),
            }),
            length: PointerLength::One,
        };
        assert!(pointer_member_matches(&member, "POINTER TO Logger"));
        assert!(pointer_member_matches(&member, "POINTER TO #0.Logger"));
        assert!(!pointer_member_matches(&member, "POINTER TO Log.Logger"));
        assert!(!pointer_member_matches(&member, "POINTER TO Foo.Logger"));
    }

    #[test]
    fn region_element_match_ignores_length() {
        let member = Type::Pointer {
            element: Box::new(Type::Integer(crate::IntegerType::Int32)),
            length: PointerLength::Dynamic,
        };
        assert!(pointer_element_matches(&member, "POINTER TO INTEGER[3]"));
        assert!(pointer_element_matches(&member, "POINTER TO INTEGER[]"));
        assert!(pointer_element_matches(&member, "POINTER TO INTEGER[0]"));
        assert!(!pointer_member_matches(&member, "POINTER TO INTEGER[3]"));
        assert!(pointer_member_matches(&member, "POINTER TO INTEGER[]"));
        assert!(!pointer_element_matches(&member, "POINTER TO FLOAT64[]"));
        assert!(!pointer_element_matches(&member, "POINTER TO VOID"));
    }
}
