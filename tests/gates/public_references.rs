// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Public files never cite private material: a reader of this repository
//! or of the book cannot open the private process repository or the
//! unpublished documentation, so a path into them or a private identifier
//! (an issue number, a bucket item, an audit finding) explains nothing.
//! `tests/check-public-references.sh` finds them; this gate holds the
//! files that still have some to a ratchet that may only shrink, with each
//! count exact, so the list ends empty.

use std::{collections::BTreeMap, fs, path::Path, process::Command};

const SCANNER: &str = "tests/check-public-references.sh";
const RATCHET: &str = "tests/public_references.list";

/// Findings per file from the scanner run on `root`.
fn findings_per_file(root: &Path) -> BTreeMap<String, usize> {
    let output = Command::new("bash")
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .arg(SCANNER)
        .arg("--root")
        .arg(root)
        .output()
        .expect("run the public-reference scanner");
    assert!(output.status.success(), "{output:?}");
    let mut counts = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let file = line.split(':').next().unwrap_or_default().to_owned();
        *counts.entry(file).or_insert(0) += 1;
    }
    counts
}

/// `count<TAB>path` lines; `#` starts a comment.
fn ratchet(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (count, path) = line.split_once('\t').expect("count<TAB>path");
            (path.to_owned(), count.parse().expect("a count"))
        })
        .collect()
}

/// Every difference between the findings and the ratchet.
fn ratchet_violations(
    found: &BTreeMap<String, usize>,
    allowed: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (file, count) in found {
        match allowed.get(file) {
            None => violations.push(format!("{file}: {count} new private reference(s)")),
            Some(limit) if count > limit => {
                violations.push(format!(
                    "{file}: {count} private references, ratchet {limit}"
                ));
            }
            Some(limit) if count < limit => {
                violations.push(format!(
                    "{file}: down to {count}; lower its ratchet from {limit}"
                ));
            }
            Some(_) => {}
        }
    }
    for file in allowed.keys().filter(|file| !found.contains_key(*file)) {
        violations.push(format!("{file}: clean now; remove it from {RATCHET}"));
    }
    violations
}

#[test]
fn public_files_cite_no_private_material_beyond_the_ratchet() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let allowed = ratchet(&fs::read_to_string(root.join(RATCHET)).expect("ratchet list"));
    let violations = ratchet_violations(&findings_per_file(root), &allowed);
    assert!(
        violations.is_empty(),
        "state the reason in place instead of citing a private path or ID \
         (`bash {SCANNER} --root .` lists them):\n{}",
        violations.join("\n")
    );
}

#[test]
fn the_scanner_finds_each_private_shape_and_passes_plain_text() {
    let tree = super::support::TestDir::new("public-references").expect("scratch tree");
    // Built at run time, so this file holds none of the shapes it plants.
    let issue = format!("K{}", 61);
    let samples = [
        (
            "issue.rs",
            format!("// released at the end (known issue {issue}).\n"),
        ),
        ("item.rs", format!("// see T.{issue} for the cell\n")),
        (
            "path.md",
            format!("Open a proposal in `{}/proposals/`.\n", "todo"),
        ),
        (
            "docs.md",
            format!("See {}/architecture/ir-contract.md.\n", "docs"),
        ),
        ("bucket.rs", format!("// shipped by bucket {}.6.12\n", 0)),
        (
            "finding.rs",
            format!("// rejection sampling (audit, finding A{}).\n", 14),
        ),
        (
            "decision.md",
            format!("Decided (BDFL decision G-{}, 2026-10-27).\n", 5),
        ),
        (
            "plain.rs",
            "// a K-means step; Section 3 of RFC 3339; INT32 (0x7f).\n".to_owned(),
        ),
    ];
    for (name, text) in &samples {
        fs::write(tree.join(name), text).expect("sample");
    }
    let found = findings_per_file(tree.path());
    let flagged: Vec<&str> = found.keys().map(String::as_str).collect();
    assert_eq!(
        flagged,
        [
            "bucket.rs",
            "decision.md",
            "docs.md",
            "finding.rs",
            "issue.rs",
            "item.rs",
            "path.md"
        ],
        "{found:?}"
    );
}

#[test]
fn the_ratchet_only_shrinks_and_ends_empty() {
    let allowed = ratchet("# comment\n2\ta.rs\n1\tb.rs\n");
    let same = BTreeMap::from([("a.rs".to_owned(), 2), ("b.rs".to_owned(), 1)]);
    assert!(ratchet_violations(&same, &allowed).is_empty());
    let grown = BTreeMap::from([("a.rs".to_owned(), 3), ("b.rs".to_owned(), 1)]);
    assert_eq!(ratchet_violations(&grown, &allowed).len(), 1);
    let new_file = BTreeMap::from([
        ("a.rs".to_owned(), 2),
        ("b.rs".to_owned(), 1),
        ("c.rs".to_owned(), 1),
    ]);
    assert_eq!(ratchet_violations(&new_file, &allowed).len(), 1);
    let shrunk = BTreeMap::from([("a.rs".to_owned(), 1)]);
    assert_eq!(
        ratchet_violations(&shrunk, &allowed).len(),
        2,
        "a lower count and a clean file must update the list"
    );
}
