// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If applicable, see the MPL-2.0 license.

//! The function-name protocol of the IR.
//!
//! Lowering spells synthesised functions by convention; since bucket 0.5.1c
//! §3.2 what a function *is* lives in `Function::kind` / `owner`, and this
//! table is informative for lowered functions. It stays load-bearing for
//! callees that have no body in the module — intrinsics and provider-backed
//! stubs (standard modules are not lowered) — which backends classify through
//! [`classify`]. Documented in `docs/architecture/ir-contract.md`.

/// What a function name says about the function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmittedNameKind {
    /// The program entry point.
    Entry,
    /// `<Class>.CONSTRUCTOR` — the class constructor body.
    Constructor,
    /// `<Class>.DESTRUCTOR` — the class destructor body.
    Destructor,
    /// `<Class>.$fields` — field-initialiser prologue run by `NEW`.
    FieldInit,
    /// `<Class>.$init` — construction helper (allocate + fields + constructor).
    Init,
    /// `<Struct>.$default` — value-type default constructor.
    Default,
    /// `<Class>.<Method>` or `<Module>.<Function>` — a user function or method.
    User,
}

/// Suffixes (after the last `.`) that name a synthesised function. Backends
/// may match these and nothing else.
pub const SYNTHESISED_SUFFIXES: &[(&str, EmittedNameKind)] = &[
    ("CONSTRUCTOR", EmittedNameKind::Constructor),
    ("DESTRUCTOR", EmittedNameKind::Destructor),
    ("$fields", EmittedNameKind::FieldInit),
    ("$init", EmittedNameKind::Init),
    ("$default", EmittedNameKind::Default),
];

/// Name of the entry function.
pub const ENTRY: &str = "Start";

/// Prefix of a super-call target (`@super:<Class>.<Method>`), resolved by the
/// backends to the base implementation, never dispatched dynamically.
pub const SUPER_PREFIX: &str = "@super:";

/// Prefix of an imported module namespace (`#<ModuleId>.<Name>`).
pub const MODULE_PREFIX: char = '#';

/// Intrinsic callees: names a `Call` may target that have **no** `Function`
/// body in the module. Every backend must implement each one natively; the
/// frontend must not emit a callee starting with `$` outside this list.
pub const INTRINSICS: &[&str] = &["$for_condition"];

/// Marker shared by synthesised suffixes and intrinsics.
pub const SYNTHESISED_MARKER: char = '$';

/// Whether a callee name is a documented intrinsic.
#[must_use]
pub fn is_intrinsic(name: &str) -> bool {
    INTRINSICS.contains(&name)
}

/// Classifies an emitted function name, or `None` when the name follows no
/// documented pattern (a lowering bug or an undocumented extension).
#[must_use]
pub fn classify(name: &str) -> Option<EmittedNameKind> {
    let name = name.strip_prefix(SUPER_PREFIX).unwrap_or(name);
    if name == ENTRY {
        return Some(EmittedNameKind::Entry);
    }
    let body = match name.strip_prefix(MODULE_PREFIX) {
        Some(rest) => {
            let (id, rest) = rest.split_once('.')?;
            if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            rest
        }
        None => name,
    };
    if body.is_empty() || body.starts_with('.') || body.ends_with('.') {
        return None;
    }
    let mut parts = body.split('.');
    let head = parts.next()?;
    let tail: Vec<&str> = parts.collect();
    if !is_identifier(head) {
        return None;
    }
    match tail.as_slice() {
        [] => Some(EmittedNameKind::User),
        [member] => {
            if let Some((_, kind)) = SYNTHESISED_SUFFIXES
                .iter()
                .find(|(suffix, _)| suffix == member)
            {
                return Some(*kind);
            }
            is_identifier(member).then_some(EmittedNameKind::User)
        }
        _ => None,
    }
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::{EmittedNameKind, classify};

    #[test]
    fn documented_shapes_classify() {
        assert_eq!(classify("Start"), Some(EmittedNameKind::Entry));
        assert_eq!(
            classify("Counter.CONSTRUCTOR"),
            Some(EmittedNameKind::Constructor)
        );
        assert_eq!(
            classify("Counter.DESTRUCTOR"),
            Some(EmittedNameKind::Destructor)
        );
        assert_eq!(
            classify("Counter.$fields"),
            Some(EmittedNameKind::FieldInit)
        );
        assert_eq!(classify("Counter.$init"), Some(EmittedNameKind::Init));
        assert_eq!(classify("Point.$default"), Some(EmittedNameKind::Default));
        assert_eq!(classify("Counter.Inc"), Some(EmittedNameKind::User));
        assert_eq!(classify("Helper"), Some(EmittedNameKind::User));
        assert_eq!(classify("#3.Greeting.Value"), Some(EmittedNameKind::User));
        assert_eq!(classify("#3.Box.$default"), Some(EmittedNameKind::Default));
        assert_eq!(classify("@super:Animal.Speak"), Some(EmittedNameKind::User));
        assert_eq!(
            classify("@super:Animal.DESTRUCTOR"),
            Some(EmittedNameKind::Destructor)
        );
    }

    #[test]
    fn intrinsics_are_callees_not_functions() {
        assert!(super::is_intrinsic("$for_condition"));
        assert!(!super::is_intrinsic("$for_step"));
        assert_eq!(
            classify("$for_condition"),
            None,
            "intrinsics never appear as functions"
        );
    }

    #[test]
    fn undocumented_shapes_are_rejected() {
        for bogus in [
            "",
            ".",
            "Counter.",
            ".Inc",
            "Counter.$bogus",
            "Counter.$fields.extra",
            "#x.Name",
            "#.Name",
            "3Name",
            "Counter.Inc.Twice",
            "$bogus_intrinsic",
        ] {
            assert_eq!(classify(bogus), None, "{bogus:?} must not classify");
        }
    }
}
