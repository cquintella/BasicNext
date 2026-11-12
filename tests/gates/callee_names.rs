// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The interpreter calls a resolved target (`bn_ir::callee::CalleeTable`);
//! its call dispatch never parses a callee name to find out what it is.

use std::fs;

/// The interpreter's call dispatch: `call_named` and the method dispatch.
const CALL_DISPATCH: [&str; 2] = [
    "crates/bn_interp/src/executor/part3.rs",
    "crates/bn_interp/src/executor/part10.rs",
];
/// Ways of reading a callee's kind out of its spelling.
const NAME_PARSING: [&str; 3] = ["strip_prefix('#')", "\"@super:\"", "rsplit('.')"];

fn name_parsing_in(path: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .flat_map(|(index, line)| {
            NAME_PARSING
                .iter()
                .filter(move |pattern| line.contains(**pattern))
                .map(move |pattern| format!("{path}:{}: {pattern}", index + 1))
        })
        .collect()
}

#[test]
fn callee_names_are_not_parsed_in_call_dispatch() {
    let found = CALL_DISPATCH
        .iter()
        .flat_map(|path| {
            let text = fs::read_to_string(path).expect("read the call dispatch");
            name_parsing_in(path, &text)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        Vec::<String>::new(),
        "a callee name parsed at call time"
    );
}

#[test]
fn a_planted_name_parse_is_found() {
    let text = "// rsplit('.') in a comment\nlet m = name.rsplit('.').next();\n";
    assert_eq!(
        name_parsing_in("planted.rs", text),
        ["planted.rs:2: rsplit('.')"]
    );
}
