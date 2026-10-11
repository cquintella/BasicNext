// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Console queries over the graph: architecture audit, callers, callees,
//! cycles, fan-in / fan-out.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Edge, FileInfo, GUESSED, TYPE};
use crate::resolve::Edges;

/// Prints every boundary violation; returns whether there were none.
pub fn audit(files: &[FileInfo], edges: &Edges) -> bool {
    let violations: Vec<_> = edges
        .iter()
        .filter_map(|((source, target), edge)| {
            edge.violation.map(|rule| (rule, *source, *target, edge))
        })
        .collect();
    if violations.is_empty() {
        println!(
            "Architecture audit passed: 0 boundary violations across {} edges.",
            edges.len()
        );
        return true;
    }
    eprintln!(
        "Architecture audit FAILED: {} boundary violation(s) detected:",
        violations.len()
    );
    for (rule, source, target, edge) in violations {
        eprintln!(
            "  - [{rule}]: {} -> {} (using: {})",
            files[source].rel,
            files[target].rel,
            edge.summary()
        );
    }
    false
}

fn describe(edge: &Edge) -> String {
    let summary = edge.summary();
    let names = if summary.is_empty() {
        String::new()
    } else {
        format!(" ({summary})")
    };
    format!(
        "[weight: {}, types: {}, guessed: {}, conf: {}]{names}",
        edge.weight(),
        edge.counts[TYPE],
        edge.counts[GUESSED],
        edge.confidence()
    )
}

/// Callers of every file whose path or components contain `query`.
pub fn callers(files: &[FileInfo], edges: &Edges, query: &str) {
    let matches: Vec<usize> = (0..files.len())
        .filter(|&index| {
            files[index].rel.contains(query)
                || files[index]
                    .components
                    .iter()
                    .any(|item| item.contains(query))
        })
        .collect();
    neighbours(files, edges, query, &matches, true);
}

/// Callees of every file whose path contains `query`.
pub fn callees(files: &[FileInfo], edges: &Edges, query: &str) {
    let matches: Vec<usize> = (0..files.len())
        .filter(|&index| files[index].rel.contains(query))
        .collect();
    neighbours(files, edges, query, &matches, false);
}

fn neighbours(files: &[FileInfo], edges: &Edges, query: &str, matches: &[usize], incoming: bool) {
    if matches.is_empty() {
        println!("No files or components matching: '{query}'");
        return;
    }
    let (title, arrow) = if incoming {
        ("Callers", "<-")
    } else {
        ("Callees", "->")
    };
    for &index in matches {
        println!(
            "{title} of {} (crate: {}):",
            files[index].rel, files[index].krate
        );
        let mut count = 0;
        for ((source, target), edge) in edges {
            let other = match (incoming, *source == index, *target == index) {
                (true, _, true) => *source,
                (false, true, _) => *target,
                _ => continue,
            };
            println!("  {arrow} {} {}", files[other].rel, describe(edge));
            count += 1;
        }
        if count == 0 {
            println!("  (none)");
        }
        println!();
    }
}

/// Cycles of confident calls inside one crate (type uses excluded: types
/// referring to each other across files are ordinary).
pub fn cycles(files: &[FileInfo], edges: &Edges) {
    println!("Detecting dependency cycles within crates...");
    let mut adjacency: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for ((source, target), edge) in edges {
        if edge.weight() > 0 && files[*source].krate == files[*target].krate {
            adjacency.entry(*source).or_default().push(*target);
        }
    }
    fn dfs(
        current: usize,
        start: usize,
        adjacency: &BTreeMap<usize, Vec<usize>>,
        visited: &mut BTreeSet<usize>,
        path: &mut Vec<usize>,
    ) -> bool {
        path.push(current);
        visited.insert(current);
        for &next in adjacency.get(&current).into_iter().flatten() {
            if next == start && path.len() > 1 {
                path.push(next);
                return true;
            }
            if !visited.contains(&next) && dfs(next, start, adjacency, visited, path) {
                return true;
            }
        }
        path.pop();
        false
    }
    let mut found = 0;
    for start in 0..files.len() {
        let mut path = Vec::new();
        // Report each cycle once: from its smallest node.
        if dfs(start, start, &adjacency, &mut BTreeSet::new(), &mut path)
            && path[..path.len() - 1].iter().min() == Some(&start)
        {
            found += 1;
            println!("[Cycle #{found} in {}]", files[start].krate);
            let names: Vec<&str> = path
                .iter()
                .map(|&node| {
                    files[node]
                        .rel
                        .rsplit('/')
                        .next()
                        .unwrap_or(&files[node].rel)
                })
                .collect();
            println!("{}\n", names.join(" -> "));
        }
    }
    if found == 0 {
        println!("No internal cycles found within workspace crates.");
    } else {
        println!("Total cycles detected: {found}");
    }
}

/// Top fan-in and fan-out files, over confident calls.
pub fn stats(files: &[FileInfo], edges: &Edges) {
    let mut fan_in = vec![0_usize; files.len()];
    let mut fan_out = vec![0_usize; files.len()];
    for ((source, target), edge) in edges {
        if edge.weight() > 0 {
            fan_out[*source] += 1;
            fan_in[*target] += 1;
        }
    }
    println!("=== Workspace Architecture & Coupling Metrics ===");
    println!(
        "Total files: {} | Total edges: {}\n",
        files.len(),
        edges.len()
    );
    for (title, unit, counts) in [
        (
            "Top 10 Depended-Upon Files (Highest Fan-In / In-Degree):",
            "callers",
            &fan_in,
        ),
        (
            "Top 10 Most Couplings (Highest Fan-Out / Out-Degree):",
            "callees",
            &fan_out,
        ),
    ] {
        let mut ranked: Vec<(usize, usize)> = counts.iter().copied().enumerate().collect();
        ranked.sort_by_key(|&(index, count)| (Reverse(count), index));
        println!("{title}");
        for (index, count) in ranked
            .into_iter()
            .take(10)
            .take_while(|&(_, count)| count > 0)
        {
            println!(
                "  {count:3} {unit} : {} ({})",
                files[index].rel, files[index].krate
            );
        }
        println!();
    }
}
