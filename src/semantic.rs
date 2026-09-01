// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{HashMap, HashSet};

use crate::{
    ast::{
        Block, DeclarationKind, Expression, ExpressionKind, ForHeader, FunctionSignature, Item,
        Literal, Program, Statement, TypeReference,
    },
    diagnostic::Diagnostic,
    module_graph::{ModuleGraph, ModuleId},
    source::Span,
};

#[path = "semantic/helpers1.rs"]
mod helpers1;
pub(crate) use helpers1::*;
#[path = "semantic/helpers2.rs"]
mod helpers2;
pub(crate) use helpers2::*;
#[path = "semantic/helpers3.rs"]
mod helpers3;
pub(crate) use helpers3::*;

#[must_use]
pub fn static_len(ty: &Type) -> Option<u64> {
    helpers1::static_len(ty)
}

#[must_use]
pub fn static_size_of(ty: &Type) -> Option<u64> {
    helpers1::static_size_of(ty)
}

#[must_use]
pub fn integer_byte_size(kind: IntegerType) -> u64 {
    helpers1::integer_byte_size(kind)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SymbolId(u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TypeId(u32);

impl SymbolId {
    #[must_use]
    pub fn value(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw(id: u32) -> Self {
        Self(id)
    }
}

impl SemanticModel {
    #[must_use]
    pub fn expression(&self, span: Span) -> Option<&ResolvedExpression> {
        self.expressions
            .iter()
            .find(|expression| expression.span == span)
    }

    #[must_use]
    pub fn symbol_at(&self, span: Span) -> Option<&ResolvedSymbol> {
        self.symbols.iter().find(|symbol| symbol.span == span)
    }
}

#[derive(Debug)]
pub struct ResolvedSymbol {
    pub id: SymbolId,
    pub type_id: TypeId,
    pub name: String,
    pub type_name: String,
    pub ty: Type,
    pub constant: bool,
    pub span: Span,
}

#[derive(Debug)]
pub struct SemanticModel {
    pub symbols: Vec<ResolvedSymbol>,
    pub expressions: Vec<ResolvedExpression>,
    pub layouts: HashMap<String, u64>,
    pub base_classes: HashMap<String, String>,
    pub(crate) bnmath_modules: HashSet<ModuleId>,
}

impl SemanticModel {
    #[must_use]
    pub fn size_of(&self, ty: &Type) -> Option<u64> {
        if let Some(size) = static_size_of(ty) {
            return Some(size);
        }
        match ty {
            Type::Named(name) => self.layouts.get(name).copied(),
            Type::Vector {
                element,
                dimensions,
            } => self
                .size_of(element)
                .and_then(|element| dimension_product(dimensions)?.checked_mul(element)),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct ResolvedExpression {
    pub span: Span,
    pub type_name: String,
    pub ty: Type,
    pub symbol_id: Option<SymbolId>,
    pub member_target: Option<MemberTarget>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberTarget {
    pub module: Option<ModuleId>,
    pub owner: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Boolean,
    Integer(IntegerType),
    IntegerLiteral(String),
    Float(FloatType),
    FloatLiteral,
    String,
    Null,
    NotAvailable,
    EndOfFile,
    System,
    HostClock,
    HostRandom,
    HostConsole,
    HostFileSystem,
    HostNet,
    HostArgs,
    Named(String),
    TypeName(String),
    ImportedNamed {
        module: ModuleId,
        name: String,
    },
    ImportedTypeName {
        module: ModuleId,
        name: String,
    },
    Module(ModuleId),
    Function {
        parameters: Vec<Type>,
        return_type: Box<Type>,
    },
    Vector {
        element: Box<Type>,
        dimensions: Vec<u64>,
    },
    Pointer {
        element: Box<Type>,
        length: PointerLength,
    },
    Alternative(Vec<Type>),
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegerType {
    Byte,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt16,
    UInt32,
    UInt64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatType {
    Float32,
    Float64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerLength {
    One,
    Fixed(u64),
    Dynamic,
}

#[path = "semantic/type_ops.rs"]
mod type_ops;
pub(crate) use type_ops::*;
#[derive(Clone)]
struct Symbol {
    id: SymbolId,
    type_id: TypeId,
    ty: Type,
    declared_ty: Type,
    constant: bool,
}
#[derive(Clone)]
pub(crate) struct Member {
    ty: Type,
    is_static: bool,
    private: bool,
    mutable: bool,
}

#[derive(Clone)]
struct Constructor {
    parameters: Vec<Type>,
    public: bool,
}
#[derive(Clone)]
pub(crate) struct ImportedTypeInfo {
    kind: DeclarationKind,
    members: HashMap<String, Member>,
    constructor: Option<Constructor>,
    interfaces: Vec<String>,
}
struct Analyzer {
    globals: HashMap<String, Symbol>,
    members: HashMap<String, HashMap<String, Member>>,
    module_exports: HashMap<ModuleId, HashMap<String, Type>>,
    module_imports: HashMap<String, ModuleId>,
    current_class: Option<String>,
    declaration_kinds: HashMap<String, DeclarationKind>,
    constructors: HashMap<String, Constructor>,
    base_classes: HashMap<String, String>,
    declared_members: HashMap<String, std::collections::HashSet<String>>,
    implementations: HashMap<String, Vec<String>>,
    imported_types: HashMap<(ModuleId, String), ImportedTypeInfo>,
    bnmath_modules: HashSet<ModuleId>,
    standard_modules: HashSet<ModuleId>,
    next_symbol: u32,
    next_type: u32,
    symbols: Vec<ResolvedSymbol>,
    expressions: Vec<ResolvedExpression>,
    layouts: HashMap<String, u64>,
    executable_module: bool,
    allow_variable_vectors: bool,
}

/// Resolves names and applies the currently implemented static semantic rules.
///
/// # Errors
///
/// Returns the first diagnostic found in source order.
pub fn analyze(program: &Program) -> Result<SemanticModel, Diagnostic> {
    analyze_with_modules(
        program,
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashSet::new(),
        HashSet::new(),
        true,
        false,
    )
}

#[derive(Debug)]
pub struct ModuleAnalysisError {
    pub module: ModuleId,
    pub diagnostic: Diagnostic,
}

/// Resolves every module after the import graph has been made acyclic.
///
/// # Errors
///
/// Returns the module that owns the first semantic diagnostic.
pub fn analyze_modules(graph: &ModuleGraph) -> Result<Vec<SemanticModel>, ModuleAnalysisError> {
    let exports = graph
        .modules
        .iter()
        .map(|module| (module.id, exported_declarations(module.id, &module.program)))
        .collect::<HashMap<_, _>>();
    let imported_types = imported_type_catalog(graph);
    let mut models = Vec::with_capacity(graph.modules.len());
    for module in &graph.modules {
        if module.id != graph.root
            && let Some(Item::Import { span, .. }) = module.program.items.iter().find(|item| {
                matches!(item, Item::Import { path, .. } if path == &["HOST".to_string(), "Main".to_string()])
            })
        {
            return Err(ModuleAnalysisError {
                module: module.id,
                diagnostic: error(
                    "HOST_IMPORT_SCOPE",
                    "only the executable module may import HOST.Main",
                    *span,
                ),
            });
        }
        if module.id != graph.root
            && let Some(Item::Declaration { span, .. }) = module.program.items.iter().find(|item| {
                matches!(item, Item::Declaration { kind: DeclarationKind::Function, name, .. } if name == "Start")
            })
        {
            return Err(ModuleAnalysisError {
                module: module.id,
                diagnostic: error(
                    "IMPORTED_START",
                    "an imported module must not declare Start",
                    *span,
                ),
            });
        }
        let imported_modules = module_imports(module);
        let bnmath_modules = graph
            .modules
            .iter()
            .filter_map(|loaded| {
                (loaded.standard_module == Some(crate::module_graph::StandardModule::BNMath))
                    .then_some(loaded.id)
            })
            .collect();
        let standard_modules = graph
            .modules
            .iter()
            .filter_map(|loaded| loaded.standard_module.map(|_| loaded.id))
            .collect();
        let model = analyze_with_modules(
            &module.program,
            exports.clone(),
            imported_modules,
            imported_types.clone(),
            bnmath_modules,
            standard_modules,
            module.id == graph.root,
            module.standard_module.is_some(),
        )
        .map_err(|diagnostic| ModuleAnalysisError {
            module: module.id,
            diagnostic,
        })?;
        models.push(model);
    }
    Ok(models)
}

#[path = "semantic/module_analysis.rs"]
mod module_analysis;
pub(crate) use module_analysis::*;
#[path = "semantic/analyzer1.rs"]
mod analyzer1;
#[path = "semantic/analyzer2.rs"]
mod analyzer2;
#[path = "semantic/analyzer3.rs"]
mod analyzer3;
#[path = "semantic/analyzer4.rs"]
mod analyzer4;
#[path = "semantic/analyzer5.rs"]
mod analyzer5;
#[path = "semantic/analyzer6.rs"]
mod analyzer6;
#[path = "semantic/analyzer7.rs"]
mod analyzer7;
#[path = "semantic/analyzer8.rs"]
mod analyzer8;
#[path = "semantic/host_defaults.rs"]
mod host_defaults;
#[path = "semantic/host_members1.rs"]
mod host_members1;
#[path = "semantic/host_members2.rs"]
mod host_members2;
#[path = "semantic/host_members3.rs"]
mod host_members3;
#[path = "semantic/host_members4.rs"]
mod host_members4;
pub(crate) use analyzer8::{declaration_type, function_type, type_from_atom, type_from_reference};
#[path = "semantic/type_names.rs"]
mod type_names;
pub(crate) use type_names::*;
fn default_span() -> Span {
    Span {
        start: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
        end: crate::source::Position {
            offset: 0,
            line: 1,
            column: 1,
        },
    }
}
