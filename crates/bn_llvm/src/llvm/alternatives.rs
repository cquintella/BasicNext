// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Native alternatives stored as `{ i1 error, ptr, i64 }`: the `NA` / `EOF`
// sentinels in the pointer field, `IS` tests on them, `IS T` on the value
// side of `T OR Error`, and loads of a STRING narrowed by `IS`.
#![allow(clippy::wildcard_imports)]
use super::*;

/// The pointer a `{ i1, ptr, i64 }` alternative stores for a value that is
/// neither the payload nor an `Error`: `NA` or `EOF`.
#[derive(Clone, Copy)]
pub(crate) enum Sentinel {
    NotAvailable,
    EndOfFile,
}

impl Sentinel {
    pub(crate) fn of(alternatives: &[Type]) -> Option<Self> {
        if string_na_or_error(alternatives) || scalar_na_or_error(alternatives) {
            Some(Self::NotAvailable)
        } else if string_eof_or_error(alternatives) || integer_eof_or_error(alternatives) {
            Some(Self::EndOfFile)
        } else {
            None
        }
    }

    pub(crate) const fn global(self) -> (&'static str, &'static str) {
        match self {
            Self::NotAvailable => ("@.bn_na", "[3 x i8]"),
            Self::EndOfFile => ("@.bn_eof", "[4 x i8]"),
        }
    }
}

/// `FS.File.ReadLine`: the string, the `@.bn_eof` sentinel, or an `Error`.
pub(crate) fn string_eof_or_error(alternatives: &[Type]) -> bool {
    alternatives.len() == 3
        && alternatives.iter().any(|ty| matches!(ty, Type::String))
        && alternatives.iter().any(|ty| matches!(ty, Type::EndOfFile))
        && alternatives.iter().any(is_error_type)
}

/// The type a `Load` of `stored` yields when the IR asks for `ty`: a slot
/// keeps its stored representation, except a STRING narrowed by `IS`.
pub(crate) fn loaded_type(stored: Option<&Type>, ty: &Type) -> Type {
    stored
        .filter(|stored| llvm_type(stored).is_some())
        .filter(|stored| llvm_type(stored) != llvm_type(ty) && !narrows_to_string(stored, ty))
        .cloned()
        .unwrap_or_else(|| ty.clone())
}

/// A `{ i1, ptr, i64 }` alternative holding STRING, loaded as STRING after an
/// `IS` narrowing: the string is the aggregate's pointer field.
pub(crate) fn narrows_to_string(stored: &Type, loaded: &Type) -> bool {
    *loaded == Type::String
        && llvm_type(stored) == Some("{ i1, ptr, i64 }")
        && matches!(stored, Type::Alternative(alternatives) if alternatives.contains(&Type::String))
}

/// Whether `IS test` names `value_ty`, the non-`Error` side of `T OR Error`.
pub(crate) fn alternative_is(value_ty: &Type, test: &str) -> bool {
    match value_ty {
        Type::Named(name) => name == test,
        _ => bn_types::scalar_test_type(test).as_ref() == Some(value_ty),
    }
}

/// Loads a STRING narrowed by `IS` from a `{ i1, ptr, i64 }` slot: the
/// string is the aggregate's pointer field.
pub(crate) fn emit_narrowed_string_load(text: &mut String, destination: ValueId, slot: usize) {
    let dest = destination.0;
    let _ = writeln!(
        text,
        "  %strload{dest} = load {{ i1, ptr, i64 }}, ptr %s{slot}"
    );
    let _ = writeln!(
        text,
        "  %v{dest} = extractvalue {{ i1, ptr, i64 }} %strload{dest}, 1"
    );
}

/// `IS` on an alternative with a sentinel (`STRING OR NA OR Error`,
/// `STRING OR EOF OR Error`, ...). Returns false when `left_ty` has none.
pub(crate) fn emit_sentinel_is(
    text: &mut String,
    destination: ValueId,
    left: ValueId,
    left_ty: &Type,
    right_ty: &Type,
    test_name: &str,
) -> bool {
    if let Type::Alternative(alternatives) = left_ty
        && let Some(sentinel) = Sentinel::of(alternatives)
    {
        let id = destination.0;
        let (global, array) = sentinel.global();
        let sentinel_name = match sentinel {
            Sentinel::NotAvailable => "NA",
            Sentinel::EndOfFile => "EOF",
        };
        let _ = writeln!(
            text,
            "  %cellerror{id} = extractvalue {{ i1, ptr, i64 }} %v{}, 0",
            left.0
        );
        let _ = writeln!(
            text,
            "  %cellptr{id} = extractvalue {{ i1, ptr, i64 }} %v{}, 1",
            left.0
        );
        let _ = writeln!(
            text,
            "  %cellnaptr{id} = getelementptr {array}, ptr {global}, i64 0, i64 0"
        );
        let _ = writeln!(
            text,
            "  %cellna{id} = icmp eq ptr %cellptr{id}, %cellnaptr{id}"
        );
        if test_name == "Error" {
            let _ = writeln!(text, "  %v{id} = or i1 false, %cellerror{id}");
        } else if test_name == sentinel_name
            || matches!(right_ty, Type::EndOfFile | Type::NotAvailable)
        {
            let _ = writeln!(text, "  %cellok{id} = xor i1 %cellerror{id}, true");
            let _ = writeln!(text, "  %v{id} = and i1 %cellok{id}, %cellna{id}");
        } else {
            let matches = alternatives
                .iter()
                .any(|ty| ty == right_ty || alternative_is(ty, test_name));
            let _ = writeln!(
                text,
                "  %cellabsent{id} = or i1 %cellerror{id}, %cellna{id}"
            );
            let _ = writeln!(text, "  %cellpresent{id} = xor i1 %cellabsent{id}, true");
            let _ = writeln!(
                text,
                "  %v{id} = and i1 %cellpresent{id}, {}",
                u8::from(matches)
            );
        }
        return true;
    }
    false
}
