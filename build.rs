// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{env, fmt::Write as _, fs, path::PathBuf};

#[path = "crates/bn_frontend/src/keyword_registry.rs"]
mod keyword_registry;

use keyword_registry::{SPEC_EBNF, SPEC_KEYWORDS};

fn main() {
    println!("cargo:rerun-if-changed={SPEC_KEYWORDS}");
    println!("cargo:rerun-if-changed={SPEC_EBNF}");

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let registry = fs::read_to_string(manifest.join(SPEC_KEYWORDS))
        .unwrap_or_else(|error| panic!("read {SPEC_KEYWORDS}: {error}"));
    let ebnf = fs::read_to_string(manifest.join(SPEC_EBNF))
        .unwrap_or_else(|error| panic!("read {SPEC_EBNF}: {error}"));
    let parsed = keyword_registry::parse_keywords_md(&registry).unwrap_or_else(|error| {
        panic!("{SPEC_KEYWORDS}: {error}");
    });
    let ebnf_reserved = keyword_registry::parse_ebnf_quoted_production(&ebnf, "reserved-word")
        .unwrap_or_else(|error| panic!("{SPEC_EBNF} reserved-word: {error}"));
    let ebnf_special =
        keyword_registry::parse_ebnf_quoted_production(&ebnf, "special-float-literal")
            .unwrap_or_else(|error| panic!("{SPEC_EBNF} special-float-literal: {error}"));
    assert_eq!(
        parsed.reserved, ebnf_reserved,
        "{SPEC_KEYWORDS} reserved-word list must match {SPEC_EBNF}"
    );
    assert_eq!(
        parsed.special_literals, ebnf_special,
        "{SPEC_KEYWORDS} special-float-literal list must match {SPEC_EBNF}"
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
