// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Deterministic replay of the fuzz seed corpus through `lex` + `parse`.
//! Contract: no `.bn` input may panic; malformed input yields `Err(Diagnostic)`.
//! The fuzzer itself (`fuzz/`) runs offline on nightly; this test is the CI gate.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

fn collect_bn(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_bn(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "bn") {
            out.push(path);
        }
    }
}

#[test]
fn seed_corpus_never_panics() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_bn(&root.join("examples"), &mut files);
    collect_bn(&root.join("tests/grammar"), &mut files);
    collect_bn(&root.join("fuzz/artifacts"), &mut files);
    files.sort();
    assert!(files.len() > 100, "corpus too small: {}", files.len());

    for path in &files {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let source = bn_frontend::source::SourceFile::new(name, text);
        if let Ok(tokens) = bn_frontend::lexer::lex(&source) {
            let _ = bn_frontend::parser::parse(&tokens);
        }
    }
}
