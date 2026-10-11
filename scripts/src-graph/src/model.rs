// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Workspace packages, per-file facts, and the edge model shared by every
//! output and query.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Package {
    pub name: String, // crate name as written in Rust paths (`-` -> `_`)
    pub dir: PathBuf,
    pub src: PathBuf,
    /// Workspace members this package depends on (normal, dev or build).
    pub deps: BTreeSet<String>,
}

impl Package {
    /// A library crate: its `pub` items can be named by other crates.
    pub fn is_lib(&self) -> bool {
        self.src.join("lib.rs").is_file()
    }
}

pub type Deps = HashMap<String, BTreeSet<String>>;

/// A `pub` item of a library crate, a candidate for `pub(crate)`.
pub struct PubItem {
    pub kind: &'static str,
    /// `name`, or `Type::name` for an inherent method.
    pub label: String,
    /// The identifier other crates would write.
    pub name: String,
    pub line: usize,
}

pub struct FileInfo {
    pub rel: String,
    pub krate: String,
    pub lines: usize,
    pub sha256: String,
    pub components: Vec<String>,
    pub calls: Vec<Call>,
    pub uses: HashMap<String, Vec<String>>,
    pub globs: Vec<Vec<String>>,
    pub pub_items: Vec<PubItem>,
    pub parse_error: Option<String>,
}

pub enum Call {
    Path(Vec<String>),
    Method(String),
    /// `self.m()` inside `impl T`: resolved through `(T, m)`.
    SelfMethod(String, String),
    /// A path naming a type (struct, enum, trait, alias), cut at its first
    /// upper-case segment: `ast::Token::Ident` -> `ast::Token`.
    Type(Vec<String>),
}

#[derive(Default)]
pub struct Defs {
    pub free_fns: HashMap<String, Vec<usize>>,
    // (type, method) -> files
    pub type_methods: HashMap<(String, String), Vec<usize>>,
    // method -> files
    pub methods: HashMap<String, Vec<usize>>,
    // struct / enum / trait / type alias / union name -> files
    pub types: HashMap<String, Vec<usize>>,
}

/// Edge kinds, the index into `Edge::counts`.
pub const PATH: usize = 0;
pub const SELF: usize = 1;
pub const GUESSED: usize = 2;
pub const TYPE: usize = 3;

/// References from one file to another: counts per kind [path, self,
/// guessed, type], with the names behind each kind.
#[derive(Default)]
pub struct Edge {
    pub counts: [usize; 4],
    pub names: BTreeSet<String>,
    pub guessed: BTreeSet<String>,
    pub types: BTreeSet<String>,
    pub violation: Option<&'static str>,
}

impl Edge {
    /// Confidently resolved calls (path + self).
    pub fn weight(&self) -> usize {
        self.counts[PATH] + self.counts[SELF]
    }

    pub fn confidence(&self) -> &'static str {
        if self.weight() > 0 {
            "high"
        } else if self.counts[TYPE] > 0 {
            "type"
        } else {
            "low"
        }
    }

    /// The names a reader wants first: calls, else types, else guesses.
    pub fn summary(&self) -> String {
        let set = [&self.names, &self.types, &self.guessed]
            .into_iter()
            .find(|set| !set.is_empty())
            .unwrap_or(&self.names);
        set.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

pub fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

pub fn packages(root: &Path) -> Vec<Package> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .expect("run cargo metadata");
    assert!(output.status.success(), "cargo metadata failed");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("metadata json");
    let list = json["packages"].as_array().expect("packages array");
    let crate_name = |value: &serde_json::Value| value.as_str().unwrap().replace('-', "_");
    let members: BTreeSet<String> = list
        .iter()
        .map(|package| crate_name(&package["name"]))
        .collect();
    list.iter()
        .map(|package| {
            let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
            let dir = manifest.parent().unwrap().to_path_buf();
            Package {
                name: crate_name(&package["name"]),
                src: dir.join("src"),
                dir,
                deps: package["dependencies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|dep| crate_name(&dep["name"]))
                    .filter(|dep| members.contains(dep))
                    .collect(),
            }
        })
        .collect()
}
