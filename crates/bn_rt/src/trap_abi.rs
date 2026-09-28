// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Native runtime diagnostics (bucket 0.6.2b R6). `bnc` renders each trap
//! site's diagnostic from the shared catalog at compile time; the runtime
//! only fills the facts known when the trap fires and prints the text. No
//! catalog text lives here.

use std::ffi::{CStr, c_char};
use std::io::Write as _;

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
    fn slots_take_the_runtime_facts() {
        assert_eq!(
            fill("Index \u{1}0 is outside vector (bound: \u{1}1).", [5, 3]),
            "Index 5 is outside vector (bound: 3)."
        );
        assert_eq!(fill("no slots", [0, 0]), "no slots");
    }
}
