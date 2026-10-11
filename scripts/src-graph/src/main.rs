// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Developer tool: file-level component and call graph of the workspace, as
//! GML. Parses every `src/**/*.rs` of each workspace package with `syn`.
//! Calls and type references are resolved through each file's `use` map,
//! without type inference; every edge carries a confidence (see the graph
//! `comment` in `output.rs`). Run through `scripts/src-graph.sh`.
//!
//!   bn-src-graph --check <root> <graph.gml>
//!   bn-src-graph <root> [out.gml|out.dot] [--crates] [--audit] [--cycles]
//!                [--stats] [--callers <q>] [--callees <q>]
//!                [--unused-pub[=<crate>]]

mod collect;
mod fresh;
mod model;
mod output;
mod query;
mod resolve;
mod unused_pub;

use std::path::PathBuf;

use model::Deps;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--check") {
        let root = PathBuf::from(args.get(1).expect("workspace root"));
        let graph = PathBuf::from(args.get(2).expect("graph .gml"));
        let root = root.canonicalize().expect("workspace root exists");
        std::process::exit(i32::from(!fresh::check(&root, &graph)));
    }

    let flag = |name: &str| args.iter().any(|arg| arg == name);
    let value_of = |name: &str| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|at| args.get(at + 1))
    };
    let unused_pub = args.iter().find_map(|arg| {
        arg.strip_prefix("--unused-pub")
            .filter(|rest| rest.is_empty() || rest.starts_with('='))
            .map(|rest| rest.strip_prefix('=').map(str::to_string))
    });
    let mut positional = Vec::new();
    let mut skip = false;
    for arg in &args {
        if std::mem::take(&mut skip) {
            continue;
        }
        if arg == "--callers" || arg == "--callees" {
            skip = true;
        } else if !arg.starts_with("--") {
            positional.push(arg);
        }
    }

    let root = PathBuf::from(positional.first().expect("workspace root"));
    let root = root.canonicalize().expect("workspace root exists");
    let out = positional.get(1).map(PathBuf::from);
    let packages = model::packages(&root);
    let deps: Deps = packages
        .iter()
        .map(|package| (package.name.clone(), package.deps.clone()))
        .collect();
    let (files, definitions) = collect::collect(&root, &packages);
    let (edges, unresolved_methods) = resolve::link(&files, &definitions, &deps);

    let mut failed = false;
    if flag("--audit") {
        failed |= !query::audit(&files, &edges);
    }
    if flag("--cycles") {
        query::cycles(&files, &edges);
    }
    if flag("--stats") {
        query::stats(&files, &edges);
    }
    if let Some(target) = value_of("--callers") {
        query::callers(&files, &edges, target);
    }
    if let Some(target) = value_of("--callees") {
        query::callees(&files, &edges, target);
    }
    if let Some(only) = unused_pub {
        unused_pub::report(&root, &packages, &files, only.as_deref());
    }
    if let Some(out) = out {
        match out.extension().and_then(|ext| ext.to_str()) {
            Some("dot" | "gv") if flag("--crates") => {
                output::write_crates_dot(&out, &files, &edges)
            }
            Some("dot" | "gv") => output::write_dot(&out, &files, &edges),
            _ => output::write_gml(&out, &files, &edges, &unresolved_methods),
        }
    } else if failed {
        std::process::exit(1);
    }
}
