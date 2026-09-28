// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native runtime diagnostics (bucket 0.6.2b R6). `bnc` renders each trap
//! site's diagnostic from the shared catalog at compile time; the runtime
//! only fills the facts known when the trap fires and prints the text. No
//! catalog text lives here.

use std::cell::RefCell;
use std::ffi::{CStr, c_char};
use std::io::Write as _;

/// A failure inside a runtime function: its registry code and named facts
/// (the identity's schema arguments). `bni` builds its diagnostic from the
/// same record; natively the calling site prints it
/// (`bn_rt_trap_report_failure`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeFailure {
    pub code: &'static str,
    pub facts: Vec<(&'static str, String)>,
}

thread_local! {
    static LAST_FAILURE: RefCell<Option<RuntimeFailure>> = const { RefCell::new(None) };
}

/// Records `failure` for the calling site to report.
pub(crate) fn record_failure(failure: RuntimeFailure) {
    LAST_FAILURE.with(|last| *last.borrow_mut() = Some(failure));
}

/// Opens a named slot in rendered text: `\u{1}name\u{2}`.
const NAMED_END: char = '\u{2}';
/// Separates the entries of a site's set; an entry is `CODE\u{1e}text`.
const ENTRY: char = '\u{1d}';
const CODE_END: char = '\u{1e}';

/// Prints the recorded failure with the calling site's diagnostic for its
/// code (`set`: the texts `bnc` rendered for each identity the call can
/// raise, with named slots), or `error[CODE]: facts` when the set has none.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_trap_report_failure(set: *const c_char) {
    let Some(failure) = LAST_FAILURE.with(|last| last.borrow_mut().take()) else {
        return;
    };
    let set = if set.is_null() {
        String::new()
    } else {
        // SAFETY: emitted code passes a NUL-terminated constant.
        unsafe { CStr::from_ptr(set) }
            .to_string_lossy()
            .into_owned()
    };
    let text = set
        .split(ENTRY)
        .find_map(|entry| {
            let (code, text) = entry.split_once(CODE_END)?;
            (code == failure.code).then(|| fill_named(text, &failure.facts))
        })
        .unwrap_or_else(|| {
            let facts = failure
                .facts
                .iter()
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            format!("error[{}]: {facts}", failure.code)
        });
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{text}");
}

/// `text` with each named slot replaced by its fact.
fn fill_named(text: &str, facts: &[(&'static str, String)]) -> String {
    let mut filled = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(SLOT) {
        filled.push_str(&rest[..start]);
        let after = &rest[start + SLOT.len_utf8()..];
        let Some(end) = after.find(NAMED_END) else {
            filled.push_str(after);
            return filled;
        };
        let name = &after[..end];
        if let Some((_, value)) = facts.iter().find(|(fact, _)| *fact == name) {
            filled.push_str(value);
        }
        rest = &after[end + NAMED_END.len_utf8()..];
    }
    filled.push_str(rest);
    filled
}

/// Marks runtime fact `n` in rendered text: `\u{1}` followed by `0` or `1`.
const SLOT: char = '\u{1}';

/// `text` with each slot replaced by its fact.
fn fill(text: &str, facts: [i128; 2]) -> String {
    let mut filled = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != SLOT {
            filled.push(character);
            continue;
        }
        match characters.next() {
            Some('0') => filled.push_str(&facts[0].to_string()),
            Some('1') => filled.push_str(&facts[1].to_string()),
            Some(other) => filled.push(other),
            None => {}
        }
    }
    filled
}

/// Prints the diagnostic of a trap site to standard error. The emitted code
/// then leaves through its trap exit (status 1), as `bni` does.
#[allow(unsafe_code)] // C ABI export.
#[unsafe(no_mangle)]
pub extern "C" fn bn_rt_trap_report(text: *const c_char, first: i128, second: i128) {
    if text.is_null() {
        return;
    }
    // SAFETY: emitted code passes a NUL-terminated constant.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{}", fill(&text, [first, second]));
}

#[cfg(test)]
mod tests {
    use super::fill;

    #[test]
    fn named_slots_take_the_recorded_facts() {
        assert_eq!(
            super::fill_named(
                "Index \u{1}index\u{2} is outside \u{1}context\u{2}.",
                &[("index", "(0, 1)".into()), ("context", "the window".into())]
            ),
            "Index (0, 1) is outside the window."
        );
    }

    #[test]
    fn slots_take_the_runtime_facts() {
        assert_eq!(
            fill("Index \u{1}0 is outside vector (bound: \u{1}1).", [5, 3]),
            "Index 5 is outside vector (bound: 3)."
        );
        assert_eq!(fill("no slots", [0, 0]), "no slots");
    }
}
