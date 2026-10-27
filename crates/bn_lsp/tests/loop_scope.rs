// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! IDE Problems match `bni check` for loop scopes (0.6.md, "Scopes and
//! bindings"; ide-tooling.md baseline): the editor reports exactly the
//! language diagnostics of each loop-scope grammar fixture.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bn_frontend::frontend_session::FrontendSession;
use bn_source::SourceFile;
use lsp_types::{DiagnosticSeverity, NumberOrString};

/// Fixture (relative to the workspace) and the error codes `bni check` reports.
const FIXTURES: [(&str, &[&str]); 7] = [
    ("tests/grammar/valid/for-variable-sibling-loops.bn", &[]),
    (
        "tests/grammar/invalid/for-variable-shadows-enclosing.bn",
        &["DUPLICATE_NAME"],
    ),
    (
        "tests/grammar/invalid/for-variable-redeclares-local.bn",
        &["DUPLICATE_NAME"],
    ),
    (
        "tests/grammar/invalid/for-each-variable-shadows-parameter.bn",
        &["DUPLICATE_NAME"],
    ),
    (
        "tests/grammar/invalid/for-body-binding-after-end-for.bn",
        &["NAME_NOT_FOUND"],
    ),
    (
        "tests/grammar/invalid/for-each-body-binding-after-end-for.bn",
        &["NAME_NOT_FOUND"],
    ),
    (
        "tests/grammar/invalid/repeat-body-binding-after-end-repeat.bn",
        &["NAME_NOT_FOUND"],
    ),
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// As an editor sends it: `file:///C:/…` on Windows, `file:///…` elsewhere.
fn file_uri(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let slash = if path.starts_with('/') { "" } else { "/" };
    format!("file://{slash}{path}")
}

fn editor_error_codes(relative: &str) -> Vec<String> {
    let path = std::fs::canonicalize(workspace_root().join(relative)).expect("fixture path");
    let text = std::fs::read_to_string(&path).expect("read fixture");
    let uri = file_uri(&path);
    let documents = HashMap::from([(uri.clone(), SourceFile::new(uri.clone(), text))]);
    let mut session = FrontendSession::default();
    let diagnostics = bn_lsp::diagnostics_for_documents(&path, &documents, &mut session);
    diagnostics
        .get(&uri)
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
        .filter_map(|diagnostic| match &diagnostic.code {
            Some(NumberOrString::String(code)) => Some(code.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn loop_scope_problems_match_bni_check() {
    let mismatches = FIXTURES
        .iter()
        .filter_map(|(fixture, expected)| {
            let actual = editor_error_codes(fixture);
            (actual != *expected).then(|| format!("{fixture}: {actual:?} != {expected:?}"))
        })
        .collect::<Vec<_>>();
    assert!(mismatches.is_empty(), "{mismatches:#?}");
}
