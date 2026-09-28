// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Developer tool: file-level component and call graph of the workspace, as
//! GML. Parses every `src/**/*.rs` of each workspace package with `syn`.
//! Calls are resolved through each file's `use` map, without type
//! inference; every edge carries a confidence (see the graph `comment`).
//! Run through `scripts/src-graph.sh`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use syn::visit::Visit;

struct Package {
    name: String, // crate name as written in Rust paths (`-` -> `_`)
    src: PathBuf,
    deps: BTreeSet<String>,
}

struct FileInfo {
    rel: String,
    krate: String,
    lines: usize,
    sha256: String,
    components: Vec<String>,
    calls: Vec<Call>,
    uses: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
    parse_error: Option<String>,
}

enum Call {
    Path(Vec<String>),
    Method(String),
    /// `self.m()` inside `impl T`: resolved through `(T, m)`.
    SelfMethod(String, String),
}

#[derive(Default)]
struct Defs {
    // name -> (file index, crate)
    free_fns: HashMap<String, Vec<usize>>,
    // (type, method) -> files
    type_methods: HashMap<(String, String), Vec<usize>>,
    // method -> files
    methods: HashMap<String, Vec<usize>>,
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Meta>()
                .is_ok_and(|meta| meta.path().is_ident("test"))
    }) || attrs.iter().any(|attr| attr.path().is_ident("test"))
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        syn::Type::Reference(reference) => type_name(&reference.elem),
        _ => "?".into(),
    }
}

/// Collects components (items) and call sites of one file, skipping
/// `#[cfg(test)]` modules and `#[test]` functions.
struct Collector {
    components: Vec<String>,
    calls: Vec<Call>,
    free_fns: Vec<String>,
    type_methods: Vec<(String, String)>,
    prefix: Vec<String>,
    impl_ty: Option<String>,
    uses: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
    /// Names bound by `let` or parameters in the enclosing fn (closures and
    /// function values): calling one is not a call to an item.
    locals: Vec<BTreeSet<String>>,
}

fn pattern_names(pat: &syn::Pat, out: &mut BTreeSet<String>) {
    match pat {
        syn::Pat::Ident(ident) => {
            out.insert(ident.ident.to_string());
        }
        syn::Pat::Type(typed) => pattern_names(&typed.pat, out),
        syn::Pat::Tuple(tuple) => tuple.elems.iter().for_each(|pat| pattern_names(pat, out)),
        syn::Pat::Reference(reference) => pattern_names(&reference.pat, out),
        _ => {}
    }
}

fn signature_locals(sig: &syn::Signature) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for input in &sig.inputs {
        if let syn::FnArg::Typed(typed) = input {
            pattern_names(&typed.pat, &mut names);
        }
    }
    names
}

/// Flattens one `use` tree into local name -> full path, and glob prefixes.
fn flatten_use(
    tree: &syn::UseTree,
    prefix: &mut Vec<String>,
    uses: &mut HashMap<String, Vec<String>>,
    globs: &mut Vec<Vec<String>>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            flatten_use(&path.tree, prefix, uses, globs);
            prefix.pop();
        }
        syn::UseTree::Name(name) if name.ident == "self" => {
            if let Some(last) = prefix.last() {
                uses.insert(last.clone(), prefix.clone());
            }
        }
        syn::UseTree::Name(name) => {
            let mut full = prefix.clone();
            full.push(name.ident.to_string());
            uses.insert(name.ident.to_string(), full);
        }
        syn::UseTree::Rename(rename) => {
            let mut full = prefix.clone();
            if rename.ident != "self" {
                full.push(rename.ident.to_string());
            }
            uses.insert(rename.rename.to_string(), full);
        }
        syn::UseTree::Glob(_) => globs.push(prefix.clone()),
        syn::UseTree::Group(group) => {
            for item in &group.items {
                flatten_use(item, prefix, uses, globs);
            }
        }
    }
}

impl Collector {
    fn qualified(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", self.prefix.join("::"))
        }
    }
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        flatten_use(&node.tree, &mut Vec::new(), &mut self.uses, &mut self.globs);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        if node.content.is_some() {
            self.components
                .push(format!("mod {}", self.qualified(&node.ident.to_string())));
            self.prefix.push(node.ident.to_string());
            syn::visit::visit_item_mod(self, node);
            self.prefix.pop();
        } else {
            self.components.push(format!(
                "mod {} (file)",
                self.qualified(&node.ident.to_string())
            ));
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let name = node.sig.ident.to_string();
        self.components
            .push(format!("fn {}", self.qualified(&name)));
        self.free_fns.push(name);
        self.locals.push(signature_locals(&node.sig));
        syn::visit::visit_item_fn(self, node);
        self.locals.pop();
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.components.push(format!(
            "struct {}",
            self.qualified(&node.ident.to_string())
        ));
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.components
            .push(format!("enum {}", self.qualified(&node.ident.to_string())));
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        let name = node.ident.to_string();
        self.components
            .push(format!("trait {}", self.qualified(&name)));
        for item in &node.items {
            if let syn::TraitItem::Fn(method) = item {
                self.type_methods
                    .push((name.clone(), method.sig.ident.to_string()));
            }
        }
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.components
            .push(format!("type {}", self.qualified(&node.ident.to_string())));
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.components
            .push(format!("const {}", self.qualified(&node.ident.to_string())));
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        self.components.push(format!(
            "static {}",
            self.qualified(&node.ident.to_string())
        ));
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(ident) = &node.ident {
            self.components
                .push(format!("macro {}", self.qualified(&ident.to_string())));
        }
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let self_ty = type_name(&node.self_ty);
        let header = match &node.trait_ {
            Some((_, path, _)) => format!(
                "impl {} for {self_ty}",
                path.segments
                    .last()
                    .map(|segment| segment.ident.to_string())
                    .unwrap_or_default()
            ),
            None => format!("impl {self_ty}"),
        };
        let methods: Vec<String> = node
            .items
            .iter()
            .filter_map(|item| match item {
                syn::ImplItem::Fn(method) if !is_cfg_test(&method.attrs) => {
                    Some(method.sig.ident.to_string())
                }
                _ => None,
            })
            .collect();
        for method in &methods {
            self.type_methods.push((self_ty.clone(), method.clone()));
        }
        self.components
            .push(format!("{header} {{{}}}", methods.join(", ")));
        let outer = self.impl_ty.replace(self_ty);
        syn::visit::visit_item_impl(self, node);
        self.impl_ty = outer;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        self.locals.push(signature_locals(&node.sig));
        syn::visit::visit_impl_item_fn(self, node);
        self.locals.pop();
    }

    fn visit_local(&mut self, node: &'ast syn::Local) {
        if let Some(scope) = self.locals.last_mut() {
            pattern_names(&node.pat, scope);
        }
        syn::visit::visit_local(self, node);
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let local = matches!(&*node.func, syn::Expr::Path(path)
        if path.path.segments.len() == 1
            && self.locals.last().is_some_and(|scope| {
                scope.contains(&path.path.segments[0].ident.to_string())
            }));
        if let syn::Expr::Path(path) = &*node.func
            && !local
        {
            self.calls.push(Call::Path(
                path.path
                    .segments
                    .iter()
                    .map(|segment| segment.ident.to_string())
                    .collect(),
            ));
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let is_self =
            matches!(&*node.receiver, syn::Expr::Path(path) if path.path.is_ident("self"));
        match (&self.impl_ty, is_self) {
            (Some(ty), true) => self
                .calls
                .push(Call::SelfMethod(ty.clone(), node.method.to_string())),
            _ => self.calls.push(Call::Method(node.method.to_string())),
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
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

fn packages(root: &Path) -> Vec<Package> {
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
    let members: BTreeSet<String> = json["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| package["name"].as_str().unwrap().replace('-', "_"))
        .collect();
    json["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            let manifest = PathBuf::from(package["manifest_path"].as_str().unwrap());
            Package {
                name: package["name"].as_str().unwrap().replace('-', "_"),
                src: manifest.parent().unwrap().join("src"),
                deps: package["dependencies"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|dep| dep["name"].as_str().unwrap().replace('-', "_"))
                    .filter(|dep| members.contains(dep))
                    .collect(),
            }
        })
        .collect()
}

/// Resolves a path call made in file `from` to the defining files. Follows
/// re-exports (`pub use`) in a crate's root file, up to `depth` levels.
#[allow(clippy::too_many_lines)] // one resolution rule per path shape
fn resolve_path(
    files: &[FileInfo],
    definitions: &Defs,
    deps: &HashMap<String, BTreeSet<String>>,
    from: usize,
    segments: &[String],
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
            definitions
                .free_fns
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
        let found = definitions
            .free_fns
            .get(&name)
            .map(in_crates)
            .unwrap_or_default();
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
    let reexport = files[root].uses.get(path.first()?)?.clone();
    let mut rest = reexport;
    rest.extend(path.iter().skip(1).cloned());
    if rest
        .first()
        .is_some_and(|first| !deps.contains_key(first.as_str()) && first != "crate")
    {
        rest.insert(0, "crate".into());
    }
    resolve_path(files, definitions, deps, root, &rest, depth - 1)
}

fn gml_string(text: &str) -> String {
    text.replace('&', "&amp;").replace('"', "&quot;")
}

/// Calls from one file to another: counts per kind [path, self, guessed],
/// with the confidently resolved names and the guessed ones.
#[derive(Default)]
struct Edge {
    counts: [usize; 3],
    names: BTreeSet<String>,
    guessed: BTreeSet<String>,
}

type Deps = HashMap<String, BTreeSet<String>>;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--check") {
        let root = PathBuf::from(args.get(1).expect("workspace root"));
        let graph = PathBuf::from(args.get(2).expect("graph .gml"));
        let root = root.canonicalize().unwrap();
        std::process::exit(i32::from(!check(&root, &graph)));
    }
    let root = PathBuf::from(args.first().expect("workspace root"));
    let out = PathBuf::from(args.get(1).expect("output .gml"));
    let root = root.canonicalize().unwrap();
    let packages = packages(&root);
    let deps: Deps = packages
        .iter()
        .map(|package| (package.name.clone(), package.deps.clone()))
        .collect();
    let (files, definitions) = collect(&root, &packages);
    let (edges, unresolved_methods) = link(&files, &definitions, &deps);
    write_gml(&out, &files, &edges, &unresolved_methods);
}

/// Parses every source file and indexes its definitions.
fn collect(root: &Path, packages: &[Package]) -> (Vec<FileInfo>, Defs) {
    let mut files: Vec<FileInfo> = Vec::new();
    let mut definitions = Defs::default();
    for package in packages {
        let mut paths = Vec::new();
        rust_files(&package.src, &mut paths);
        for path in paths {
            let source = std::fs::read_to_string(&path).expect("read source");
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            let index = files.len();
            let mut info = FileInfo {
                rel,
                krate: package.name.clone(),
                lines: source.lines().count(),
                sha256: sha256_hex(source.as_bytes()),
                components: Vec::new(),
                calls: Vec::new(),
                uses: HashMap::new(),
                globs: Vec::new(),
                parse_error: None,
            };
            match syn::parse_file(&source) {
                Ok(ast) => {
                    let mut collector = Collector {
                        components: Vec::new(),
                        calls: Vec::new(),
                        free_fns: Vec::new(),
                        type_methods: Vec::new(),
                        prefix: Vec::new(),
                        impl_ty: None,
                        uses: HashMap::new(),
                        globs: Vec::new(),
                        locals: Vec::new(),
                    };
                    collector.visit_file(&ast);
                    for name in collector.free_fns {
                        definitions.free_fns.entry(name).or_default().push(index);
                    }
                    for (ty, method) in collector.type_methods {
                        definitions
                            .methods
                            .entry(method.clone())
                            .or_default()
                            .push(index);
                        definitions
                            .type_methods
                            .entry((ty, method))
                            .or_default()
                            .push(index);
                    }
                    info.components = collector.components;
                    info.calls = collector.calls;
                    info.uses = collector.uses;
                    info.globs = collector.globs;
                }
                Err(error) => info.parse_error = Some(error.to_string()),
            }
            files.push(info);
        }
    }

    (files, definitions)
}

/// Resolves every call site to file-to-file edges; also returns, per file,
/// the method calls that matched several files and were left unlinked.
fn link(
    files: &[FileInfo],
    definitions: &Defs,
    deps: &Deps,
) -> (BTreeMap<(usize, usize), Edge>, Vec<usize>) {
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

    let mut edges: BTreeMap<(usize, usize), Edge> = BTreeMap::new();
    let mut unresolved_methods = vec![0_usize; files.len()];
    for (caller, info) in files.iter().enumerate() {
        for call in &info.calls {
            let (targets, kind, name) = match call {
                Call::Path(segments) => {
                    let Some(targets) = resolve_path(files, definitions, deps, caller, segments, 3)
                    else {
                        continue;
                    };
                    (dedup(targets), 0, segments.last().unwrap().clone())
                }
                Call::SelfMethod(ty, name) => {
                    let targets = definitions
                        .type_methods
                        .get(&(ty.clone(), name.clone()))
                        .map(|found| visible(&info.krate, found))
                        .unwrap_or_default();
                    (dedup(targets), 1, name.clone())
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
                    (targets, 2, name.clone())
                }
            };
            for callee in targets {
                if callee == caller {
                    continue;
                }
                let entry = edges.entry((caller, callee)).or_default();
                entry.counts[kind] += 1;
                if kind == 2 {
                    entry.guessed.insert(name.clone());
                } else {
                    entry.names.insert(name.clone());
                }
            }
        }
    }

    (edges, unresolved_methods)
}

fn write_gml(
    out: &Path,
    files: &[FileInfo],
    edges: &BTreeMap<(usize, usize), Edge>,
    unresolved_methods: &[usize],
) {
    let mut gml = String::new();
    gml.push_str("graph [\n  directed 1\n");
    let _ = writeln!(
        gml,
        "  sourcesSha256 \"{}\"",
        sources_digest(
            files
                .iter()
                .map(|info| (info.rel.as_str(), info.sha256.as_str()))
        )
    );
    let _ = writeln!(gml, "  generatorSha256 \"{}\"", generator_digest());
    gml.push_str(
        "  comment \"Basic Next: one node per src/**/*.rs file of each workspace package. \
Node components = items parsed with syn (fn, struct, enum, trait, impl with its methods, \
const, static, type, macro_rules, mod); #[cfg(test)] modules and #[test] fns are skipped. \
Edges = call sites from one file to another. No type inference. \
calls: path calls f() / Type::f() / crate::f(), linked to same-name definitions in the \
caller's crate, else in its workspace dependencies (several files if several define it). \
selfCalls: self.m() inside impl T, linked to files defining T::m. \
guessedCalls (LOW confidence): x.m() on any other receiver, counted only when exactly \
one visible file defines a method m; std methods with the same name produce false edges. \
confidence = high when calls + selfCalls > 0, else low. weight = calls + selfCalls. \
ambiguousMethodCalls on a node = x.m() calls matching several files (not linked). \
std/external calls, macros, and type uses are not edges. Self-edges are omitted.\"\n",
    );
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
    let join = |set: &BTreeSet<String>| {
        gml_string(
            &set.iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        )
    };
    let (mut high, mut low) = (0, 0);
    for ((source, target), edge) in edges {
        let [calls, self_calls, guessed] = edge.counts;
        let confident = calls + self_calls > 0;
        if confident {
            high += 1;
        } else {
            low += 1;
        }
        let _ = writeln!(gml, "  edge [");
        let _ = writeln!(gml, "    source {source}");
        let _ = writeln!(gml, "    target {target}");
        let _ = writeln!(gml, "    calls {calls}");
        let _ = writeln!(gml, "    selfCalls {self_calls}");
        let _ = writeln!(gml, "    guessedCalls {guessed}");
        let _ = writeln!(gml, "    weight {}", calls + self_calls);
        let _ = writeln!(
            gml,
            "    confidence \"{}\"",
            if confident { "high" } else { "low" }
        );
        let _ = writeln!(gml, "    names \"{}\"", join(&edge.names));
        let _ = writeln!(gml, "    guessedNames \"{}\"", join(&edge.guessed));
        let _ = writeln!(gml, "  ]");
    }
    gml.push_str("]\n");
    std::fs::write(out, gml).expect("write gml");
    let parse_errors = files
        .iter()
        .filter(|info| info.parse_error.is_some())
        .count();
    eprintln!(
        "files {} edges {} (high {high}, low {low}) parse errors {parse_errors}",
        files.len(),
        edges.len()
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

/// One digest over every (path, file digest) pair, in path order.
fn sources_digest<'a>(pairs: impl Iterator<Item = (&'a str, &'a str)>) -> String {
    let mut pairs: Vec<_> = pairs.collect();
    pairs.sort_unstable();
    let mut text = String::new();
    for (path, digest) in pairs {
        let _ = writeln!(text, "{path}\0{digest}");
    }
    sha256_hex(text.as_bytes())
}

/// Digest of this generator's source: a graph from another version of the
/// generator is stale even when no source file changed.
fn generator_digest() -> String {
    sha256_hex(include_str!("main.rs").as_bytes())
}

/// Current `src/**/*.rs` files and their digests, keyed by relative path.
fn source_hashes(root: &Path) -> BTreeMap<String, String> {
    let mut hashes = BTreeMap::new();
    for package in packages(root) {
        let mut paths = Vec::new();
        rust_files(&package.src, &mut paths);
        for path in paths {
            let bytes = std::fs::read(&path).expect("read source");
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            hashes.insert(rel, sha256_hex(&bytes));
        }
    }
    hashes
}

/// Compares a written graph with the current sources. Prints what changed
/// and returns whether the graph is up to date.
fn check(root: &Path, graph: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(graph) else {
        println!("missing: {}", graph.display());
        return false;
    };
    let quoted = |line: &str, key: &str| -> Option<String> {
        let value = line.trim_start().strip_prefix(key)?.strip_prefix(" \"")?;
        Some(
            value
                .strip_suffix('"')?
                .replace("&quot;", "\"")
                .replace("&amp;", "&"),
        )
    };
    let mut recorded = BTreeMap::new();
    let mut generator = None;
    let mut label = None;
    for line in text.lines() {
        if let Some(value) = quoted(line, "generatorSha256") {
            generator = Some(value);
        } else if let Some(value) = quoted(line, "label") {
            label = Some(value);
        } else if let Some(value) = quoted(line, "sha256")
            && let Some(path) = label.take()
        {
            recorded.insert(path, value);
        }
    }
    let current = source_hashes(root);
    let mut fresh = true;
    if generator.as_deref() != Some(generator_digest().as_str()) {
        println!("generator: changed");
        fresh = false;
    }
    for (path, digest) in &current {
        match recorded.get(path) {
            None => println!("added: {path}"),
            Some(old) if old != digest => println!("changed: {path}"),
            Some(_) => continue,
        }
        fresh = false;
    }
    for path in recorded.keys().filter(|path| !current.contains_key(*path)) {
        println!("removed: {path}");
        fresh = false;
    }
    if fresh {
        println!("up to date: {}", graph.display());
    }
    fresh
}
