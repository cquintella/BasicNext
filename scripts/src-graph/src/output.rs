// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Graph writers: GML (the checked artifact), file-level DOT, crate-level DOT.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use crate::fresh::{generator_digest, sources_digest};
use crate::model::{FileInfo, GUESSED, PATH, SELF, TYPE};
use crate::resolve::Edges;

const GML_COMMENT: &str = "Basic Next: one node per src/**/*.rs file of each workspace package. \
Node components = items parsed with syn (fn, struct, enum, union, trait, impl with its methods, \
const, static, type, macro_rules, mod); #[cfg(test)] modules and #[test] fns are skipped. \
Edges = references from one file to another. No type inference. \
calls: path calls f() / Type::f() / crate::f(), linked to same-name definitions in the \
caller's crate, else in its workspace dependencies (several files if several define it). \
selfCalls: self.m() inside impl T, linked to files defining T::m. \
typeUses: paths naming a struct/enum/union/trait/type alias (in types, patterns, expressions, \
impl headers, bounds), cut at the first upper-case segment and resolved like calls. \
guessedCalls (LOW confidence): x.m() on any other receiver, counted only when exactly \
one visible file defines a method m; std methods with the same name produce false edges. \
Calls inside macro arguments count when the arguments parse as comma-separated expressions \
(format!, vec![..], assert!, write!); macro_rules bodies and other macro shapes are not read. \
confidence = high when calls + selfCalls > 0, else type when typeUses > 0, else low. \
weight = calls + selfCalls (min 1). ambiguousMethodCalls on a node = x.m() calls matching \
several files (not linked). Trait-object dispatch, std/external calls and \
type inference are out of reach. Self-edges are omitted.";

fn gml_string(text: &str) -> String {
    text.replace('&', "&amp;").replace('"', "&quot;")
}

fn join(set: &BTreeSet<String>) -> String {
    gml_string(
        &set.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", "),
    )
}

pub fn write_gml(out: &Path, files: &[FileInfo], edges: &Edges, unresolved_methods: &[usize]) {
    let mut gml = String::new();
    gml.push_str("graph [\n  directed 1\n");
    let digest = sources_digest(
        files
            .iter()
            .map(|info| (info.rel.as_str(), info.sha256.as_str())),
    );
    let _ = writeln!(gml, "  sourcesSha256 \"{digest}\"");
    let _ = writeln!(gml, "  generatorSha256 \"{}\"", generator_digest());
    let _ = writeln!(gml, "  comment \"{}\"", gml_string(GML_COMMENT));
    for (index, info) in files.iter().enumerate() {
        let _ = writeln!(gml, "  node [");
        let _ = writeln!(gml, "    id {index}");
        let _ = writeln!(gml, "    label \"{}\"", gml_string(&info.rel));
        let _ = writeln!(gml, "    crate \"{}\"", info.krate);
        let _ = writeln!(gml, "    lines {}", info.lines);
        let _ = writeln!(gml, "    sha256 \"{}\"", info.sha256);
        let _ = writeln!(gml, "    componentCount {}", info.components.len());
        let _ = writeln!(
            gml,
            "    components \"{}\"",
            gml_string(&info.components.join("; "))
        );
        let _ = writeln!(gml, "    pubItems {}", info.pub_items.len());
        let _ = writeln!(
            gml,
            "    ambiguousMethodCalls {}",
            unresolved_methods[index]
        );
        if let Some(error) = &info.parse_error {
            let _ = writeln!(gml, "    parseError \"{}\"", gml_string(error));
        }
        let _ = writeln!(gml, "  ]");
    }
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for (edge_id, ((source, target), edge)) in edges.iter().enumerate() {
        *tally.entry(edge.confidence()).or_default() += 1;
        let _ = writeln!(gml, "  edge [");
        let _ = writeln!(gml, "    id {edge_id}");
        let _ = writeln!(gml, "    source {source}");
        let _ = writeln!(gml, "    target {target}");
        let _ = writeln!(gml, "    label \"{}\"", gml_string(&edge.summary()));
        let _ = writeln!(gml, "    calls {}", edge.counts[PATH]);
        let _ = writeln!(gml, "    selfCalls {}", edge.counts[SELF]);
        let _ = writeln!(gml, "    guessedCalls {}", edge.counts[GUESSED]);
        let _ = writeln!(gml, "    typeUses {}", edge.counts[TYPE]);
        // Gephi rejects weight = 0.
        let _ = writeln!(gml, "    weight {}", edge.weight().max(1));
        let _ = writeln!(gml, "    confidence \"{}\"", edge.confidence());
        let _ = writeln!(gml, "    names \"{}\"", join(&edge.names));
        let _ = writeln!(gml, "    typeNames \"{}\"", join(&edge.types));
        let _ = writeln!(gml, "    guessedNames \"{}\"", join(&edge.guessed));
        if let Some(rule) = edge.violation {
            let _ = writeln!(gml, "    violation \"{rule}\"");
        }
        let _ = writeln!(gml, "  ]");
    }
    gml.push_str("]\n");
    std::fs::write(out, gml).expect("write gml");
    let parse_errors = files
        .iter()
        .filter(|info| info.parse_error.is_some())
        .count();
    eprintln!(
        "files {} edges {} (high {}, type {}, low {}) parse errors {parse_errors}",
        files.len(),
        edges.len(),
        tally.get("high").unwrap_or(&0),
        tally.get("type").unwrap_or(&0),
        tally.get("low").unwrap_or(&0),
    );
}

fn dot_escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

const DOT_HEADER: &str =
    "  graph [rankdir=LR, splines=true, overlap=false, fontname=\"Helvetica\"];\n";

pub fn write_dot(out: &Path, files: &[FileInfo], edges: &Edges) {
    let mut dot = String::from("digraph bn_workspace {\n");
    dot.push_str(DOT_HEADER);
    dot.push_str("  node [shape=box, style=filled, fillcolor=\"#f8f9fa\", color=\"#dee2e6\", fontname=\"Helvetica\", fontsize=10];\n");
    dot.push_str("  edge [fontname=\"Helvetica\", fontsize=8];\n\n");
    let mut by_crate: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, file) in files.iter().enumerate() {
        by_crate.entry(&file.krate).or_default().push(index);
    }
    for (krate, indices) in by_crate {
        let _ = writeln!(dot, "  subgraph \"cluster_{krate}\" {{");
        let _ = writeln!(dot, "    label = \"{krate}\";");
        dot.push_str("    style = \"rounded,filled\";\n    fillcolor = \"#f1f3f5\";\n    color = \"#ced4da\";\n");
        for index in indices {
            let rel = &files[index].rel;
            let lines = files[index].lines;
            let short = rel.rsplit('/').next().unwrap_or(rel);
            // tests/source_size.rs caps a file at 1000 lines.
            let fill = match lines {
                1000.. => "#ffe3e3",
                800.. => "#fff3bf",
                _ => "#ffffff",
            };
            let _ = writeln!(
                dot,
                "    n{index} [label=\"{short}\\n({lines} lines)\", fillcolor=\"{fill}\", tooltip=\"{rel}\"];"
            );
        }
        dot.push_str("  }\n\n");
    }
    for ((source, target), edge) in edges {
        let (color, style, penwidth) = match (edge.violation, edge.confidence()) {
            (Some(_), _) => ("#e03131", "bold", 2.5),
            (None, "high") => ("#495057", "solid", 1.0),
            (None, "type") => ("#74c0fc", "dotted", 0.8),
            _ => ("#adb5bd", "dashed", 0.5),
        };
        let label = match edge.violation {
            Some(rule) => format!(" [VIOLATION: {rule}]"),
            None if edge.weight() > 1 => format!(" ({})", edge.weight()),
            None => String::new(),
        };
        let tooltip = dot_escape(&format!(
            "calls: {}, types: {}, guessed: {}{label}",
            edge.weight(),
            edge.counts[TYPE],
            edge.counts[GUESSED]
        ));
        let _ = writeln!(
            dot,
            "  n{source} -> n{target} [color=\"{color}\", style=\"{style}\", penwidth={penwidth}, tooltip=\"{tooltip}\"];"
        );
    }
    dot.push_str("}\n");
    std::fs::write(out, dot).expect("write dot");
    eprintln!("wrote DOT graph to {}", out.display());
}

pub fn write_crates_dot(out: &Path, files: &[FileInfo], edges: &Edges) {
    let mut dot = String::from("digraph bn_crates {\n");
    dot.push_str(DOT_HEADER);
    dot.push_str("  node [shape=box, style=\"rounded,filled\", fillcolor=\"#e7f5ff\", color=\"#339af0\", fontname=\"Helvetica\", fontsize=11];\n");
    dot.push_str("  edge [fontname=\"Helvetica\", fontsize=9];\n\n");
    // (caller, callee) -> (calls, type uses, violation)
    let mut crate_edges: BTreeMap<(&str, &str), (usize, usize, Option<&'static str>)> =
        BTreeMap::new();
    for ((source, target), edge) in edges {
        let (caller, callee) = (files[*source].krate.as_str(), files[*target].krate.as_str());
        if caller == callee {
            continue;
        }
        let entry = crate_edges.entry((caller, callee)).or_default();
        entry.0 += edge.weight();
        entry.1 += edge.counts[TYPE];
        entry.2 = entry.2.or(edge.violation);
    }
    let crates: BTreeSet<&str> = files.iter().map(|file| file.krate.as_str()).collect();
    for krate in crates {
        let _ = writeln!(dot, "  \"{krate}\";");
    }
    for ((caller, callee), (weight, types, violation)) in crate_edges {
        let (color, style, penwidth) = match violation {
            Some(_) => ("#e03131", "bold", 3.0),
            None if weight > 0 => ("#343a40", "solid", 1.2),
            None if types > 0 => ("#74c0fc", "dotted", 1.0),
            None => ("#adb5bd", "dashed", 0.8),
        };
        let label = match violation {
            Some(rule) => format!("VIOLATION: {rule}"),
            None => format!("{weight}/{types}t"),
        };
        let _ = writeln!(
            dot,
            "  \"{caller}\" -> \"{callee}\" [label=\" {label}\", color=\"{color}\", style=\"{style}\", penwidth={penwidth}];"
        );
    }
    dot.push_str("}\n");
    std::fs::write(out, dot).expect("write crates dot");
    eprintln!("wrote aggregated crates DOT graph to {}", out.display());
}
