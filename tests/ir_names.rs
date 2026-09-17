// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The IR function-name protocol is a contract (bucket 0.5.1c §3.1):
//! the frontend may only emit documented name shapes, and the backends may
//! only decode documented shapes. `bn_ir::names` is the single table.

use std::{fs, path::Path};

use bn::{
    ir::{FunctionKind, names},
    lowering::lower_graph,
    module_graph::load,
};
use bn_frontend::semantic::analyze_modules;

fn bn_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            bn_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "bn") {
            out.push(path);
        }
    }
}

#[test]
fn every_emitted_function_name_follows_a_documented_shape() {
    let mut files = Vec::new();
    bn_files(Path::new("tests/grammar/valid"), &mut files);
    bn_files(Path::new("examples"), &mut files);
    files.sort();
    let mut lowered = 0;
    let mut undocumented = Vec::new();
    for path in &files {
        let Ok(graph) = load(path.to_str().expect("path")) else {
            continue;
        };
        let Ok(models) = analyze_modules(&graph) else {
            continue;
        };
        let Ok(module) = lower_graph(&graph, &models) else {
            continue;
        };
        lowered += 1;
        for function in &module.functions {
            let Some(shape) = names::classify(&function.name) else {
                undocumented.push(format!("{}: {}", path.display(), function.name));
                continue;
            };
            // While the name protocol and `FunctionKind` both exist they must
            // agree: the name is informative, the kind is what backends read.
            let expected = match shape {
                names::EmittedNameKind::Entry => FunctionKind::Entry,
                names::EmittedNameKind::Constructor => FunctionKind::Constructor,
                names::EmittedNameKind::Destructor => FunctionKind::Destructor,
                names::EmittedNameKind::FieldInit => FunctionKind::FieldInit,
                names::EmittedNameKind::Init => FunctionKind::Init,
                names::EmittedNameKind::Default => FunctionKind::Default,
                names::EmittedNameKind::User => FunctionKind::User,
            };
            if function.kind != expected {
                undocumented.push(format!(
                    "{}: {} is {:?} but its name says {expected:?}",
                    path.display(),
                    function.name,
                    function.kind
                ));
            }
            if function.kind != FunctionKind::User
                && function.kind != FunctionKind::Entry
                && function.owner.as_deref()
                    != function
                        .name
                        .strip_prefix(names::SUPER_PREFIX)
                        .unwrap_or(&function.name)
                        .rsplit_once('.')
                        .map(|(owner, _)| owner)
            {
                undocumented.push(format!(
                    "{}: {} owner {:?} disagrees with its name",
                    path.display(),
                    function.name,
                    function.owner
                ));
            }
            for block in &function.blocks {
                for instruction in &block.instructions {
                    let bn::ir::Instruction::Constant {
                        value: bn::ir::Constant::Function(callee),
                        ..
                    } = instruction
                    else {
                        continue;
                    };
                    if callee.starts_with(names::SYNTHESISED_MARKER) && !names::is_intrinsic(callee)
                    {
                        undocumented.push(format!("{}: intrinsic callee {callee}", path.display()));
                    }
                }
            }
        }
    }
    assert!(
        lowered > 50,
        "only {lowered} fixtures lowered; corpus too small"
    );
    assert!(
        undocumented.is_empty(),
        "undocumented emitted names:\n  {}",
        undocumented.join("\n  ")
    );
}

/// String literals a backend may still compare a callee name against. Since
/// `FunctionKind` (bucket 0.5.1c §3.2) function identity is read from the
/// IR; only the super-call prefix and the intrinsic callees remain a string
/// protocol. Synthesised suffixes (`.CONSTRUCTOR`, `.$fields`, …) are
/// informative and must not be matched by a backend.
fn documented_literals() -> Vec<String> {
    let mut literals = vec![names::SUPER_PREFIX.to_string()];
    literals.extend(names::INTRINSICS.iter().map(|name| (*name).to_string()));
    literals
}

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn backends_only_decode_documented_name_shapes() {
    let documented = documented_literals();
    let mut sources = Vec::new();
    rust_files(Path::new("crates/bn_llvm/src"), &mut sources);
    rust_files(Path::new("crates/bn_interp/src"), &mut sources);
    rust_files(Path::new("src/hosts"), &mut sources);
    rust_files(Path::new("src/libraries"), &mut sources);
    sources.push("src/runtime.rs".into());
    let mut offenders = Vec::new();
    for path in &sources {
        let text = fs::read_to_string(path).expect("read backend source");
        for (index, line) in text.lines().enumerate() {
            for call in [
                "ends_with(\"",
                "strip_suffix(\"",
                "strip_prefix(\"",
                "== \"",
                "Some(\"",
            ] {
                let mut rest = line;
                while let Some(start) = rest.find(call) {
                    let literal_start = start + call.len();
                    let Some(len) = rest[literal_start..].find('"') else {
                        break;
                    };
                    let literal = &rest[literal_start..literal_start + len];
                    // Only literals that look like the IR's synthesised shapes
                    // are in scope: a `$` marker, an all-caps `.WORD` suffix,
                    // the entry name, or the super prefix. Library member names
                    // (`.Queue.Concurrent`, `HOST.Exec.Result`) are a different
                    // contract and are not judged here.
                    // The entry name is language surface (FUNCTION Start),
                    // shared through `names::ENTRY`; a bare "Start" literal
                    // here is a library member (BNWeb.Server.Start), not judged.
                    let looks_synthesised = literal.contains('$')
                        || literal.starts_with('@')
                        || literal.rsplit('.').next().is_some_and(|tail| {
                            tail.len() > 3
                                && tail.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
                                && literal.starts_with('.')
                        });
                    if looks_synthesised && !documented.iter().any(|d| d == literal) {
                        offenders.push(format!("{}:{}: {literal:?}", path.display(), index + 1));
                    }
                    rest = &rest[literal_start + len..];
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "backend decodes undocumented name shapes:\n  {}",
        offenders.join("\n  ")
    );
}
