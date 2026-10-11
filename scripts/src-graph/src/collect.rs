// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Parses each source file with `syn`: its items (components), call sites,
//! type references, `use` map, and `pub` items.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use syn::punctuated::Punctuated;
use syn::visit::Visit;

use crate::fresh::sha256_hex;
use crate::model::{Call, Defs, FileInfo, Package, PubItem, rust_files};

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("test")
            || (attr.path().is_ident("cfg")
                && attr
                    .parse_args::<syn::Meta>()
                    .is_ok_and(|meta| meta.path().is_ident("test")))
    })
}

/// `#[no_mangle]`, `#[unsafe(no_mangle)]`, `#[export_name]`: a C export,
/// public by contract whatever Rust code uses it.
fn is_c_export(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let text = match &attr.meta {
            syn::Meta::List(list) => format!(
                "{} {}",
                list.path
                    .get_ident()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                list.tokens
            ),
            meta => meta
                .path()
                .get_ident()
                .map(ToString::to_string)
                .unwrap_or_default(),
        };
        text.contains("no_mangle") || text.contains("export_name")
    })
}

fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
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

/// Collects components (items), call sites and type references of one
/// file, skipping `#[cfg(test)]` modules and `#[test]` functions.
#[derive(Default)]
struct Collector {
    components: Vec<String>,
    calls: Vec<Call>,
    free_fns: Vec<String>,
    type_methods: Vec<(String, String)>,
    types: Vec<String>,
    pub_items: Vec<PubItem>,
    prefix: Vec<String>,
    impl_ty: Option<String>,
    uses: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
    /// Names bound by `let` or parameters in the enclosing fn (closures and
    /// function values): calling one is not a call to an item.
    locals: Vec<BTreeSet<String>>,
}

impl Collector {
    fn qualified(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", self.prefix.join("::"))
        }
    }

    /// Records item `ident` of `kind` as a component, and as a `pub` item
    /// when `public`.
    fn item(&mut self, kind: &'static str, ident: &syn::Ident, public: bool) {
        let name = ident.to_string();
        self.components
            .push(format!("{kind} {}", self.qualified(&name)));
        if public {
            self.pub_items.push(PubItem {
                kind,
                label: self.qualified(&name),
                name,
                line: ident.span().start().line,
            });
        }
    }

    fn type_item(&mut self, kind: &'static str, ident: &syn::Ident, vis: &syn::Visibility) {
        self.item(kind, ident, is_pub(vis));
        self.types.push(ident.to_string());
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
            self.item("mod", &node.ident, is_pub(&node.vis));
            self.prefix.push(node.ident.to_string());
            syn::visit::visit_item_mod(self, node);
            self.prefix.pop();
        } else {
            self.components.push(format!(
                "mod {} (file)",
                self.qualified(&node.ident.to_string())
            ));
            if is_pub(&node.vis) {
                self.pub_items.push(PubItem {
                    kind: "mod",
                    label: self.qualified(&node.ident.to_string()),
                    name: node.ident.to_string(),
                    line: node.ident.span().start().line,
                });
            }
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if is_cfg_test(&node.attrs) {
            return;
        }
        let public = is_pub(&node.vis) && node.sig.abi.is_none() && !is_c_export(&node.attrs);
        self.item("fn", &node.sig.ident, public);
        self.free_fns.push(node.sig.ident.to_string());
        self.locals.push(signature_locals(&node.sig));
        syn::visit::visit_item_fn(self, node);
        self.locals.pop();
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.type_item("struct", &node.ident, &node.vis);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.type_item("enum", &node.ident, &node.vis);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        self.type_item("union", &node.ident, &node.vis);
        syn::visit::visit_item_union(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.type_item("trait", &node.ident, &node.vis);
        let name = node.ident.to_string();
        for item in &node.items {
            if let syn::TraitItem::Fn(method) = item {
                self.type_methods
                    .push((name.clone(), method.sig.ident.to_string()));
            }
        }
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.type_item("type", &node.ident, &node.vis);
        syn::visit::visit_item_type(self, node);
    }

    fn visit_item_const(&mut self, node: &'ast syn::ItemConst) {
        self.item("const", &node.ident, is_pub(&node.vis));
        syn::visit::visit_item_const(self, node);
    }

    fn visit_item_static(&mut self, node: &'ast syn::ItemStatic) {
        let public = is_pub(&node.vis) && !is_c_export(&node.attrs);
        self.item("static", &node.ident, public);
        syn::visit::visit_item_static(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        // `macro_rules!` bodies are token patterns, not code: not visited.
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
        let mut methods = Vec::new();
        for item in &node.items {
            let syn::ImplItem::Fn(method) = item else {
                continue;
            };
            if is_cfg_test(&method.attrs) {
                continue;
            }
            let name = method.sig.ident.to_string();
            if node.trait_.is_none() && is_pub(&method.vis) && !is_c_export(&method.attrs) {
                self.pub_items.push(PubItem {
                    kind: "method",
                    label: self.qualified(&format!("{self_ty}::{name}")),
                    name: name.clone(),
                    line: method.sig.ident.span().start().line,
                });
            }
            self.type_methods.push((self_ty.clone(), name.clone()));
            methods.push(name);
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

    /// Every path that names a type: in a type, a pattern, an expression,
    /// an `impl` header or a bound.
    fn visit_path(&mut self, node: &'ast syn::Path) {
        let segments: Vec<String> = node
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if let Some(cut) = segments
            .iter()
            .position(|segment| segment.starts_with(char::is_uppercase))
            && segments[cut] != "Self"
        {
            self.calls.push(Call::Type(segments[..=cut].to_vec()));
        }
        syn::visit::visit_path(self, node);
    }

    /// Macro arguments that parse as comma-separated expressions
    /// (`format!`, `vec![..]`, `assert!`, `write!`): their calls count.
    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        if let Ok(args) =
            node.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated)
        {
            for expr in &args {
                self.visit_expr(expr);
            }
        }
        syn::visit::visit_macro(self, node);
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

/// Parses every source file and indexes its definitions.
pub fn collect(root: &Path, packages: &[Package]) -> (Vec<FileInfo>, Defs) {
    let mut files: Vec<FileInfo> = Vec::new();
    let mut definitions = Defs::default();
    for package in packages {
        let mut paths = Vec::new();
        rust_files(&package.src, &mut paths);
        for path in paths {
            let source = std::fs::read_to_string(&path).expect("read source");
            let index = files.len();
            let mut info = FileInfo {
                rel: path.strip_prefix(root).unwrap().display().to_string(),
                krate: package.name.clone(),
                lines: source.lines().count(),
                sha256: sha256_hex(source.as_bytes()),
                components: Vec::new(),
                calls: Vec::new(),
                uses: HashMap::new(),
                globs: Vec::new(),
                pub_items: Vec::new(),
                parse_error: None,
            };
            match syn::parse_file(&source) {
                Ok(ast) => {
                    let mut collector = Collector::default();
                    collector.visit_file(&ast);
                    for name in collector.free_fns {
                        definitions.free_fns.entry(name).or_default().push(index);
                    }
                    for name in collector.types {
                        definitions.types.entry(name).or_default().push(index);
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
                    info.pub_items = collector.pub_items;
                }
                Err(error) => info.parse_error = Some(error.to_string()),
            }
            files.push(info);
        }
    }
    (files, definitions)
}
