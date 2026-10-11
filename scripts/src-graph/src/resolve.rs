// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Resolves call sites and type references to file-to-file edges, without
//! type inference, and flags edges that cross an architecture boundary.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::model::{Call, Defs, Deps, Edge, FileInfo, GUESSED, PATH, SELF, TYPE};

/// What a path names: a function (or `Type::method`) or a type.
#[derive(Clone, Copy)]
enum Target {
    Fn,
    Type,
}

/// Resolves a path used in file `from` to the defining files. Follows
/// re-exports (`pub use`) in a crate's root file, up to `depth` levels.
#[allow(clippy::too_many_arguments)]
fn resolve_path(
    files: &[FileInfo],
    definitions: &Defs,
    deps: &Deps,
    from: usize,
    segments: &[String],
    target: Target,
    depth: usize,
) -> Option<Vec<usize>> {
    let info = &files[from];
    let mut path = segments.to_vec();
    let imported = info.uses.get(&path[0]).cloned();
    if let Some(full) = &imported {
        path.splice(0..1, full.iter().cloned());
    }
    let mut crates: BTreeSet<String> = BTreeSet::new();
    let mut explicit_crate = None;
    if deps.contains_key(path[0].as_str()) && path.len() > 1 {
        let krate = path.remove(0);
        explicit_crate = Some(krate.clone());
        crates.insert(krate);
    } else if matches!(path[0].as_str(), "crate" | "self" | "super") {
        while path.len() > 1 && matches!(path[0].as_str(), "crate" | "self" | "super") {
            path.remove(0);
        }
        crates.insert(info.krate.clone());
    } else if imported.is_some() {
        return None; // std or an external crate
    } else {
        crates.insert(info.krate.clone());
        for glob in &info.globs {
            if let Some(first) = glob.first()
                && deps.contains_key(first.as_str())
            {
                crates.insert(first.clone());
            }
        }
    }
    let name = path.last().unwrap().clone();
    let table: &HashMap<String, Vec<usize>> = match target {
        Target::Fn => &definitions.free_fns,
        Target::Type => &definitions.types,
    };
    let in_crates = |found: &Vec<usize>| -> Vec<usize> {
        found
            .iter()
            .copied()
            .filter(|index| crates.contains(&files[*index].krate))
            .collect()
    };
    let targets: Vec<usize> = if path.len() >= 2 {
        let owner = &path[path.len() - 2];
        if owner.starts_with(char::is_uppercase) {
            definitions
                .type_methods
                .get(&(owner.clone(), name.clone()))
                .map(in_crates)
                .unwrap_or_default()
        } else {
            let file_hint = format!("/{owner}.rs");
            let dir_hint = format!("/{owner}/");
            table
                .get(&name)
                .map(in_crates)
                .unwrap_or_default()
                .into_iter()
                .filter(|index| {
                    files[*index].rel.ends_with(&file_hint) || files[*index].rel.contains(&dir_hint)
                })
                .collect()
        }
    } else {
        let found = table.get(&name).map(in_crates).unwrap_or_default();
        let rooted = matches!(segments[0].as_str(), "crate" | "super")
            || imported
                .as_ref()
                .is_some_and(|full| matches!(full[0].as_str(), "crate" | "super"));
        if found.contains(&from) && !rooted {
            // Unqualified name defined in this very file.
            vec![from]
        } else if rooted {
            // `crate::f` / `super::f`: the crate root when it defines `f`.
            let root: Vec<usize> = found
                .iter()
                .copied()
                .filter(|index| {
                    files[*index].rel.ends_with("src/lib.rs")
                        || files[*index].rel.ends_with("src/main.rs")
                })
                .collect();
            if root.is_empty() { found } else { root }
        } else {
            found
        }
    };
    if !targets.is_empty() || depth == 0 {
        return Some(targets);
    }
    // `other_crate::name` with no definition named `name`: follow the
    // other crate's root re-export of `name`, if any.
    let krate = explicit_crate?;
    let root = files
        .iter()
        .position(|file| file.krate == krate && file.rel.ends_with("src/lib.rs"))?;
    let mut rest = files[root].uses.get(path.first()?)?.clone();
    rest.extend(path.iter().skip(1).cloned());
    if rest
        .first()
        .is_some_and(|first| !deps.contains_key(first.as_str()) && first != "crate")
    {
        rest.insert(0, "crate".into());
    }
    resolve_path(files, definitions, deps, root, &rest, target, depth - 1)
}

/// Checks whether a dependency from `caller_crate` to `callee_crate`
/// crosses a workspace architecture boundary.
fn check_architecture_rule(caller_crate: &str, callee_crate: &str) -> Option<&'static str> {
    if caller_crate == callee_crate {
        return None;
    }
    const BACKEND_CRATES: &[&str] = &[
        "bn_interp",
        "bn_llvm",
        "bn_rt",
        "bn_runtime",
        "bn_value",
        "bn_host_exec",
        "bn_host_fs",
        "bn_host_net",
        "bn_lib_math",
        "bn_lib_json",
        "bn_lib_log",
        "bn_lib_data",
        "bn_lib_dispatch",
        "bn_lib_web",
        "bn_lib_crypto",
        "bn_lib_sqlite",
        "bn_limits",
    ];
    const FORBIDDEN_FROM_INTERP: &[&str] = &[
        "bn_host_exec",
        "bn_host_fs",
        "bn_host_net",
        "bn_lib_web",
        "bn_lib_crypto",
        "bn_lib_sqlite",
        "bn_lib_dispatch",
    ];
    // Rule 1: backends and runtimes do not depend on the frontend.
    if BACKEND_CRATES.contains(&caller_crate) && callee_crate == "bn_frontend" {
        return Some("backend->frontend (compiler boundary violation)");
    }
    // Rule 2 (W5): the bn_ir model is independent of frontend and backends.
    if caller_crate == "bn_ir"
        && (callee_crate == "bn_frontend" || BACKEND_CRATES.contains(&callee_crate))
    {
        return Some("bn_ir->frontend/backend (W5 model violation)");
    }
    // Rule 3: the interpreter core does not link host/web/io implementations.
    if caller_crate == "bn_interp" && FORBIDDEN_FROM_INTERP.contains(&callee_crate) {
        return Some("bn_interp->host/io (interpreter purity violation)");
    }
    // Rule 4 (W3): bnc and the compile driver never link the interpreter.
    if (caller_crate == "bnc" || caller_crate == "bn_compile_driver")
        && (callee_crate == "bn_interp" || callee_crate == "bn_interpret_driver")
    {
        return Some("compiler->interpreter (W3 pipeline violation)");
    }
    None
}

pub type Edges = BTreeMap<(usize, usize), Edge>;

/// Resolves every call site and type reference to file-to-file edges; also
/// returns, per file, the method calls that matched several files and were
/// left unlinked.
pub fn link(files: &[FileInfo], definitions: &Defs, deps: &Deps) -> (Edges, Vec<usize>) {
    // Candidate files visible from `krate`: its own files, then its
    // workspace dependencies.
    let visible = |krate: &str, candidates: &[usize]| -> Vec<usize> {
        let own: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|index| files[*index].krate == krate)
            .collect();
        if !own.is_empty() {
            return own;
        }
        let krate_deps = &deps[krate];
        candidates
            .iter()
            .copied()
            .filter(|index| krate_deps.contains(&files[*index].krate))
            .collect()
    };
    let dedup = |mut list: Vec<usize>| {
        list.sort_unstable();
        list.dedup();
        list
    };

    let mut edges = Edges::new();
    let mut unresolved_methods = vec![0_usize; files.len()];
    for (caller, info) in files.iter().enumerate() {
        for call in &info.calls {
            let (targets, kind, name) = match call {
                Call::Path(segments) => {
                    let Some(targets) =
                        resolve_path(files, definitions, deps, caller, segments, Target::Fn, 3)
                    else {
                        continue;
                    };
                    (dedup(targets), PATH, segments.last().unwrap().clone())
                }
                Call::Type(segments) => {
                    let Some(targets) =
                        resolve_path(files, definitions, deps, caller, segments, Target::Type, 3)
                    else {
                        continue;
                    };
                    (dedup(targets), TYPE, segments.last().unwrap().clone())
                }
                Call::SelfMethod(ty, name) => {
                    let targets = definitions
                        .type_methods
                        .get(&(ty.clone(), name.clone()))
                        .map(|found| visible(&info.krate, found))
                        .unwrap_or_default();
                    (dedup(targets), SELF, name.clone())
                }
                Call::Method(name) => {
                    let targets = dedup(
                        definitions
                            .methods
                            .get(name)
                            .map(|found| visible(&info.krate, found))
                            .unwrap_or_default(),
                    );
                    if targets.len() != 1 {
                        if !targets.is_empty() {
                            unresolved_methods[caller] += 1;
                        }
                        continue;
                    }
                    (targets, GUESSED, name.clone())
                }
            };
            for callee in targets {
                if callee == caller {
                    continue;
                }
                let entry = edges.entry((caller, callee)).or_default();
                entry.counts[kind] += 1;
                if entry.violation.is_none() {
                    entry.violation =
                        check_architecture_rule(&files[caller].krate, &files[callee].krate);
                }
                match kind {
                    GUESSED => entry.guessed.insert(name.clone()),
                    TYPE => entry.types.insert(name.clone()),
                    _ => entry.names.insert(name.clone()),
                };
            }
        }
    }
    (edges, unresolved_methods)
}
