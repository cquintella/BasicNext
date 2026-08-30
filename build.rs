// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{env, fmt::Write as _, fs, path::PathBuf};

#[path = "src/keyword_registry.rs"]
mod keyword_registry;

const REGISTRY: &str = "docs/language/0.2/keywords.md";
const EBNF: &str = "docs/language/0.2/0.2.ebnf";

fn main() {
    println!("cargo:rerun-if-changed={REGISTRY}");
    println!("cargo:rerun-if-changed={EBNF}");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let registry = fs::read_to_string(manifest.join(REGISTRY)).expect("read keyword registry");
    let ebnf = fs::read_to_string(manifest.join(EBNF)).expect("read 0.2 EBNF");
    let parsed = keyword_registry::parse_keywords_md(&registry).unwrap_or_else(|error| {
        panic!("keyword registry: {error}");
    });
    let ebnf_reserved = keyword_registry::parse_ebnf_quoted_production(&ebnf, "reserved-word")
        .unwrap_or_else(|error| panic!("EBNF reserved-word: {error}"));
    let ebnf_special =
        keyword_registry::parse_ebnf_quoted_production(&ebnf, "special-float-literal")
            .unwrap_or_else(|error| panic!("EBNF special-float-literal: {error}"));
    assert_eq!(
        parsed.reserved, ebnf_reserved,
        "keyword registry reserved-word list must match 0.2.ebnf"
    );
    assert_eq!(
        parsed.special_literals, ebnf_special,
        "keyword registry special-float-literal list must match 0.2.ebnf"
    );

    let mut generated = String::from("const RESERVED_WORDS: &[&str] = &[\n");
    for word in &parsed.reserved {
        writeln!(generated, "    {word:?},").expect("write generated keyword");
    }
    generated.push_str("];\n\nconst SPECIAL_FLOAT_LITERALS: &[&str] = &[\n");
    for word in &parsed.special_literals {
        writeln!(generated, "    {word:?},").expect("write generated special literal");
    }
    generated.push_str("];\n");

    let output = PathBuf::from(env::var("OUT_DIR").expect("build output directory"));
    fs::write(output.join("reserved_words.rs"), generated)
        .expect("write generated reserved-word table");
}
