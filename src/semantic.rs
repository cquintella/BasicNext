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

fn binary_type(
    operator: &str,
    left: &Type,
    right: &Type,
    expression: &Expression,
) -> Result<Type, Diagnostic> {
    let invalid = || {
        error(
            "TYPE_MISMATCH",
            format!(
                "operator {operator} cannot combine {} and {}",
                display(left),
                display(right)
            ),
            expression.span,
        )
    };

    match operator {
        "Assign" | "NotEqual" => {
            if comparable(left, right) {
                Ok(Type::Boolean)
            } else {
                Err(invalid())
            }
        }
        "Less" | "LessEqual" | "Greater" | "GreaterEqual" => {
            numeric_result(left, right).map_or_else(|| Err(invalid()), |_| Ok(Type::Boolean))
        }
        "AND" | "OR" | "XOR" if left == &Type::Boolean && right == &Type::Boolean => {
            Ok(Type::Boolean)
        }
        "AND" | "OR" | "XOR" | "DIV" | "Percent" => integer_result(left, right).ok_or_else(invalid),
        "SHL" | "SHR" => {
            if !is_integer(left) || !is_integer(right) {
                return Err(invalid());
            }
            let result = default_literal_type(left.clone());
            if let Some(count) = constant_integer_from_type(right)
                && (count < 0 || count >= i128::from(integer_width(&result)))
            {
                return Err(error(
                    "INVALID_SHIFT_COUNT",
                    "shift count must be non-negative and smaller than the left operand width",
                    expression.span,
                ));
            }
            Ok(result)
        }
        "Plus" if left == &Type::String && right == &Type::String => Ok(Type::String),
        "Slash" => numeric_result(left, right)
            .map(|_| Type::Float(FloatType::Float64))
            .ok_or_else(invalid),
        "Plus" | "Minus" | "Star" | "Power" => {
            if let Some(value) = constant_integer(expression) {
                return Ok(Type::IntegerLiteral(value.to_string()));
            }
            numeric_result(left, right).ok_or_else(invalid)
        }
        _ => Err(invalid()),
    }
}

fn conversion_allowed(source: &Type, target: &Type) -> bool {
    let numeric = |ty: &Type| {
        matches!(
            ty,
            Type::Integer(_) | Type::IntegerLiteral(_) | Type::Float(_) | Type::FloatLiteral
        )
    };
    numeric(source) && (numeric(target) || *target == Type::Boolean)
        || *source == Type::String && *target == Type::Boolean
        || matches!(source, Type::Null | Type::NotAvailable | Type::EndOfFile)
            && *target == Type::Boolean
}

fn comparable(left: &Type, right: &Type) -> bool {
    compatible(left, right) || compatible(right, left) || numeric_result(left, right).is_some()
}

fn compound_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "PlusAssign" => Some("Plus"),
        "MinusAssign" => Some("Minus"),
        "StarAssign" => Some("Star"),
        "SlashAssign" => Some("Slash"),
        "PercentAssign" => Some("Percent"),
        "PowerAssign" => Some("Power"),
        _ => None,
    }
}

fn numeric_result(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::IntegerLiteral(_), Type::Float(kind))
        | (Type::Float(kind), Type::IntegerLiteral(_)) => Some(Type::Float(*kind)),
        (Type::IntegerLiteral(_) | Type::FloatLiteral, Type::FloatLiteral)
        | (Type::FloatLiteral, Type::IntegerLiteral(_)) => Some(Type::Float(FloatType::Float64)),
        (Type::Float(left), Type::Float(right)) => Some(Type::Float(
            if left == &FloatType::Float64 || right == &FloatType::Float64 {
                FloatType::Float64
            } else {
                FloatType::Float32
            },
        )),
        (Type::Float(kind), Type::FloatLiteral) | (Type::FloatLiteral, Type::Float(kind)) => {
            Some(Type::Float(*kind))
        }
        _ => integer_result(left, right),
    }
}

fn integer_result(left: &Type, right: &Type) -> Option<Type> {
    match (left, right) {
        (Type::IntegerLiteral(left), Type::IntegerLiteral(right)) => {
            let left = parse_integer(left)?;
            let right = parse_integer(right)?;
            let minimum = left.min(right);
            let maximum = left.max(right);
            integer_kind_for_range(minimum, maximum).map(Type::Integer)
        }
        (Type::Integer(kind), Type::IntegerLiteral(value))
        | (Type::IntegerLiteral(value), Type::Integer(kind)) => {
            integer_literal_fits(value, *kind).then_some(Type::Integer(*kind))
        }
        (Type::Integer(left), Type::Integer(right)) => {
            promote_integers(*left, *right).map(Type::Integer)
        }
        _ => None,
    }
}

fn promote_integers(left: IntegerType, right: IntegerType) -> Option<IntegerType> {
    if left == right {
        return Some(left);
    }
    if left == IntegerType::Byte {
        return Some(if is_unsigned(right) {
            right
        } else {
            widest_signed(IntegerType::Int16, right)
        });
    }
    if right == IntegerType::Byte {
        return Some(if is_unsigned(left) {
            left
        } else {
            widest_signed(left, IntegerType::Int16)
        });
    }
    match (is_unsigned(left), is_unsigned(right)) {
        (true, true) => Some(
            if integer_width(&Type::Integer(left)) >= integer_width(&Type::Integer(right)) {
                left
            } else {
                right
            },
        ),
        (false, false) => Some(widest_signed(left, right)),
        _ => None,
    }
}

fn is_unsigned(kind: IntegerType) -> bool {
    matches!(
        kind,
        IntegerType::Byte | IntegerType::UInt16 | IntegerType::UInt32 | IntegerType::UInt64
    )
}

fn widest_signed(left: IntegerType, right: IntegerType) -> IntegerType {
    if integer_width(&Type::Integer(left)) >= integer_width(&Type::Integer(right)) {
        left
    } else {
        right
    }
}

fn integer_width(ty: &Type) -> u8 {
    match ty {
        Type::Integer(IntegerType::Byte | IntegerType::Int8) => 8,
        Type::Integer(IntegerType::Int16 | IntegerType::UInt16) => 16,
        Type::Integer(IntegerType::Int32 | IntegerType::UInt32) | Type::IntegerLiteral(_) => 32,
        Type::Integer(IntegerType::Int64 | IntegerType::UInt64) => 64,
        _ => 0,
    }
}

fn integer_kind_for_range(minimum: i128, maximum: i128) -> Option<IntegerType> {
    [IntegerType::Int32, IntegerType::Int64, IntegerType::UInt64]
        .into_iter()
        .find(|kind| {
            let (low, high) = integer_range(*kind);
            minimum >= low && maximum <= high
        })
}
#[derive(Clone, Debug)]
#[allow(dead_code)] // IDs become cross-stage handles in the validated AST.
struct Symbol {
    id: SymbolId,
    type_id: TypeId,
    ty: Type,
    declared_ty: Type,
    constant: bool,
}
#[derive(Clone)]
struct Member {
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
struct ImportedTypeInfo {
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

#[allow(clippy::too_many_arguments)] // Module analysis receives the graph catalogs directly.
fn analyze_with_modules(
    program: &Program,
    module_exports: HashMap<ModuleId, HashMap<String, Type>>,
    module_imports: HashMap<String, ModuleId>,
    imported_types: HashMap<(ModuleId, String), ImportedTypeInfo>,
    bnmath_modules: HashSet<ModuleId>,
    standard_modules: HashSet<ModuleId>,
    executable_module: bool,
    allow_variable_vectors: bool,
) -> Result<SemanticModel, Diagnostic> {
    let mut analyzer = Analyzer {
        globals: HashMap::new(),
        members: HashMap::new(),
        module_exports,
        module_imports,
        current_class: None,
        declaration_kinds: HashMap::new(),
        constructors: HashMap::new(),
        base_classes: HashMap::new(),
        declared_members: HashMap::new(),
        implementations: HashMap::new(),
        imported_types,
        bnmath_modules,
        standard_modules,
        next_symbol: 0,
        next_type: 0,
        symbols: Vec::new(),
        expressions: Vec::new(),
        layouts: HashMap::new(),
        executable_module,
        allow_variable_vectors,
    };
    analyzer.declare_globals(program)?;
    validate_implemented_interfaces(
        program,
        &analyzer.imported_types,
        &analyzer.module_imports,
        &analyzer.members,
    )?;
    analyzer.analyze_declarations(program)?;
    Ok(SemanticModel {
        symbols: analyzer.symbols,
        expressions: analyzer.expressions,
        layouts: analyzer.layouts,
        base_classes: analyzer.base_classes,
        bnmath_modules: analyzer.bnmath_modules,
    })
}

fn exported_declarations(module: ModuleId, program: &Program) -> HashMap<String, Type> {
    let mut exports = HashMap::new();
    for item in &program.items {
        let Item::Declaration {
            exported: true,
            name,
            kind,
            signature,
            statements,
            ..
        } = item
        else {
            continue;
        };
        let ty = signature
            .as_ref()
            .map_or_else(|| declaration_type(*kind, name), function_type);
        exports.insert(name.clone(), qualify_local_type(module, program, ty));
        if *kind == DeclarationKind::Class {
            for statement in statements {
                let Statement::Binding {
                    name: field,
                    type_ref,
                    is_static: true,
                    visibility,
                    ..
                } = statement
                else {
                    continue;
                };
                if *visibility != Some(crate::ast::Visibility::Public) {
                    continue;
                }
                exports.insert(
                    field.clone(),
                    qualify_local_type(module, program, type_from_reference(type_ref)),
                );
            }
        }
    }
    exports
}

#[allow(clippy::too_many_lines)] // Imported type catalogs preserve declaration shape explicitly.
fn imported_type_catalog(graph: &ModuleGraph) -> HashMap<(ModuleId, String), ImportedTypeInfo> {
    let mut catalog = HashMap::new();
    for module in &graph.modules {
        for item in &module.program.items {
            let Item::Declaration {
                exported: true,
                kind,
                name,
                interfaces,
                statements,
                ..
            } = item
            else {
                continue;
            };
            if *kind == DeclarationKind::Function {
                continue;
            }
            let mut members = HashMap::new();
            let mut constructor = None;
            for statement in statements {
                match statement {
                    Statement::Binding {
                        name: member_name,
                        type_ref,
                        is_static,
                        constant,
                        visibility,
                        ..
                    } if *kind != DeclarationKind::Class
                        || *visibility == Some(crate::ast::Visibility::Public) =>
                    {
                        members.insert(
                            member_name.clone(),
                            Member {
                                ty: qualify_local_type(
                                    module.id,
                                    &module.program,
                                    type_from_reference(type_ref),
                                ),
                                is_static: *is_static,
                                private: false,
                                mutable: !*constant,
                            },
                        );
                    }
                    Statement::MemberFunction {
                        name: member_name,
                        signature: Some(signature),
                        is_static,
                        visibility,
                        ..
                    } if *kind != DeclarationKind::Class
                        || *visibility == Some(crate::ast::Visibility::Public) =>
                    {
                        members.insert(
                            member_name.clone(),
                            Member {
                                ty: qualify_local_type(
                                    module.id,
                                    &module.program,
                                    function_type(signature),
                                ),
                                is_static: *is_static,
                                private: false,
                                mutable: false,
                            },
                        );
                    }
                    Statement::MemberFunction {
                        name: member_name,
                        parameters,
                        visibility,
                        ..
                    } if member_name == "CONSTRUCTOR" => {
                        constructor = Some(Constructor {
                            parameters: parameters
                                .iter()
                                .map(|parameter| {
                                    qualify_local_type(
                                        module.id,
                                        &module.program,
                                        type_from_reference(&parameter.type_ref),
                                    )
                                })
                                .collect(),
                            public: *visibility == Some(crate::ast::Visibility::Public),
                        });
                    }
                    _ => {}
                }
            }
            catalog.insert(
                (module.id, name.clone()),
                ImportedTypeInfo {
                    kind: *kind,
                    members,
                    constructor,
                    interfaces: interfaces.clone(),
                },
            );
        }
    }
    catalog
}

fn qualify_local_type(module: ModuleId, program: &Program, ty: Type) -> Type {
    let is_local = |name: &str| {
        program.items.iter().any(|item| {
            matches!(item, Item::Declaration { name: declaration, .. } if declaration == name)
        })
    };
    match ty {
        Type::Named(name) if is_local(&name) => Type::ImportedNamed { module, name },
        Type::TypeName(name) if is_local(&name) => Type::ImportedTypeName { module, name },
        Type::Alternative(types) => Type::Alternative(
            types
                .into_iter()
                .map(|ty| qualify_local_type(module, program, ty))
                .collect(),
        ),
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters
                .into_iter()
                .map(|ty| qualify_local_type(module, program, ty))
                .collect(),
            return_type: Box::new(qualify_local_type(module, program, *return_type)),
        },
        Type::Vector {
            element,
            dimensions,
        } => Type::Vector {
            element: Box::new(qualify_local_type(module, program, *element)),
            dimensions,
        },
        Type::Pointer { element, length } => Type::Pointer {
            element: Box::new(qualify_local_type(module, program, *element)),
            length,
        },
        ty => ty,
    }
}

fn module_imports(module: &crate::module_graph::LoadedModule) -> HashMap<String, ModuleId> {
    let mut imported_ids = module.imports.iter();
    module
        .program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Import { path, alias, .. } if path.first().is_some_and(|part| part != "HOST") => {
                Some((
                    alias.clone(),
                    *imported_ids.next().expect("module graph import order"),
                ))
            }
            _ => None,
        })
        .collect()
}

fn validate_implemented_interfaces(
    program: &Program,
    imported_types: &HashMap<(ModuleId, String), ImportedTypeInfo>,
    module_imports: &HashMap<String, ModuleId>,
    class_members: &HashMap<String, HashMap<String, Member>>,
) -> Result<(), Diagnostic> {
    let interfaces = program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration {
                kind: DeclarationKind::Interface,
                name,
                statements,
                ..
            } => Some((name.as_str(), statements)),
            _ => None,
        })
        .collect::<std::collections::HashMap<_, _>>();
    for item in &program.items {
        let Item::Declaration {
            kind: DeclarationKind::Class,
            name,
            interfaces: implemented,
            span,
            ..
        } = item
        else {
            continue;
        };
        for interface in implemented {
            let required = if let Some(required) = interfaces.get(interface.as_str()) {
                required
                    .iter()
                    .filter_map(|statement| match statement {
                        Statement::MemberFunction {
                            name,
                            signature: Some(signature),
                            ..
                        } => Some((name.clone(), function_type(signature))),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            } else if let Some((alias, name)) = interface.split_once('.') {
                let module = module_imports.get(alias).ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("interface module '{alias}' is not imported"),
                        *span,
                    )
                })?;
                let info = imported_types.get(&(*module, name.into())).ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("interface '{interface}' is not declared"),
                        *span,
                    )
                })?;
                if info.kind != DeclarationKind::Interface {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!("'{interface}' is not an INTERFACE"),
                        *span,
                    ));
                }
                info.members
                    .iter()
                    .filter(|(_, member)| matches!(member.ty, Type::Function { .. }))
                    .map(|(name, member)| (name.clone(), member.ty.clone()))
                    .collect::<Vec<_>>()
            } else {
                return Err(error(
                    "NAME_NOT_FOUND",
                    format!("interface '{interface}' is not declared"),
                    *span,
                ));
            };
            for (method, required_signature) in required {
                let implementation = class_members
                    .get(name)
                    .and_then(|members| members.get(&method));
                if implementation.is_none_or(|member| {
                    member.private || member.is_static || member.ty != required_signature
                }) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!(
                            "CLASS must implement PUBLIC instance FUNCTION {method} with the exact signature from interface {interface}"
                        ),
                        *span,
                    ));
                }
            }
        }
    }
    Ok(())
}

impl Analyzer {
    #[allow(clippy::too_many_lines)] // Global and standard namespaces share one declaration pass.
    fn declare_globals(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for (name, parameter) in [
            ("ASC", Type::String),
            ("CHAR", Type::Integer(IntegerType::Int32)),
        ] {
            self.declare_global(
                name,
                Type::Function {
                    parameters: vec![parameter],
                    return_type: Box::new(Type::Alternative(vec![
                        if name == "ASC" {
                            Type::Integer(IntegerType::Int32)
                        } else {
                            Type::String
                        },
                        Type::Named("Error".into()),
                    ])),
                },
                false,
                default_span(),
            )?;
        }
        self.declare_global(
            "Float",
            Type::TypeName("Float".into()),
            false,
            default_span(),
        )?;
        for namespace in ["Date", "Time", "TimeZone", "Timestamp"] {
            self.declare_global(
                namespace,
                Type::TypeName(namespace.into()),
                false,
                default_span(),
            )?;
        }
        for item in &program.items {
            match item {
                Item::Import { path, alias, span } => {
                    let ty = match path.as_slice() {
                        [host, capability] if host == "HOST" && capability == "Main" => {
                            return Err(error(
                                "NAME_NOT_FOUND",
                                "HOST.Main was withdrawn in 0.2; use HOST.Args",
                                *span,
                            ));
                        }
                        [host, capability] if host == "HOST" && capability == "Clock" => {
                            Type::HostClock
                        }
                        [host, capability] if host == "HOST" && capability == "Console" => {
                            Type::HostConsole
                        }
                        [host, capability] if host == "HOST" && capability == "Random" => {
                            Type::HostRandom
                        }
                        [host, capability] if host == "HOST" && capability == "FileSystem" => {
                            Type::HostFileSystem
                        }
                        [host, capability] if host == "HOST" => {
                            return Err(error(
                                "NAME_NOT_FOUND",
                                format!("HOST.{capability} is not a Basic Next 0.2 capability"),
                                *span,
                            ));
                        }
                        _ => Type::Module(*self.module_imports.get(alias).ok_or_else(|| {
                            error(
                                "MODULE_NOT_RESOLVED",
                                format!("module alias '{alias}' has no resolved ModuleId"),
                                *span,
                            )
                        })?),
                    };
                    self.declare_global(alias, ty, false, *span)?;
                }
                Item::Declaration {
                    name,
                    kind,
                    interfaces,
                    signature,
                    span,
                    statements,
                    ..
                } => {
                    let ty = signature
                        .as_ref()
                        .map_or_else(|| declaration_type(*kind, name), function_type);
                    self.declare_global(name, ty, false, *span)?;
                    self.declaration_kinds.insert(name.clone(), *kind);
                    if *kind == DeclarationKind::Class {
                        let mut declared_interfaces = std::collections::HashSet::new();
                        for interface in interfaces {
                            if !declared_interfaces.insert(interface) {
                                return Err(error(
                                    "DUPLICATE_INTERFACE",
                                    format!("CLASS '{name}' repeats interface '{interface}'"),
                                    *span,
                                ));
                            }
                        }
                        self.implementations
                            .insert(name.clone(), interfaces.clone());
                        if let Some(Statement::MemberFunction {
                            visibility,
                            is_static,
                            parameters,
                            span: constructor_span,
                            ..
                        }) = statements.iter().find(|statement| {
                            matches!(statement, Statement::MemberFunction { name, .. } if name == "CONSTRUCTOR")
                        }) {
                            if *is_static {
                                return Err(error(
                                    "INVALID_CONSTRUCTOR",
                                    "CONSTRUCTOR must be an instance function",
                                    *constructor_span,
                                ));
                            }
                            self.constructors.insert(
                                name.clone(),
                                Constructor {
                                    parameters: parameters
                                        .iter()
                                        .map(|parameter| self.resolve_reference(&parameter.type_ref))
                                        .collect(),
                                    public: *visibility == Some(crate::ast::Visibility::Public),
                                },
                            );
                        }
                    }
                }
            }
        }
        self.declare_bases(program)?;
        self.validate_super_constructors(program)?;
        self.members.insert(
            "Error".into(),
            HashMap::from([
                (
                    "Code".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Message".into(),
                    Member {
                        ty: Type::String,
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        self.members.insert(
            "HOST.Clock".into(),
            HashMap::from([
                (
                    "Timestamp".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int64)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Monotonic".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int64)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        self.members.insert(
            "HOST.Random".into(),
            HashMap::from([
                (
                    "Random".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Float(FloatType::Float64)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Seed".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Integer(IntegerType::Int32)],
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        self.members.insert(
            "HOST.FileSystem".into(),
            HashMap::from([
                (
                    "File".into(),
                    Member {
                        ty: Type::TypeName("FS.File".into()),
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Exists".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Boolean,
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Open".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String, Type::Integer(IntegerType::Int32)],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("FS.File".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "READ".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "WRITE".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "APPEND".into(),
                    Member {
                        ty: Type::Integer(IntegerType::Int32),
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "DeleteFile".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        self.members.insert(
            "FS.File".into(),
            HashMap::from([
                (
                    "Close".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "ReadLine".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::String,
                                Type::EndOfFile,
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "ReadAll".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Alternative(vec![
                                Type::String,
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "ReadBytes".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Pointer {
                                element: Box::new(Type::Integer(IntegerType::Byte)),
                                length: PointerLength::Dynamic,
                            }],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Integer(IntegerType::Int32),
                                Type::EndOfFile,
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Write".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "WriteBytes".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Pointer {
                                    element: Box::new(Type::Integer(IntegerType::Byte)),
                                    length: PointerLength::Dynamic,
                                },
                                Type::Integer(IntegerType::Int32),
                            ],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "WriteLine".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(Type::Alternative(vec![
                                Type::Named("VOID".into()),
                                Type::Named("Error".into()),
                            ])),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        self.members.insert("Float".into(), HashMap::new());
        self.members.insert(
            "HOST.Console".into(),
            HashMap::from([
                (
                    "Cls".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "Beep".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "PrintAt".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![
                                Type::Integer(IntegerType::Int32),
                                Type::Integer(IntegerType::Int32),
                                Type::String,
                            ],
                            return_type: Box::new(Type::Named("VOID".into())),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "NumCols".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int32)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
                (
                    "NumRows".into(),
                    Member {
                        ty: Type::Function {
                            parameters: Vec::new(),
                            return_type: Box::new(Type::Integer(IntegerType::Int32)),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        for (namespace, result) in [
            ("Date", Type::Named("DATE".into())),
            ("Time", Type::Named("TIME".into())),
            ("TimeZone", Type::Named("TIMEZONE".into())),
            ("Timestamp", Type::Integer(IntegerType::Int64)),
        ] {
            self.members.insert(
                namespace.into(),
                HashMap::from([(
                    "Parse".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::String],
                            return_type: Box::new(result),
                        },
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                )]),
            );
        }
        self.members
            .get_mut("Timestamp")
            .expect("Timestamp namespace")
            .insert(
                "Format".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![Type::Integer(IntegerType::Int64)],
                        return_type: Box::new(Type::String),
                    },
                    is_static: true,
                    private: false,
                    mutable: false,
                },
            );
        for item in &program.items {
            let Item::Declaration {
                kind,
                name,
                statements,
                ..
            } = item
            else {
                continue;
            };
            if *kind == DeclarationKind::Function {
                continue;
            }
            let mut declared = HashMap::new();
            for statement in statements {
                let member = match statement {
                    Statement::Binding {
                        name,
                        type_ref,
                        is_static,
                        visibility,
                        constant,
                        ..
                    } => Some((
                        name,
                        Member {
                            ty: self.resolve_reference(type_ref),
                            is_static: *is_static,
                            private: *kind == DeclarationKind::Class
                                && *visibility != Some(crate::ast::Visibility::Public),
                            mutable: !*constant,
                        },
                    )),
                    Statement::MemberFunction {
                        name,
                        signature: Some(signature),
                        is_static,
                        visibility,
                        ..
                    } => Some((
                        name,
                        Member {
                            ty: function_type(signature),
                            is_static: *is_static,
                            private: *kind == DeclarationKind::Class
                                && *visibility != Some(crate::ast::Visibility::Public),
                            mutable: false,
                        },
                    )),
                    _ => None,
                };
                if let Some((member_name, member)) = member {
                    declared.insert(member_name.clone(), member);
                }
            }
            self.members.insert(name.clone(), declared);
            self.declared_members.insert(
                name.clone(),
                statements
                    .iter()
                    .filter_map(|statement| match statement {
                        Statement::Binding { name, .. }
                        | Statement::MemberFunction { name, .. } => Some(name.clone()),
                        _ => None,
                    })
                    .collect(),
            );
        }
        self.inherit_members()?;
        self.compute_layouts();
        Ok(())
    }

    fn declare_bases(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for item in &program.items {
            let Item::Declaration {
                kind: DeclarationKind::Class,
                name,
                base_class: Some(base_class),
                ..
            } = item
            else {
                continue;
            };
            if base_class.alternatives.len() != 1 {
                return Err(error(
                    "TYPE_MISMATCH",
                    "EXTENDS requires one CLASS name",
                    base_class.span,
                ));
            }
            let base = match self.resolve_reference(base_class) {
                Type::Named(base)
                    if self.declaration_kinds.get(&base) == Some(&DeclarationKind::Class) =>
                {
                    base
                }
                Type::ImportedNamed { module, name }
                    if self
                        .imported_types
                        .get(&(module, name.clone()))
                        .is_some_and(|info| info.kind == DeclarationKind::Class) =>
                {
                    if self.standard_modules.contains(&module) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "EXTENDS cannot target a host or standard-library class",
                            base_class.span,
                        ));
                    }
                    let key = format!("#{}.{name}", module.0);
                    let info = self
                        .imported_types
                        .get(&(module, name))
                        .expect("checked above");
                    self.members.insert(key.clone(), info.members.clone());
                    if let Some(constructor) = &info.constructor {
                        self.constructors.insert(key.clone(), constructor.clone());
                    }
                    self.declared_members
                        .insert(key.clone(), info.members.keys().cloned().collect());
                    key
                }
                _ => {
                    let base = qualified_type_name(&base_class.alternatives[0]);
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!("EXTENDS requires a declared CLASS, found '{base}'"),
                        base_class.span,
                    ));
                }
            };
            self.base_classes.insert(name.clone(), base);
        }
        for class in self.base_classes.keys() {
            let mut seen = std::collections::HashSet::new();
            let mut current = class.as_str();
            while let Some(base) = self.base_classes.get(current) {
                if !seen.insert(current) {
                    return Err(error(
                        "INHERITANCE_CYCLE",
                        "class inheritance must be acyclic",
                        default_span(),
                    ));
                }
                current = base;
            }
        }
        let classes = self.base_classes.keys().cloned().collect::<Vec<_>>();
        for class in classes {
            let mut inherited = Vec::new();
            let mut current = self.base_classes.get(&class);
            while let Some(base) = current {
                if let Some(interfaces) = self.implementations.get(base) {
                    for interface in interfaces {
                        if !inherited.contains(interface) {
                            inherited.push(interface.clone());
                        }
                    }
                }
                current = self.base_classes.get(base);
            }
            let interfaces = self.implementations.entry(class).or_default();
            for interface in inherited {
                if !interfaces.contains(&interface) {
                    interfaces.push(interface);
                }
            }
        }
        Ok(())
    }

    fn inherit_members(&mut self) -> Result<(), Diagnostic> {
        let mut pending = self.base_classes.keys().cloned().collect::<Vec<_>>();
        while !pending.is_empty() {
            let pending_classes = pending
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>();
            let mut next = Vec::new();
            let mut progressed = false;
            for class in pending {
                let base = self.base_classes.get(&class).expect("base exists").clone();
                if self.base_classes.contains_key(&base) && pending_classes.contains(&base) {
                    next.push(class);
                    continue;
                }
                let base_members = self
                    .members
                    .get(&base)
                    .cloned()
                    .expect("base members exist");
                let own = self
                    .members
                    .get(&class)
                    .cloned()
                    .expect("class members exist");
                let mut inherited = base_members
                    .into_iter()
                    .filter(|(_, member)| !member.private)
                    .collect::<HashMap<_, _>>();
                for (name, member) in own {
                    if let Some(base_member) = inherited.get(&name) {
                        let methods = matches!(member.ty, Type::Function { .. })
                            && matches!(base_member.ty, Type::Function { .. });
                        let valid_override = methods
                            && !member.is_static
                            && !base_member.is_static
                            && !member.private
                            && member.ty == base_member.ty;
                        if !valid_override {
                            return Err(error(
                                "INVALID_OVERRIDE",
                                format!("member '{class}.{name}' conflicts with inherited member"),
                                default_span(),
                            ));
                        }
                    }
                    inherited.insert(name, member);
                }
                self.members.insert(class, inherited);
                progressed = true;
            }
            if !progressed {
                return Err(error(
                    "INHERITANCE_CYCLE",
                    "class inheritance must be acyclic",
                    default_span(),
                ));
            }
            pending = next;
        }
        Ok(())
    }

    fn validate_super_constructors(&self, program: &Program) -> Result<(), Diagnostic> {
        for item in &program.items {
            let Item::Declaration {
                kind: DeclarationKind::Class,
                name,
                statements,
                ..
            } = item
            else {
                continue;
            };
            let Some(base) = self.base_classes.get(name) else {
                continue;
            };
            let constructor = statements.iter().find_map(|statement| match statement {
                Statement::MemberFunction {
                    name: method,
                    body: Some(body),
                    span,
                    ..
                } if method == "CONSTRUCTOR" => Some((body, *span)),
                _ => None,
            });
            let explicit = constructor.and_then(|(body, _)| body.statements.first()).is_some_and(
                |statement| matches!(statement, Statement::Call { expression, .. } if matches!(expression.kind, ExpressionKind::Call { ref callee, .. } if matches!(callee.kind, ExpressionKind::Super))),
            );
            if let Some((body, _)) = constructor {
                for (index, statement) in body.statements.iter().enumerate() {
                    if statement_uses_super(statement) && (!explicit || index != 0) {
                        return Err(error(
                            "INVALID_SUPER",
                            "SUPER(...) must be the first constructor statement",
                            statement_span(statement),
                        ));
                    }
                }
            }
            if !explicit
                && self
                    .constructors
                    .get(base)
                    .is_some_and(|constructor| !constructor.parameters.is_empty())
            {
                return Err(error(
                    "INVALID_SUPER",
                    format!("constructor for '{name}' must call SUPER(...)"),
                    constructor.map_or(default_span(), |(_, span)| span),
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Class members, constructors, and SELF share one pass.
    fn analyze_declarations(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for item in &program.items {
            let Item::Declaration {
                name,
                kind,
                signature,
                statements,
                ..
            } = item
            else {
                continue;
            };
            if *kind != DeclarationKind::Function {
                validate_member_names(statements)?;
                for statement in statements {
                    let Statement::MemberFunction {
                        name: member_name,
                        is_static,
                        parameters,
                        signature: member_signature,
                        span: member_span,
                        body,
                        ..
                    } = statement
                    else {
                        continue;
                    };
                    if matches!(member_name.as_str(), "CONSTRUCTOR" | "DESTRUCTOR") && *is_static {
                        return Err(error(
                            "INVALID_CONSTRUCTOR",
                            "CONSTRUCTOR and DESTRUCTOR must be instance functions",
                            *member_span,
                        ));
                    }
                    if member_name == "DESTRUCTOR" && !parameters.is_empty() {
                        return Err(error(
                            "INVALID_DESTRUCTOR",
                            "DESTRUCTOR must not declare parameters",
                            *member_span,
                        ));
                    }
                    if member_name == "DESTRUCTOR"
                        && let Some(body) = body
                    {
                        for statement in &body.statements {
                            if statement_uses_super(statement) {
                                return Err(error(
                                    "INVALID_SUPER",
                                    "there is no SUPER in a destructor; the chain is implicit",
                                    statement_span(statement),
                                ));
                            }
                        }
                    }
                    for parameter in parameters {
                        self.validate_type_reference(&parameter.type_ref)?;
                    }
                    if let Some(signature) = member_signature {
                        self.validate_type_reference(&signature.return_type)?;
                        for parameter in &signature.parameters {
                            self.validate_type_reference(&parameter.type_ref)?;
                        }
                    }
                }
            }
            let mut locals = HashMap::new();
            if let Some(signature) = signature {
                self.validate_type_reference(&signature.return_type)?;
                for parameter in &signature.parameters {
                    self.validate_type_reference(&parameter.type_ref)?;
                    self.declare_local(
                        &mut locals,
                        &parameter.name,
                        self.resolve_reference(&parameter.type_ref),
                        false,
                        parameter.span,
                    )?;
                }
            }
            let return_type = signature
                .as_ref()
                .map(|signature| self.resolve_reference(&signature.return_type));
            let previous_class = std::mem::replace(
                &mut self.current_class,
                (*kind == DeclarationKind::Class).then(|| name.clone()),
            );
            if *kind == DeclarationKind::Class {
                self.declare_local(
                    &mut locals,
                    "SELF",
                    Type::Named(name.clone()),
                    true,
                    default_span(),
                )?;
            }
            self.block(
                statements,
                &mut locals,
                &mut Vec::new(),
                *kind,
                name,
                return_type.as_ref(),
            )?;
            self.current_class = previous_class;
            if *kind == DeclarationKind::Function
                && let Some(signature) = signature
            {
                validate_returns(statements, &signature.return_type, name == "Start")?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)] // Statement alternatives mirror the grammar.
    fn block(
        &mut self,
        statements: &[Statement],
        locals: &mut HashMap<String, Symbol>,
        loops: &mut Vec<&'static str>,
        declaration_kind: DeclarationKind,
        declaration_name: &str,
        return_type: Option<&Type>,
    ) -> Result<(), Diagnostic> {
        let mut terminated = false;
        for statement in statements {
            if statement_has_invalid_super(statement) {
                return Err(error(
                    "INVALID_SUPER",
                    "SUPER is only valid as SUPER(...) or SUPER.Name(...)",
                    statement_span(statement),
                ));
            }
            if terminated {
                return Err(error(
                    "UNREACHABLE_CODE",
                    "statement is unreachable after control flow leaves this path",
                    statement_span(statement),
                ));
            }
            match statement {
                Statement::Binding {
                    constant,
                    name,
                    type_ref,
                    initialized,
                    initializer,
                    span,
                    is_static,
                    ..
                } => {
                    self.validate_type_reference(type_ref)?;
                    let ty = self.resolve_reference(type_ref);
                    if !initialized
                        && (requires_initializer(type_ref) || self.type_requires_initializer(&ty))
                    {
                        return Err(error(
                            "TYPE_MISMATCH",
                            format!(
                                "{} bindings require an initializer",
                                type_ref.alternatives[0].name
                            ),
                            *span,
                        ));
                    }
                    if declaration_kind == DeclarationKind::Class && *is_static && !initialized {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "STATIC field requires an initializer",
                            *span,
                        ));
                    }
                    if let Some(initializer) = initializer {
                        let actual = self.expression_as(initializer, &ty, locals)?;
                        if !self.compatible(&ty, &actual) {
                            return Err(error(
                                if pointer_literal_length_mismatch(&ty, &actual) {
                                    "POINTER_LENGTH_MISMATCH"
                                } else {
                                    "TYPE_MISMATCH"
                                },
                                format!("cannot assign {} to {}", display(&actual), display(&ty)),
                                initializer.span,
                            ));
                        }
                    }
                    self.declare_local(locals, name, ty, *constant, *span)?;
                }
                Statement::Assignment {
                    target,
                    operator,
                    value,
                    span,
                } => {
                    let target_type = self.assignment_target(target, locals)?;
                    let value_type = self.expression_as(value, &target_type, locals)?;
                    let result_type = if operator == "Assign" {
                        value_type
                    } else {
                        binary_type(
                            compound_operator(operator).ok_or_else(|| {
                                error("TYPE_MISMATCH", "unknown assignment operator", *span)
                            })?,
                            &target_type,
                            &value_type,
                            value,
                        )?
                    };
                    if !self.compatible(&target_type, &result_type) {
                        return Err(error(
                            if pointer_literal_length_mismatch(&target_type, &result_type) {
                                "POINTER_LENGTH_MISMATCH"
                            } else {
                                "TYPE_MISMATCH"
                            },
                            format!(
                                "cannot assign {} to {}",
                                display(&result_type),
                                display(&target_type)
                            ),
                            *span,
                        ));
                    }
                    Self::replace_narrowing_fact(target, &result_type, locals);
                }
                Statement::If {
                    branches,
                    otherwise,
                    ..
                } => {
                    let mut remaining_locals = locals.clone();
                    let mut taken_exits = true;
                    for branch in branches {
                        self.require_boolean(&branch.condition, &remaining_locals)?;
                        let mut branch_locals =
                            self.narrowed_locals(&branch.condition, &remaining_locals, true)?;
                        self.block(
                            &branch.body.statements,
                            &mut branch_locals,
                            loops,
                            declaration_kind,
                            declaration_name,
                            return_type,
                        )?;
                        taken_exits &= guarantees_return(&branch.body.statements);
                        remaining_locals =
                            self.narrowed_locals(&branch.condition, &remaining_locals, false)?;
                    }
                    if let Some(Block { statements, .. }) = otherwise {
                        self.block(
                            statements,
                            &mut remaining_locals,
                            loops,
                            declaration_kind,
                            declaration_name,
                            return_type,
                        )?;
                    } else if taken_exits {
                        *locals = remaining_locals;
                    }
                }
                Statement::While {
                    condition, body, ..
                } => {
                    self.require_boolean(condition, locals)?;
                    loops.push("WHILE");
                    self.block(
                        &body.statements,
                        locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                }
                Statement::Repeat {
                    condition, body, ..
                } => {
                    loops.push("REPEAT");
                    self.block(
                        &body.statements,
                        locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                    self.require_boolean(condition, locals)?;
                }
                Statement::For { header, body, .. } => {
                    self.for_header(header, locals)?;
                    loops.push("FOR");
                    self.block(
                        &body.statements,
                        locals,
                        loops,
                        declaration_kind,
                        declaration_name,
                        return_type,
                    )?;
                    loops.pop();
                }
                Statement::Control { kind, target, span } => {
                    if !matches!(kind.as_str(), "EXIT" | "CONTINUE")
                        || !loops.iter().rev().any(|loop_kind| *loop_kind == target)
                    {
                        return Err(error(
                            "INVALID_LOOP_CONTROL",
                            format!("{kind} {target} requires an enclosing {target} loop"),
                            *span,
                        ));
                    }
                }
                Statement::Return { value, span } => {
                    if declaration_kind != DeclarationKind::Function && value.is_some() {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "constructors and destructors cannot return a value",
                            *span,
                        ));
                    }
                    if let Some(value) = value {
                        let value_type = if let Some(return_type) = return_type {
                            self.expression_as(value, return_type, locals)?
                        } else {
                            self.expression(value, locals)?
                        };
                        if let Some(return_type) = return_type
                            && !self.compatible(return_type, &value_type)
                        {
                            return Err(error(
                                "TYPE_MISMATCH",
                                format!(
                                    "cannot return {} from FUNCTION AS {}",
                                    display(&value_type),
                                    display(return_type)
                                ),
                                value.span,
                            ));
                        }
                    }
                }
                Statement::Print { values, .. } => {
                    for value in values {
                        self.expression(value, locals)?;
                    }
                }
                Statement::ClearScreen { span, .. } | Statement::Beep { span, .. } => {
                    return Err(error(
                        "NAME_NOT_FOUND",
                        "CLS and BEEP statements were withdrawn in 0.2; use HOST.Console methods",
                        *span,
                    ));
                }
                Statement::Delete { value, .. } => {
                    let ty = self.expression(value, locals)?;
                    if !self.deletable(&ty) {
                        return Err(error(
                            "INVALID_DELETE_TARGET",
                            format!(
                                "DELETE requires a pointer or CLASS reference, found {}",
                                display(&ty)
                            ),
                            value.span,
                        ));
                    }
                }
                Statement::Stop { code, .. } => {
                    if !is_integer(&self.expression(code, locals)?) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "STOP requires an INTEGER exit code",
                            code.span,
                        ));
                    }
                    if let Some(value) = integer_literal(code)
                        && !(0..=255).contains(&value)
                    {
                        return Err(error(
                            "INVALID_EXIT_CODE",
                            "STOP exit code must be in 0..255",
                            code.span,
                        ));
                    }
                }
                Statement::Call { expression, .. } => {
                    self.expression(expression, locals)?;
                }
                Statement::MemberFunction {
                    name,
                    is_static,
                    parameters,
                    signature,
                    body: Some(body),
                    ..
                } => {
                    if *is_static && block_uses_self(&body.statements) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "STATIC FUNCTION cannot access SELF",
                            body.span,
                        ));
                    }
                    let member_kind = if matches!(name.as_str(), "CONSTRUCTOR" | "DESTRUCTOR") {
                        DeclarationKind::Class
                    } else {
                        DeclarationKind::Function
                    };
                    let mut member_locals = HashMap::new();
                    if !*is_static {
                        self.declare_local(
                            &mut member_locals,
                            "SELF",
                            Type::Named(declaration_name.into()),
                            true,
                            body.span,
                        )?;
                    }
                    let return_type = signature
                        .as_ref()
                        .map(|signature| self.resolve_reference(&signature.return_type));
                    if let Some(signature) = signature {
                        for parameter in &signature.parameters {
                            self.validate_type_reference(&parameter.type_ref)?;
                            self.declare_local(
                                &mut member_locals,
                                &parameter.name,
                                self.resolve_reference(&parameter.type_ref),
                                false,
                                parameter.span,
                            )?;
                        }
                    } else {
                        for parameter in parameters {
                            self.validate_type_reference(&parameter.type_ref)?;
                            self.declare_local(
                                &mut member_locals,
                                &parameter.name,
                                self.resolve_reference(&parameter.type_ref),
                                false,
                                parameter.span,
                            )?;
                        }
                    }
                    self.block(
                        &body.statements,
                        &mut member_locals,
                        loops,
                        member_kind,
                        declaration_name,
                        return_type.as_ref(),
                    )?;
                    if let Some(signature) = signature {
                        validate_returns(&body.statements, &signature.return_type, false)?;
                    }
                }
                Statement::MemberFunction { body: None, .. } => {}
            }
            terminated = matches!(
                statement,
                Statement::Return { .. } | Statement::Stop { .. } | Statement::Control { .. }
            );
        }
        Ok(())
    }

    fn for_header(
        &mut self,
        header: &ForHeader,
        locals: &mut HashMap<String, Symbol>,
    ) -> Result<(), Diagnostic> {
        match header {
            ForHeader::Counted {
                variable,
                type_ref,
                start,
                end,
                step,
            } => {
                for expression in [start, end] {
                    if !is_integer(&self.expression(expression, locals)?) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "counted FOR bounds must be integral",
                            expression.span,
                        ));
                    }
                }
                if let Some(step) = step
                    && !is_integer(&self.expression(step, locals)?)
                {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "FOR STEP must be integral",
                        step.span,
                    ));
                }
                if let Some(step) = step
                    && integer_literal(step) == Some(0)
                {
                    return Err(error("TYPE_MISMATCH", "FOR STEP cannot be zero", step.span));
                }
                self.declare_local(
                    locals,
                    variable,
                    self.resolve_reference(type_ref),
                    false,
                    type_ref.span,
                )
            }
            ForHeader::Each {
                variable,
                type_ref,
                iterable,
            } => {
                let iterable_type = self.expression(iterable, locals)?;
                let Type::Vector {
                    element,
                    dimensions,
                } = iterable_type
                else {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "FOR EACH requires a fixed-length vector",
                        iterable.span,
                    ));
                };
                if dimensions.contains(&u64::MAX) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "FOR EACH requires a fixed-length vector",
                        iterable.span,
                    ));
                }
                let declared = self.resolve_reference(type_ref);
                if declared != *element {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!(
                            "FOR EACH binding must have element type {}",
                            display(&element)
                        ),
                        type_ref.span,
                    ));
                }
                self.declare_local(locals, variable, declared, true, type_ref.span)
            }
        }
    }

    fn assignment_target(
        &mut self,
        expression: &Expression,
        locals: &mut HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        match &expression.kind {
            ExpressionKind::Name { name } => {
                let symbol = self.lookup(name, locals).cloned().ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("name '{name}' is not declared"),
                        expression.span,
                    )
                })?;
                if symbol.constant {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!("cannot assign to CONST '{name}'"),
                        expression.span,
                    ));
                }
                self.record_expression(expression.span, &symbol.declared_ty, Some(symbol.id));
                if let Some(local) = locals.get_mut(name) {
                    local.ty = local.declared_ty.clone();
                }
                Ok(symbol.declared_ty)
            }
            ExpressionKind::Member { object, name } => {
                let ty = self.expression(expression, locals)?;
                let object_type = self.expression(object, locals)?;
                if !self.member_mutable(&object_type, name) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!("member '{name}' is not an assignable field"),
                        expression.span,
                    ));
                }
                Ok(ty)
            }
            ExpressionKind::Index { object, .. } if matches!(object.kind, ExpressionKind::HostCapability { ref name } if name == "Args") => {
                Err(error(
                    "TYPE_MISMATCH",
                    "HOST.Args entries are immutable",
                    expression.span,
                ))
            }
            ExpressionKind::Index { object, .. } => {
                let object_type = self.expression(object, locals)?;
                if object_type == Type::String {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "STRING indices are read-only",
                        expression.span,
                    ));
                }
                self.expression(expression, locals)
            }
            _ => Err(error(
                "TYPE_MISMATCH",
                "assignment target is not mutable",
                expression.span,
            )),
        }
    }
    fn require_boolean(
        &mut self,
        expression: &Expression,
        locals: &HashMap<String, Symbol>,
    ) -> Result<(), Diagnostic> {
        let ty = self.expression(expression, locals)?;
        if compatible(&Type::Boolean, &ty) {
            Ok(())
        } else {
            Err(error(
                "TYPE_MISMATCH",
                format!("condition must be BOOLEAN, found {}", display(&ty)),
                expression.span,
            ))
        }
    }
    fn expression_as(
        &mut self,
        expression: &Expression,
        expected: &Type,
        locals: &HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        if matches!(&expression.kind, ExpressionKind::Vector { values } if values.is_empty())
            && matches!(expected, Type::Vector { .. })
        {
            self.record_expression(expression.span, expected, None);
            return Ok(expected.clone());
        }
        self.expression(expression, locals)
    }
    #[allow(clippy::too_many_lines)] // Expression alternatives mirror the syntax AST.
    fn expression(
        &mut self,
        expression: &Expression,
        locals: &HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        let result = match &expression.kind {
            ExpressionKind::Super => Err(error(
                "INVALID_SUPER",
                "SUPER is only valid as SUPER(...) or SUPER.Name(...)",
                expression.span,
            )),
            ExpressionKind::Literal(Literal::TypeName(name)) => Err(error(
                "TYPE_NAME_AS_VALUE",
                format!("type name '{name}' is not a first-class value"),
                expression.span,
            )),
            ExpressionKind::TypeTest { .. } => Err(error(
                "TYPE_NAME_AS_VALUE",
                "a type test may appear only to the right of IS",
                expression.span,
            )),
            ExpressionKind::Literal(literal) => Ok(literal_type(literal)),
            ExpressionKind::Input => Ok(Type::Alternative(vec![Type::String, Type::EndOfFile])),
            ExpressionKind::HostCapability { name } if name == "Args" => {
                if self.executable_module {
                    Err(error(
                        "INVALID_HOST_ARGS_USE",
                        "HOST.Args is valid only in LEN(HOST.Args) or HOST.Args[index]",
                        expression.span,
                    ))
                } else {
                    Err(error(
                        "HOST_ARGS_SCOPE",
                        "HOST.Args is valid only in the executable module",
                        expression.span,
                    ))
                }
            }
            ExpressionKind::HostCapability { name } => host_capability_type(name, expression.span),
            ExpressionKind::Length { operand } => {
                if matches!(operand.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    if self.executable_module {
                        Ok(Type::Integer(IntegerType::Int32))
                    } else {
                        Err(error(
                            "HOST_ARGS_SCOPE",
                            "HOST.Args is valid only in the executable module",
                            operand.span,
                        ))
                    }
                } else {
                    let ty = self.expression(operand, locals)?;
                    length_type(&ty, operand.span)
                }
            }
            ExpressionKind::SizeOf { operand } => {
                let ty = self.expression(operand, locals)?;
                self.sizeof_type(&ty, operand.span)
            }
            ExpressionKind::Name { name } => self
                .lookup(name, locals)
                .map(|symbol| symbol.ty.clone())
                .ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("name '{name}' is not declared"),
                        expression.span,
                    )
                }),
            ExpressionKind::Vector { values } => {
                if vector_shape(expression).is_none() {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "vector literal has inconsistent nested dimensions",
                        expression.span,
                    ));
                }
                let mut element_type = Type::Unknown;
                for value in values {
                    let actual = self.expression(value, locals)?;
                    if element_type == Type::Unknown {
                        element_type = actual;
                    } else if !comparable(&element_type, &actual) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            "vector literal elements must have one compatible type",
                            value.span,
                        ));
                    }
                }
                if element_type == Type::Unknown {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "empty vector literal has no inferable element type",
                        expression.span,
                    ));
                }
                let element_type = match element_type {
                    Type::Vector { element, .. } => *element,
                    element => element,
                };
                Ok(Type::Vector {
                    element: Box::new(default_literal_type(element_type)),
                    dimensions: vector_shape(expression)
                        .expect("validated vector shape")
                        .into_iter()
                        .map(|dimension| u64::try_from(dimension).expect("usize fits u64"))
                        .collect(),
                })
            }
            ExpressionKind::New {
                type_name,
                arguments,
            } => {
                let allocation_type = self.resolve_type(type_from_name(type_name));
                let mut argument_types = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let argument_type = self.expression(argument, locals)?;
                    if matches!(allocation_type, Type::Integer(_) | Type::Float(_))
                        && !is_integer(&argument_type)
                    {
                        return Err(error(
                            "ALLOCATION_SIZE_INVALID",
                            "numeric NEW length must be integral",
                            argument.span,
                        ));
                    }
                    if matches!(allocation_type, Type::Integer(_) | Type::Float(_))
                        && constant_integer(argument).is_some_and(|length| length < 0)
                    {
                        return Err(error(
                            "ALLOCATION_SIZE_INVALID",
                            "numeric NEW length cannot be negative",
                            argument.span,
                        ));
                    }
                    argument_types.push(argument_type);
                }
                Ok(match allocation_type {
                    Type::Integer(kind) => Type::Pointer {
                        element: Box::new(Type::Integer(kind)),
                        length: allocation_length(arguments),
                    },
                    Type::Float(kind) => Type::Pointer {
                        element: Box::new(Type::Float(kind)),
                        length: allocation_length(arguments),
                    },
                    Type::ImportedNamed { module, name } => {
                        let info = self
                            .imported_types
                            .get(&(module, name.clone()))
                            .ok_or_else(|| {
                                error(
                                    "UNKNOWN_TYPE",
                                    format!("imported type '{type_name}' is unavailable"),
                                    expression.span,
                                )
                            })?;
                        if info.kind != DeclarationKind::Class {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!("NEW requires a CLASS, found '{type_name}'"),
                                expression.span,
                            ));
                        }
                        let constructor = info.constructor.as_ref().ok_or_else(|| {
                            error(
                                "PRIVATE_ACCESS",
                                format!(
                                    "CLASS '{type_name}' has only an implicit PRIVATE constructor"
                                ),
                                expression.span,
                            )
                        })?;
                        if !constructor.public {
                            return Err(error(
                                "PRIVATE_ACCESS",
                                format!("constructor for CLASS '{type_name}' is PRIVATE"),
                                expression.span,
                            ));
                        }
                        if constructor.parameters.len() != argument_types.len()
                            || constructor
                                .parameters
                                .iter()
                                .zip(&argument_types)
                                .any(|(expected, actual)| !self.compatible(expected, actual))
                        {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!("arguments do not match constructor for '{type_name}'"),
                                expression.span,
                            ));
                        }
                        Type::ImportedNamed { module, name }
                    }
                    Type::Named(name) if matches!(name.as_str(), "FS.File" | "DataFrame") => {
                        Type::Named(name)
                    }
                    _ => {
                        if self.declaration_kinds.get(type_name) != Some(&DeclarationKind::Class) {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!("NEW requires a declared CLASS, found '{type_name}'"),
                                expression.span,
                            ));
                        }
                        let constructor = self.constructors.get(type_name).cloned().or_else(|| {
                            (self.current_class.as_deref() == Some(type_name.as_str())
                                && arguments.is_empty())
                            .then_some(Constructor {
                                parameters: Vec::new(),
                                public: false,
                            })
                        });
                        let Some(constructor) = constructor else {
                            return Err(error(
                                "PRIVATE_ACCESS",
                                format!(
                                    "CLASS '{type_name}' has only an implicit PRIVATE constructor"
                                ),
                                expression.span,
                            ));
                        };
                        if !constructor.public
                            && self.current_class.as_deref() != Some(type_name.as_str())
                        {
                            return Err(error(
                                "PRIVATE_ACCESS",
                                format!("constructor for CLASS '{type_name}' is PRIVATE"),
                                expression.span,
                            ));
                        }
                        if constructor.parameters.len() != argument_types.len() {
                            return Err(error(
                                "INVALID_CONSTRUCTOR",
                                format!(
                                    "constructor for CLASS '{type_name}' expects {} argument(s), found {}",
                                    constructor.parameters.len(),
                                    argument_types.len()
                                ),
                                expression.span,
                            ));
                        }
                        for ((expected, actual), argument) in constructor
                            .parameters
                            .iter()
                            .zip(&argument_types)
                            .zip(arguments)
                        {
                            if !self.compatible(expected, actual) {
                                return Err(error(
                                    "INVALID_CONSTRUCTOR",
                                    format!(
                                        "constructor argument has type {}, expected {}",
                                        display(actual),
                                        display(expected)
                                    ),
                                    argument.span,
                                ));
                            }
                        }
                        Type::Named(type_name.clone())
                    }
                })
            }
            ExpressionKind::Unary { operator, operand } => {
                let operand_type = self.expression(operand, locals)?;
                match operator.as_str() {
                    "Minus" if is_numeric(&operand_type) => {
                        if let Some(value) = constant_integer(expression) {
                            Ok(Type::IntegerLiteral(value.to_string()))
                        } else {
                            Ok(default_literal_type(operand_type))
                        }
                    }
                    "NOT" if operand_type == Type::Boolean => Ok(Type::Boolean),
                    "NOT" if is_integer(&operand_type) => Ok(default_literal_type(operand_type)),
                    _ => Err(error(
                        "TYPE_MISMATCH",
                        format!(
                            "operator {operator} cannot be applied to {}",
                            display(&operand_type)
                        ),
                        expression.span,
                    )),
                }
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => {
                let left_type = self.expression(left, locals)?;
                if operator == "IS" {
                    self.validate_is_test(&left_type, right, expression.span)?;
                    Ok(Type::Boolean)
                } else {
                    let right_type = self.expression(right, locals)?;
                    binary_type(operator, &left_type, &right_type, expression)
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                if matches!(callee.kind, ExpressionKind::Super) {
                    self.super_constructor_call(arguments, locals, expression.span)
                } else {
                    self.call(callee, arguments, locals, expression.span)
                }
            }
            ExpressionKind::Member { object, name }
                if matches!(object.kind, ExpressionKind::Super) =>
            {
                let base = self.direct_base(expression.span)?;
                self.member_type(&Type::Named(base), name, expression.span)
            }
            ExpressionKind::Member { object, name } => {
                let object_type = self.expression(object, locals)?;
                self.member_type(&object_type, name, expression.span)
            }
            ExpressionKind::Index { object, index } => {
                let object_type = if matches!(object.kind, ExpressionKind::HostCapability { ref name } if name == "Args")
                {
                    if self.executable_module {
                        Type::HostArgs
                    } else {
                        return Err(error(
                            "HOST_ARGS_SCOPE",
                            "HOST.Args is valid only in the executable module",
                            object.span,
                        ));
                    }
                } else {
                    self.expression(object, locals)?
                };
                let index_type = self.expression(index, locals)?;
                if !is_integer(&index_type) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        "index must have an integral type",
                        index.span,
                    ));
                }
                match object_type {
                    Type::Vector {
                        element,
                        mut dimensions,
                    } => {
                        if dimensions.len() <= 1 {
                            Ok(*element)
                        } else {
                            dimensions.remove(0);
                            Ok(Type::Vector {
                                element,
                                dimensions,
                            })
                        }
                    }
                    Type::Pointer { element, .. } if !is_void(&element) => Ok(*element),
                    Type::Pointer { .. } => Err(error(
                        "TYPE_MISMATCH",
                        "POINTER TO VOID must be converted to a typed pointer before indexing",
                        object.span,
                    )),
                    Type::String | Type::HostArgs => Ok(Type::String),
                    Type::Alternative(types) => types
                        .iter()
                        .find_map(|ty| match ty {
                            Type::Pointer { element, .. } if !is_void(element) => {
                                Some(*element.clone())
                            }
                            _ => None,
                        })
                        .ok_or_else(|| {
                            error(
                                "TYPE_MISMATCH",
                                format!("cannot index {}", display(&Type::Alternative(types))),
                                object.span,
                            )
                        }),
                    Type::Unknown => Err(error(
                        "UNRESOLVED_TYPE",
                        "indexed expression has no resolved type",
                        object.span,
                    )),
                    other => Err(error(
                        "TYPE_MISMATCH",
                        format!("cannot index {}", display(&other)),
                        object.span,
                    )),
                }
            }
            ExpressionKind::Cast { type_ref, value } => {
                let source = self.expression(value, locals)?;
                let target = self.resolve_reference(type_ref);
                if !conversion_allowed(&source, &target) {
                    return Err(error(
                        "TYPE_MISMATCH",
                        format!(
                            "cannot convert {} AS {}",
                            display(&source),
                            display(&target)
                        ),
                        expression.span,
                    ));
                }
                Ok(target)
            }
        };
        let ty = result?;
        let symbol_id = match &expression.kind {
            ExpressionKind::Name { name } => self.lookup(name, locals).map(|symbol| symbol.id),
            _ => None,
        };
        self.record_expression(expression.span, &ty, symbol_id);
        if let ExpressionKind::Member { object, name } = &expression.kind {
            if matches!(object.kind, ExpressionKind::Super)
                && let Ok(base) = self.direct_base(expression.span)
                && let Some(resolved) = self
                    .expressions
                    .iter_mut()
                    .find(|resolved| resolved.span == expression.span)
            {
                resolved.member_target = Some(MemberTarget {
                    module: None,
                    owner: Some(base),
                    name: name.clone(),
                });
            }
            let object_type = self
                .expressions
                .iter()
                .find(|resolved| resolved.span == object.span)
                .map(|resolved| resolved.ty.clone());
            if let Some(mut target) = object_type.and_then(|ty| member_target(&ty, name)) {
                let resolved_owner = target
                    .owner
                    .as_deref()
                    .and_then(|owner| self.member_owner(owner, name));
                if let Some(owner) = resolved_owner {
                    target.owner = Some(owner);
                }
                if let Some(resolved) = self
                    .expressions
                    .iter_mut()
                    .find(|resolved| resolved.span == expression.span)
                {
                    resolved.member_target = Some(target);
                }
            }
        }
        if matches!(expression.kind, ExpressionKind::New { .. })
            && let Some(target) = constructor_target(&ty)
            && let Some(resolved) = self
                .expressions
                .iter_mut()
                .find(|resolved| resolved.span == expression.span)
        {
            resolved.member_target = Some(target);
        }
        if let ExpressionKind::Call { callee, .. } = &expression.kind
            && matches!(callee.kind, ExpressionKind::Super)
            && let Ok(base) = self.direct_base(expression.span)
            && let Some(resolved) = self
                .expressions
                .iter_mut()
                .find(|resolved| resolved.span == expression.span)
        {
            resolved.member_target = Some(MemberTarget {
                module: None,
                owner: Some(base),
                name: "CONSTRUCTOR".into(),
            });
        }
        Ok(ty)
    }

    fn record_expression(&mut self, span: Span, ty: &Type, symbol_id: Option<SymbolId>) {
        let resolved = ResolvedExpression {
            span,
            type_name: display(ty),
            ty: ty.clone(),
            symbol_id,
            member_target: None,
        };
        if let Some(existing) = self
            .expressions
            .iter_mut()
            .find(|existing| existing.span == span)
        {
            *existing = resolved;
        } else {
            self.expressions.push(resolved);
        }
    }

    #[allow(clippy::too_many_lines)] // Member lookup enumerates every accepted owner type.
    fn member_type(&self, object: &Type, name: &str, span: Span) -> Result<Type, Diagnostic> {
        if object == &Type::Unknown {
            return Err(error(
                "UNRESOLVED_TYPE",
                "member receiver has no resolved type",
                span,
            ));
        }
        if let Type::Module(module) = object {
            return self
                .module_exports
                .get(module)
                .and_then(|exports| exports.get(name))
                .cloned()
                .ok_or_else(|| {
                    error(
                        "NAME_NOT_FOUND",
                        format!("imported module does not export '{name}'"),
                        span,
                    )
                });
        }
        if let Type::ImportedNamed {
            module,
            name: imported_name,
        }
        | Type::ImportedTypeName {
            module,
            name: imported_name,
        } = object
        {
            let static_access = matches!(object, Type::ImportedTypeName { .. });
            let member = self
                .imported_types
                .get(&(*module, imported_name.clone()))
                .and_then(|info| info.members.get(name));
            let member = member.ok_or_else(|| {
                error(
                    "NAME_NOT_FOUND",
                    format!(
                        "imported type '{}' has no exported member '{name}'",
                        display(object)
                    ),
                    span,
                )
            })?;
            if member.is_static != static_access {
                return Err(error(
                    "TYPE_MISMATCH",
                    "imported member has the wrong instance/static access form",
                    span,
                ));
            }
            return Ok(member.ty.clone());
        }
        let (owner, static_access) = match object {
            Type::System => ("SYSTEM", false),
            Type::HostClock => ("HOST.Clock", false),
            Type::HostRandom => ("HOST.Random", false),
            Type::HostConsole => ("HOST.Console", false),
            Type::HostFileSystem => ("HOST.FileSystem", false),
            Type::Named(owner) => (owner.as_str(), false),
            Type::TypeName(owner) => (owner.as_str(), true),
            Type::Alternative(alternatives) => {
                let mut found = None;
                for alternative in alternatives {
                    if matches!(
                        alternative,
                        Type::Null | Type::NotAvailable | Type::EndOfFile
                    ) {
                        continue;
                    }
                    let ty = self.member_type(alternative, name, span)?;
                    if found.as_ref().is_some_and(|found| found != &ty) {
                        return Err(error(
                            "TYPE_MISMATCH",
                            format!("member '{name}' has incompatible alternative types"),
                            span,
                        ));
                    }
                    found = Some(ty);
                }
                return found.ok_or_else(|| {
                    error(
                        "TYPE_MISMATCH",
                        format!("member '{name}' is unavailable on {}", display(object)),
                        span,
                    )
                });
            }
            _ => {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("{} has no member '{name}'", display(object)),
                    span,
                ));
            }
        };
        let member = self
            .members
            .get(owner)
            .and_then(|members| members.get(name))
            .ok_or_else(|| {
                error(
                    "NAME_NOT_FOUND",
                    format!("type '{owner}' has no member '{name}'"),
                    span,
                )
            })?;
        if member.is_static != static_access {
            return Err(error(
                "TYPE_MISMATCH",
                if static_access {
                    format!("instance member '{owner}.{name}' requires an object")
                } else {
                    format!("STATIC member '{owner}.{name}' requires the type name")
                },
                span,
            ));
        }
        if member.private && self.current_class.as_deref() != Some(owner) {
            return Err(error(
                "PRIVATE_ACCESS",
                format!("member '{owner}.{name}' is PRIVATE"),
                span,
            ));
        }
        Ok(member.ty.clone())
    }
    fn validate_is_test(
        &self,
        subject: &Type,
        test: &Expression,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let test_type = is_test_type(test).ok_or_else(|| {
            error(
                "INVALID_ALTERNATIVE_USE",
                "IS requires a type, NULL, NA, EOF, NAN, or INF test",
                test.span,
            )
        })?;
        let test_type = self.resolve_type(test_type);
        if is_float(&test_type)
            && matches!(&test.kind, ExpressionKind::Literal(Literal::Special(_)))
            && is_float(subject)
        {
            return Ok(());
        }
        if compatible(subject, &test_type) {
            Ok(())
        } else {
            Err(error(
                "INVALID_ALTERNATIVE_USE",
                format!(
                    "{} is not an alternative of {}",
                    display(&test_type),
                    display(subject)
                ),
                span,
            ))
        }
    }
    fn narrowed_locals(
        &self,
        condition: &Expression,
        locals: &HashMap<String, Symbol>,
        truth: bool,
    ) -> Result<HashMap<String, Symbol>, Diagnostic> {
        if let ExpressionKind::Unary { operator, operand } = &condition.kind
            && operator == "NOT"
        {
            return self.narrowed_locals(operand, locals, !truth);
        }
        let ExpressionKind::Binary {
            operator,
            left,
            right,
        } = &condition.kind
        else {
            return Ok(locals.clone());
        };
        if operator == "AND" {
            if !truth {
                return Ok(locals.clone());
            }
            let left_locals = self.narrowed_locals(left, locals, true)?;
            return self.narrowed_locals(right, &left_locals, true);
        }
        if operator == "OR" {
            if truth {
                return Ok(locals.clone());
            }
            let left_locals = self.narrowed_locals(left, locals, false)?;
            return self.narrowed_locals(right, &left_locals, false);
        }
        if !matches!(operator.as_str(), "IS" | "Assign" | "NotEqual") {
            return Ok(locals.clone());
        }
        let ExpressionKind::Name { name } = &left.kind else {
            return Ok(locals.clone());
        };
        let test_type = if operator == "IS" {
            self.resolve_type(is_test_type(right).ok_or_else(|| {
                error(
                    "INVALID_ALTERNATIVE_USE",
                    "IS requires a valid type test",
                    right.span,
                )
            })?)
        } else {
            let Some(test_type) = is_test_type(right) else {
                return Ok(locals.clone());
            };
            if !matches!(test_type, Type::Null | Type::NotAvailable | Type::EndOfFile) {
                return Ok(locals.clone());
            }
            test_type
        };
        let mut narrowed = locals.clone();
        let symbol = narrowed.get_mut(name).ok_or_else(|| {
            error(
                "NAME_NOT_FOUND",
                format!("name '{name}' is not declared"),
                left.span,
            )
        })?;
        let Type::Alternative(alternatives) = &symbol.ty else {
            return Ok(narrowed);
        };
        let truth = if operator == "NotEqual" {
            !truth
        } else {
            truth
        };
        let choices: Vec<Type> = if truth {
            alternatives
                .iter()
                .filter(|alternative| compatible(alternative, &test_type))
                .cloned()
                .collect()
        } else {
            alternatives
                .iter()
                .filter(|alternative| !compatible(alternative, &test_type))
                .cloned()
                .collect()
        };
        symbol.ty = match choices.as_slice() {
            [] => Type::Unknown,
            [ty] => ty.clone(),
            _ => Type::Alternative(choices),
        };
        Ok(narrowed)
    }

    fn replace_narrowing_fact(
        target: &Expression,
        assigned: &Type,
        locals: &mut HashMap<String, Symbol>,
    ) {
        let ExpressionKind::Name { name } = &target.kind else {
            return;
        };
        let Some(symbol) = locals.get_mut(name) else {
            return;
        };
        if matches!(symbol.declared_ty, Type::Alternative(_)) {
            symbol.ty = default_literal_type(assigned.clone());
        } else {
            symbol.ty = symbol.declared_ty.clone();
        }
    }

    fn direct_base(&self, span: Span) -> Result<String, Diagnostic> {
        let class = self.current_class.as_deref().ok_or_else(|| {
            error(
                "INVALID_SUPER",
                "SUPER is valid only in a derived CLASS",
                span,
            )
        })?;
        self.base_classes.get(class).cloned().ok_or_else(|| {
            error(
                "INVALID_SUPER",
                "SUPER is valid only in a derived CLASS",
                span,
            )
        })
    }

    fn super_constructor_call(
        &mut self,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let base = self.direct_base(span)?;
        let parameters = self
            .constructors
            .get(&base)
            .map_or_else(Vec::new, |constructor| constructor.parameters.clone());
        if arguments.len() != parameters.len() {
            return Err(error(
                "TYPE_MISMATCH",
                format!(
                    "SUPER expects {} argument(s), found {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            ));
        }
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let actual = self.expression_as(argument, &parameter, locals)?;
            if !self.compatible(&parameter, &actual) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!(
                        "cannot pass {} to SUPER parameter {}",
                        display(&actual),
                        display(&parameter)
                    ),
                    argument.span,
                ));
            }
        }
        Ok(Type::Named("VOID".into()))
    }

    fn call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let callee_type = self.expression(callee, locals)?;
        if let ExpressionKind::Member { object, name } = &callee.kind
            && let Some(Type::Module(module)) = self
                .expressions
                .iter()
                .find(|expression| expression.span == object.span)
                .map(|expression| &expression.ty)
            && self.bnmath_modules.contains(module)
        {
            return self.math_call(name, arguments, locals, span);
        }
        let Type::Function {
            parameters,
            return_type,
        } = callee_type
        else {
            for argument in arguments {
                self.expression(argument, locals)?;
            }
            return Err(error(
                "NOT_CALLABLE",
                format!("{} is not callable", display(&callee_type)),
                span,
            ));
        };
        if arguments.len() != parameters.len() {
            return Err(error(
                "TYPE_MISMATCH",
                format!(
                    "call expects {} argument(s), found {}",
                    parameters.len(),
                    arguments.len()
                ),
                span,
            ));
        }
        for (argument, parameter) in arguments.iter().zip(parameters) {
            let actual = self.expression_as(argument, &parameter, locals)?;
            if !self.compatible(&parameter, &actual) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!(
                        "cannot pass {} to parameter of type {}",
                        display(&actual),
                        display(&parameter)
                    ),
                    argument.span,
                ));
            }
        }
        if let ExpressionKind::Member { name, .. } = &callee.kind
            && name == "Open"
            && let Some(mode) = arguments.get(1).and_then(constant_integer)
            && !matches!(mode, 0..=2)
        {
            return Err(error(
                "INVALID_FILE_MODE",
                "FS.Open mode must be FS.READ, FS.WRITE, or FS.APPEND",
                arguments[1].span,
            ));
        }
        Ok(*return_type)
    }

    #[allow(clippy::too_many_lines)]
    fn math_call(
        &mut self,
        name: &str,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let expected_count = match name {
            "MIN" | "MAX" | "ROUND" | "POW" | "ATAN2" | "HYPOT" | "TOTIMESTAMP" => 2,
            "FMA" => 3,
            _ => 1,
        };
        if matches!(name, "MIN" | "MAX") && arguments.len() == 1 {
            let ty = self.expression(&arguments[0], locals)?;
            if matches!(ty, Type::Vector { ref element, .. } if is_numeric(element))
                || matches!(ty, Type::Pointer { ref element, .. } if is_numeric(element))
            {
                return Ok(match ty {
                    Type::Vector { element, .. } | Type::Pointer { element, .. } => *element,
                    _ => unreachable!(),
                });
            }
            return Err(error(
                "TYPE_MISMATCH",
                "BNMath.MIN/MAX expects a numeric vector",
                span,
            ));
        }
        if !(matches!(name, "MIN" | "MAX") && arguments.len() == 1)
            && arguments.len() != expected_count
        {
            return Err(error(
                "TYPE_MISMATCH",
                format!("BNMath.{name} expects {expected_count} argument(s)"),
                span,
            ));
        }
        let types = arguments
            .iter()
            .map(|argument| self.expression(argument, locals))
            .collect::<Result<Vec<_>, _>>()?;
        if name == "VAL" {
            if types[0] != Type::String {
                return Err(error("TYPE_MISMATCH", "BNMath.VAL expects STRING", span));
            }
            return Ok(Type::Float(FloatType::Float64));
        }
        if matches!(
            name,
            "MEAN" | "MEDIAN" | "QUARTILE1" | "QUARTILE3" | "MODE" | "STDEV" | "VARIANCE" | "RANGE"
        ) {
            let valid = matches!(types[0], Type::Vector { ref element, .. } if is_numeric(element))
                || matches!(types[0], Type::Pointer { ref element, .. } if is_numeric(element));
            if !valid {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} expects a numeric vector"),
                    span,
                ));
            }
            return Ok(if name == "MODE" {
                Type::Alternative(vec![Type::Float(FloatType::Float64), Type::NotAvailable])
            } else {
                Type::Float(FloatType::Float64)
            });
        }
        if matches!(name, "TOHOUR" | "TOWEEKDAY") {
            if !matches!(types[0], Type::Integer(IntegerType::Int64)) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} expects TIMESTAMP"),
                    span,
                ));
            }
            return Ok(Type::Integer(IntegerType::Int32));
        }
        if matches!(name, "TODATE" | "TOTIME") {
            if !matches!(types[0], Type::Integer(IntegerType::Int64)) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} expects TIMESTAMP"),
                    span,
                ));
            }
            return Ok(Type::Named(
                if name == "TODATE" { "DATE" } else { "TIME" }.into(),
            ));
        }
        if name == "TOTIMESTAMP" {
            if types != [Type::Named("DATE".into()), Type::Named("TIME".into())] {
                return Err(error(
                    "TYPE_MISMATCH",
                    "BNMath.TOTIMESTAMP expects DATE and TIME",
                    span,
                ));
            }
            return Ok(Type::Integer(IntegerType::Int64));
        }
        if name == "ROUND" {
            if !is_float(&types[0]) || !is_integer(&types[1]) {
                return Err(error(
                    "TYPE_MISMATCH",
                    "BNMath.ROUND expects a floating value and an INTEGER digit count",
                    span,
                ));
            }
            return Ok(default_literal_type(types[0].clone()));
        }
        if matches!(name, "ABS" | "MIN" | "MAX" | "SIGN") {
            let mut result = types[0].clone();
            for ty in &types[1..] {
                result = numeric_result(&result, ty).ok_or_else(|| {
                    error(
                        "TYPE_MISMATCH",
                        format!("BNMath.{name} requires compatible numeric arguments"),
                        span,
                    )
                })?;
            }
            if !is_numeric(&result) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("BNMath.{name} requires numeric arguments"),
                    span,
                ));
            }
            return Ok(default_literal_type(result));
        }
        if types.iter().any(|ty| !is_float(ty)) {
            return Err(error(
                "TYPE_MISMATCH",
                format!("BNMath.{name} requires floating-point arguments"),
                span,
            ));
        }
        let result = types
            .into_iter()
            .reduce(|left, right| numeric_result(&left, &right).expect("floats are compatible"))
            .expect("BNMath functions have arguments");
        Ok(default_literal_type(result))
    }
    fn declare_global(
        &mut self,
        name: &str,
        ty: Type,
        constant: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let symbol = self.symbol(ty, constant);
        if self.globals.insert(name.into(), symbol).is_some() {
            return Err(error(
                "DUPLICATE_NAME",
                format!("duplicate top-level declaration '{name}'"),
                span,
            ));
        }
        let record = self.globals.get(name).expect("inserted global").clone();
        self.record_symbol(name, &record, span);
        Ok(())
    }
    fn declare_local(
        &mut self,
        locals: &mut HashMap<String, Symbol>,
        name: &str,
        ty: Type,
        constant: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let symbol = self.symbol(ty, constant);
        if locals.insert(name.into(), symbol).is_some() {
            return Err(error(
                "DUPLICATE_NAME",
                format!("duplicate binding '{name}' in the same scope"),
                span,
            ));
        }
        let record = locals.get(name).expect("inserted local").clone();
        self.record_symbol(name, &record, span);
        Ok(())
    }
    fn symbol(&mut self, ty: Type, constant: bool) -> Symbol {
        let symbol = Symbol {
            id: SymbolId(self.next_symbol),
            type_id: TypeId(self.next_type),
            declared_ty: ty.clone(),
            ty,
            constant,
        };
        self.next_symbol += 1;
        self.next_type += 1;
        symbol
    }
    fn record_symbol(&mut self, name: &str, symbol: &Symbol, span: Span) {
        self.symbols.push(ResolvedSymbol {
            id: symbol.id,
            type_id: symbol.type_id,
            name: name.into(),
            type_name: display(&symbol.ty),
            ty: symbol.ty.clone(),
            constant: symbol.constant,
            span,
        });
    }
    fn lookup<'a>(&'a self, name: &str, locals: &'a HashMap<String, Symbol>) -> Option<&'a Symbol> {
        locals.get(name).or_else(|| self.globals.get(name))
    }

    fn compatible(&self, expected: &Type, actual: &Type) -> bool {
        if compatible(expected, actual) {
            return true;
        }
        match (expected, actual) {
            (Type::Named(expected), Type::Named(actual))
                if self.is_subclass_of(actual, expected) =>
            {
                true
            }
            (Type::ImportedNamed { module, name }, Type::Named(class))
                if self.is_subclass_of(class, &format!("#{}.{name}", module.0)) =>
            {
                true
            }
            (
                Type::ImportedNamed {
                    module,
                    name: interface,
                },
                Type::Named(class),
            ) => self.implementations.get(class).is_some_and(|interfaces| {
                interfaces.iter().any(|implemented| {
                    implemented.split_once('.').is_some_and(|(alias, name)| {
                        name == interface && self.module_imports.get(alias) == Some(module)
                    })
                })
            }),
            (Type::Named(interface), Type::Named(class)) => self
                .implementations
                .get(class)
                .is_some_and(|interfaces| interfaces.contains(interface)),
            (
                Type::ImportedNamed {
                    module: interface_module,
                    name: interface,
                },
                Type::ImportedNamed {
                    module: class_module,
                    name: class,
                },
            ) if interface_module == class_module => self
                .imported_types
                .get(&(*class_module, class.clone()))
                .is_some_and(|info| info.interfaces.contains(interface)),
            (Type::Alternative(expected), actual) => expected
                .iter()
                .any(|expected| self.compatible(expected, actual)),
            (expected, Type::Alternative(actual)) => actual
                .iter()
                .all(|actual| self.compatible(expected, actual)),
            (
                Type::Vector {
                    element: expected,
                    dimensions: expected_dimensions,
                },
                Type::Vector {
                    element: actual,
                    dimensions: actual_dimensions,
                },
            ) => {
                (expected_dimensions == actual_dimensions
                    || (expected_dimensions.len() == 1 && expected_dimensions[0] == u64::MAX))
                    && self.compatible(expected, actual)
            }
            _ => false,
        }
    }

    fn is_subclass_of(&self, class: &str, ancestor: &str) -> bool {
        let mut current = class;
        while let Some(base) = self.base_classes.get(current) {
            if base == ancestor {
                return true;
            }
            current = base;
        }
        false
    }

    fn member_owner(&self, class: &str, name: &str) -> Option<String> {
        if self
            .declared_members
            .get(class)
            .is_some_and(|members| members.contains(name))
        {
            return Some(class.into());
        }
        self.base_classes
            .get(class)
            .and_then(|base| self.member_owner(base, name))
            .or_else(|| Some(class.into()))
    }

    fn deletable(&self, ty: &Type) -> bool {
        match ty {
            Type::Pointer { .. } | Type::Null => true,
            Type::Named(name) => {
                name == "FS.File"
                    || self.declaration_kinds.get(name) == Some(&DeclarationKind::Class)
            }
            Type::ImportedNamed { module, name } => self
                .imported_types
                .get(&(*module, name.clone()))
                .is_some_and(|info| info.kind == DeclarationKind::Class),
            Type::Alternative(alternatives) => alternatives
                .iter()
                .all(|alternative| self.deletable(alternative)),
            _ => false,
        }
    }

    fn member_mutable(&self, object: &Type, name: &str) -> bool {
        match object {
            Type::Named(owner) | Type::TypeName(owner) => self
                .members
                .get(owner)
                .and_then(|members| members.get(name))
                .is_some_and(|member| member.mutable),
            Type::ImportedNamed {
                module,
                name: owner,
            }
            | Type::ImportedTypeName {
                module,
                name: owner,
            } => self
                .imported_types
                .get(&(*module, owner.clone()))
                .and_then(|info| info.members.get(name))
                .is_some_and(|member| member.mutable),
            Type::Alternative(alternatives) => alternatives
                .iter()
                .filter(|alternative| !matches!(alternative, Type::Null | Type::NotAvailable))
                .all(|alternative| self.member_mutable(alternative, name)),
            _ => false,
        }
    }

    fn validate_type_reference(&self, reference: &TypeReference) -> Result<(), Diagnostic> {
        validate_type_reference(reference)?;
        if !self.allow_variable_vectors
            && reference.alternatives.iter().any(|atom| {
                atom.name != "POINTER"
                    && atom
                        .parts
                        .windows(2)
                        .any(|parts| parts == ["LeftBracket", "RightBracket"])
            })
        {
            return Err(error(
                "INVALID_VECTOR_TYPE",
                "variable-length vectors are reserved for the BNData library",
                reference.span,
            ));
        }
        for atom in &reference.alternatives {
            let ty = type_from_atom(atom);
            let name = match ty {
                Type::Named(name) => name,
                Type::Pointer { element, .. } => match *element {
                    Type::Named(name) => name,
                    _ => continue,
                },
                _ => continue,
            };
            if name == "VOID" {
                continue;
            }
            if matches!(name.as_str(), "DATE" | "TIME" | "TIMEZONE" | "Error")
                || self.declaration_kinds.contains_key(&name)
            {
                continue;
            }
            if let Some((alias, exported_name)) = name.split_once('.')
                && exported_name == "File"
                && self
                    .globals
                    .get(alias)
                    .is_some_and(|symbol| symbol.ty == Type::HostFileSystem)
            {
                continue;
            }
            if let Some((alias, exported_name)) = name.split_once('.')
                && let Some(Type::Module(module)) = self.globals.get(alias).map(|symbol| &symbol.ty)
                && self
                    .module_exports
                    .get(module)
                    .is_some_and(|exports| exports.contains_key(exported_name))
            {
                continue;
            }
            return Err(error(
                "UNKNOWN_TYPE",
                format!("type '{name}' is not declared or imported"),
                atom.span,
            ));
        }
        Ok(())
    }

    fn resolve_reference(&self, reference: &TypeReference) -> Type {
        self.resolve_type(type_from_reference(reference))
    }

    fn resolve_type(&self, ty: Type) -> Type {
        match ty {
            Type::Named(name) => {
                if let Some((alias, exported_name)) = name.split_once('.')
                    && exported_name == "File"
                    && self
                        .globals
                        .get(alias)
                        .is_some_and(|symbol| symbol.ty == Type::HostFileSystem)
                {
                    return Type::Named(name);
                }
                if let Some((alias, exported_name)) = name.split_once('.')
                    && let Some(Type::Module(module)) =
                        self.globals.get(alias).map(|symbol| &symbol.ty)
                    && matches!(
                        self.module_exports
                            .get(module)
                            .and_then(|exports| exports.get(exported_name)),
                        Some(Type::ImportedTypeName { .. })
                    )
                {
                    return Type::ImportedNamed {
                        module: *module,
                        name: exported_name.into(),
                    };
                }
                Type::Named(name)
            }
            Type::Alternative(types) => {
                Type::Alternative(types.into_iter().map(|ty| self.resolve_type(ty)).collect())
            }
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters
                    .into_iter()
                    .map(|ty| self.resolve_type(ty))
                    .collect(),
                return_type: Box::new(self.resolve_type(*return_type)),
            },
            Type::Vector {
                element,
                dimensions,
            } => Type::Vector {
                element: Box::new(self.resolve_type(*element)),
                dimensions,
            },
            Type::Pointer { element, length } => Type::Pointer {
                element: Box::new(self.resolve_type(*element)),
                length,
            },
            ty => ty,
        }
    }

    fn type_requires_initializer(&self, ty: &Type) -> bool {
        match ty {
            Type::Alternative(_) | Type::Pointer { .. } | Type::Function { .. } => true,
            Type::Named(name) => matches!(
                self.declaration_kinds.get(name),
                Some(DeclarationKind::Class | DeclarationKind::Interface)
            ),
            _ => false,
        }
    }

    fn sizeof_type(&self, ty: &Type, span: Span) -> Result<Type, Diagnostic> {
        if matches!(ty, Type::String)
            || matches!(ty, Type::Vector { element, .. } if self.sizeof_allowed(element))
        {
            if let Some(size) = self.byte_size(ty) {
                require_integer_fit(Some(size), span)?;
            }
            return Ok(Type::Integer(IntegerType::Int32));
        }
        match self.byte_size(ty) {
            Some(size) => {
                require_integer_fit(Some(size), span)?;
                Ok(Type::Integer(IntegerType::Int32))
            }
            None => Err(error(
                "TYPE_MISMATCH",
                "SIZEOF requires a value with a defined byte size",
                span,
            )),
        }
    }

    fn sizeof_allowed(&self, ty: &Type) -> bool {
        matches!(ty, Type::String)
            || static_size_of(ty).is_some()
            || self.byte_size(ty).is_some()
            || matches!(ty, Type::Vector { element, .. } if self.sizeof_allowed(element))
    }

    fn byte_size(&self, ty: &Type) -> Option<u64> {
        if let Some(size) = static_size_of(ty) {
            return Some(size);
        }
        match ty {
            Type::Named(name) => self.layouts.get(name).copied(),
            Type::Vector {
                element,
                dimensions,
            } => self
                .byte_size(element)
                .and_then(|element| dimension_product(dimensions)?.checked_mul(element)),
            _ => None,
        }
    }

    fn compute_layouts(&mut self) {
        let names: Vec<String> = self.declaration_kinds.keys().cloned().collect();
        for name in names {
            self.ensure_layout(&name);
        }
    }

    fn ensure_layout(&mut self, name: &str) -> Option<u64> {
        if let Some(size) = self.layouts.get(name) {
            return Some(*size);
        }
        let kind = *self.declaration_kinds.get(name)?;
        if !matches!(kind, DeclarationKind::Struct | DeclarationKind::Class) {
            return None;
        }
        let members = self.members.get(name)?.clone();
        let mut total = 0u64;
        for member in members.values() {
            if member.is_static || matches!(member.ty, Type::Function { .. }) {
                continue;
            }
            let size = self.field_static_size(&member.ty)?;
            total = total.checked_add(size)?;
        }
        self.layouts.insert(name.to_string(), total);
        Some(total)
    }

    fn field_static_size(&mut self, ty: &Type) -> Option<u64> {
        if let Some(size) = static_size_of(ty) {
            return Some(size);
        }
        match ty {
            Type::Named(name)
                if self.declaration_kinds.get(name) == Some(&DeclarationKind::Struct) =>
            {
                self.ensure_layout(name)
            }
            Type::Vector {
                element,
                dimensions,
            } => {
                let element = self.field_static_size(element)?;
                dimension_product(dimensions)?.checked_mul(element)
            }
            _ => None,
        }
    }
}

fn declaration_type(kind: DeclarationKind, name: &str) -> Type {
    match kind {
        DeclarationKind::Function => Type::Unknown,
        _ => Type::TypeName(name.into()),
    }
}
fn function_type(signature: &FunctionSignature) -> Type {
    Type::Function {
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| type_from_reference(&parameter.type_ref))
            .collect(),
        return_type: Box::new(type_from_reference(&signature.return_type)),
    }
}
fn type_from_reference(reference: &TypeReference) -> Type {
    let alternatives = reference
        .alternatives
        .iter()
        .map(type_from_atom)
        .collect::<Vec<_>>();
    match alternatives.as_slice() {
        [] => Type::Unknown,
        [ty] => ty.clone(),
        _ => Type::Alternative(alternatives),
    }
}
fn type_from_atom(atom: &crate::ast::TypeAtom) -> Type {
    if atom.name == "FUNCTION" {
        return function_type_from_parts(&atom.parts);
    }
    if atom.name == "POINTER" {
        let element = pointer_element_type(&atom.parts);
        let length = pointer_length(&atom.parts);
        return Type::Pointer {
            element: Box::new(element),
            length,
        };
    }
    let name = qualified_type_name(atom);
    let base = match name.as_str() {
        "BOOLEAN" => Type::Boolean,
        "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64"
        | "INTEGER" | "TIMESTAMP" => {
            Type::Integer(integer_type(atom.name.as_str()).expect("numeric type"))
        }
        "FLOAT32" | "FLOAT64" | "FLOAT" => {
            Type::Float(float_type(atom.name.as_str()).expect("float type"))
        }
        "STRING" => Type::String,
        "NULL" => Type::Null,
        "NA" => Type::NotAvailable,
        "EOF" => Type::EndOfFile,
        "POINTER" => Type::Unknown,
        "SYSTEM" => Type::System,
        name => Type::Named(name.into()),
    };
    let dimensions = atom
        .parts
        .windows(3)
        .filter(|parts| parts[0] == "LeftBracket" && parts[2] == "RightBracket")
        .filter_map(|parts| parse_integer(&parts[1]).and_then(|value| u64::try_from(value).ok()))
        .collect::<Vec<_>>();
    let has_vector_brackets = atom.parts.iter().any(|part| part == "LeftBracket");
    if dimensions.is_empty() && !has_vector_brackets {
        base
    } else {
        Type::Vector {
            element: Box::new(base),
            dimensions: if dimensions.is_empty() {
                vec![u64::MAX]
            } else {
                dimensions
            },
        }
    }
}

fn function_type_from_parts(parts: &[String]) -> Type {
    let Some(close) = matching_right_paren(parts) else {
        return Type::Unknown;
    };
    let parameter_tokens = &parts[1..close];
    let mut parameters = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=parameter_tokens.len() {
        let separator =
            index == parameter_tokens.len() || (parameter_tokens[index] == "Comma" && depth == 0);
        if separator {
            if start < index {
                parameters.push(type_from_tokens(&parameter_tokens[start..index]));
            }
            start = index + 1;
            continue;
        }
        match parameter_tokens[index].as_str() {
            "LeftParen" | "LeftBracket" => depth += 1,
            "RightParen" | "RightBracket" => depth -= 1,
            _ => {}
        }
    }
    let return_type = parts
        .get(close + 1)
        .filter(|part| part.as_str() == "AS")
        .map_or(Type::Unknown, |_| type_from_tokens(&parts[close + 2..]));
    Type::Function {
        parameters,
        return_type: Box::new(return_type),
    }
}

fn matching_right_paren(parts: &[String]) -> Option<usize> {
    if parts.first()?.as_str() != "LeftParen" {
        return None;
    }
    let mut depth = 0;
    for (index, part) in parts.iter().enumerate() {
        match part.as_str() {
            "LeftParen" => depth += 1,
            "RightParen" => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn type_from_tokens(tokens: &[String]) -> Type {
    let mut alternatives = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for index in 0..=tokens.len() {
        let separator = index == tokens.len() || (tokens[index] == "OR" && depth == 0);
        if separator {
            if start < index {
                alternatives.push(type_from_atom(&crate::ast::TypeAtom {
                    name: tokens[start].clone(),
                    parts: tokens[start + 1..index].to_vec(),
                    span: default_span(),
                }));
            }
            start = index + 1;
            continue;
        }
        match tokens[index].as_str() {
            "LeftParen" | "LeftBracket" => depth += 1,
            "RightParen" | "RightBracket" => depth -= 1,
            _ => {}
        }
    }
    match alternatives.as_slice() {
        [ty] => ty.clone(),
        _ => Type::Alternative(alternatives),
    }
}

fn qualified_type_name(atom: &crate::ast::TypeAtom) -> String {
    let mut name = atom.name.clone();
    let mut parts = atom.parts.iter();
    while matches!(parts.next(), Some(part) if part == "Dot") {
        let Some(part) = parts.next() else { break };
        name.push('.');
        name.push_str(part);
    }
    name
}

fn pointer_element_type(parts: &[String]) -> Type {
    let Some(start) = parts
        .iter()
        .position(|part| part == "TO")
        .map(|index| index + 1)
    else {
        return Type::Unknown;
    };
    let end = parts[start..]
        .iter()
        .position(|part| part == "LeftBracket")
        .map_or(parts.len(), |index| start + index);
    let element = &parts[start..end];
    let Some(first) = element.first() else {
        return Type::Unknown;
    };
    if element.len() == 1 {
        return type_from_name(first);
    }
    let mut name = first.clone();
    let mut suffix = element[1..].chunks_exact(2);
    for pair in &mut suffix {
        if pair[0] != "Dot" {
            return Type::Unknown;
        }
        name.push('.');
        name.push_str(&pair[1]);
    }
    if !suffix.remainder().is_empty() {
        return Type::Unknown;
    }
    type_from_name(&name)
}

fn pointer_length(parts: &[String]) -> PointerLength {
    let Some(open) = parts.iter().position(|part| part == "LeftBracket") else {
        return PointerLength::One;
    };
    parts
        .get(open + 1)
        .filter(|part| part.as_str() != "RightBracket")
        .and_then(|part| parse_integer(part))
        .and_then(|part| u64::try_from(part).ok())
        .map_or(PointerLength::Dynamic, PointerLength::Fixed)
}

fn allocation_length(arguments: &[Expression]) -> PointerLength {
    arguments.first().map_or(PointerLength::One, |length| {
        let ExpressionKind::Literal(Literal::Integer(length)) = &length.kind else {
            return PointerLength::Dynamic;
        };
        parse_integer(length)
            .and_then(|length| u64::try_from(length).ok())
            .map_or(PointerLength::Dynamic, PointerLength::Fixed)
    })
}
fn literal_type(literal: &Literal) -> Type {
    match literal {
        Literal::Integer(value) => Type::IntegerLiteral(value.clone()),
        Literal::Float(_) | Literal::Special(_) => Type::FloatLiteral,
        Literal::String(_) => Type::String,
        Literal::TypeName(_) => Type::Unknown,
        Literal::Boolean(_) => Type::Boolean,
        Literal::Null => Type::Null,
        Literal::NotAvailable => Type::NotAvailable,
        Literal::EndOfFile => Type::EndOfFile,
    }
}
fn is_test_type(expression: &Expression) -> Option<Type> {
    match &expression.kind {
        ExpressionKind::TypeTest { type_ref } => Some(type_from_reference(type_ref)),
        ExpressionKind::Literal(Literal::Null) => Some(Type::Null),
        ExpressionKind::Literal(Literal::NotAvailable) => Some(Type::NotAvailable),
        ExpressionKind::Literal(Literal::EndOfFile) => Some(Type::EndOfFile),
        ExpressionKind::Literal(Literal::Special(_)) => Some(Type::FloatLiteral),
        ExpressionKind::Unary { operator, operand } if operator == "Minus" => is_test_type(operand),
        ExpressionKind::Literal(Literal::TypeName(name)) | ExpressionKind::Name { name } => {
            Some(type_from_name(name))
        }
        _ => None,
    }
}
fn type_from_name(name: &str) -> Type {
    match name {
        "BOOLEAN" => Type::Boolean,
        "BYTE" | "INT8" | "INT16" | "INT32" | "INT64" | "UINT16" | "UINT32" | "UINT64"
        | "INTEGER" | "TIMESTAMP" => Type::Integer(integer_type(name).expect("numeric type")),
        "FLOAT32" | "FLOAT64" | "FLOAT" => Type::Float(float_type(name).expect("float type")),
        "STRING" => Type::String,
        "SYSTEM" => Type::System,
        name => Type::Named(name.into()),
    }
}
fn integer_type(name: &str) -> Option<IntegerType> {
    match name {
        "BYTE" => Some(IntegerType::Byte),
        "INT8" => Some(IntegerType::Int8),
        "INT16" => Some(IntegerType::Int16),
        "INT32" | "INTEGER" => Some(IntegerType::Int32),
        "INT64" | "TIMESTAMP" => Some(IntegerType::Int64),
        "UINT16" => Some(IntegerType::UInt16),
        "UINT32" => Some(IntegerType::UInt32),
        "UINT64" => Some(IntegerType::UInt64),
        _ => None,
    }
}
fn float_type(name: &str) -> Option<FloatType> {
    match name {
        "FLOAT32" => Some(FloatType::Float32),
        "FLOAT64" | "FLOAT" => Some(FloatType::Float64),
        _ => None,
    }
}

fn integer_literal_fits(value: &str, target: IntegerType) -> bool {
    let parsed = parse_integer(value);
    let Some(value) = parsed else { return false };
    let (minimum, maximum) = integer_range(target);
    (minimum..=maximum).contains(&value)
}

fn integer_range(target: IntegerType) -> (i128, i128) {
    match target {
        IntegerType::Byte => (0, 255),
        IntegerType::Int8 => (-128, 127),
        IntegerType::Int16 => (-32_768, 32_767),
        IntegerType::Int32 => (-2_147_483_648, 2_147_483_647),
        IntegerType::Int64 => (i128::from(i64::MIN), i128::from(i64::MAX)),
        IntegerType::UInt16 => (0, 65_535),
        IntegerType::UInt32 => (0, 4_294_967_295),
        IntegerType::UInt64 => (0, i128::from(u64::MAX)),
    }
}

fn parse_integer(value: &str) -> Option<i128> {
    if let Some(value) = value.strip_prefix("0b") {
        i128::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i128::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}

fn constant_integer_from_type(ty: &Type) -> Option<i128> {
    let Type::IntegerLiteral(value) = ty else {
        return None;
    };
    parse_integer(value)
}

fn constant_integer(expression: &Expression) -> Option<i128> {
    match &expression.kind {
        ExpressionKind::Literal(Literal::Integer(value)) => parse_integer(value),
        ExpressionKind::Unary { operator, operand } if operator == "Minus" => {
            constant_integer(operand)?.checked_neg()
        }
        ExpressionKind::Binary {
            operator,
            left,
            right,
        } => {
            let left = constant_integer(left)?;
            let right = constant_integer(right)?;
            match operator.as_str() {
                "Plus" => left.checked_add(right),
                "Minus" => left.checked_sub(right),
                "Star" => left.checked_mul(right),
                "DIV" if right != 0 => left.checked_div_euclid(right),
                "Percent" if right != 0 => left.checked_rem_euclid(right),
                "Power" if right >= 0 => left.checked_pow(u32::try_from(right).ok()?),
                "SHL" if (0..128).contains(&right) => left.checked_shl(u32::try_from(right).ok()?),
                "SHR" if (0..128).contains(&right) => left.checked_shr(u32::try_from(right).ok()?),
                _ => None,
            }
        }
        _ => None,
    }
}

fn default_literal_type(ty: Type) -> Type {
    match ty {
        Type::IntegerLiteral(_) => Type::Integer(IntegerType::Int32),
        Type::FloatLiteral => Type::Float(FloatType::Float64),
        other => other,
    }
}

fn is_numeric(ty: &Type) -> bool {
    is_integer(ty) || is_float(ty)
}

#[must_use]
pub fn static_len(ty: &Type) -> Option<u64> {
    if is_numeric(ty) {
        return Some(1);
    }
    if let Type::Vector { dimensions, .. } = ty {
        return dimension_product(dimensions);
    }
    None
}

#[must_use]
pub fn static_size_of(ty: &Type) -> Option<u64> {
    match ty {
        Type::Boolean => Some(1),
        Type::Integer(kind) => Some(integer_byte_size(*kind)),
        Type::IntegerLiteral(_) | Type::Float(FloatType::Float32) => Some(4),
        Type::Float(FloatType::Float64) | Type::FloatLiteral => Some(8),
        Type::Named(name) if name == "DATE" || name == "TIME" => Some(4),
        Type::Vector {
            element,
            dimensions,
        } => static_size_of(element)
            .and_then(|element| dimension_product(dimensions)?.checked_mul(element)),
        _ => None,
    }
}

#[must_use]
pub fn integer_byte_size(kind: IntegerType) -> u64 {
    match kind {
        IntegerType::Byte | IntegerType::Int8 => 1,
        IntegerType::Int16 | IntegerType::UInt16 => 2,
        IntegerType::Int32 | IntegerType::UInt32 => 4,
        IntegerType::Int64 | IntegerType::UInt64 => 8,
    }
}

fn dimension_product(dimensions: &[u64]) -> Option<u64> {
    if dimensions.contains(&u64::MAX) {
        return None;
    }
    dimensions
        .iter()
        .try_fold(1u64, |product, dimension| product.checked_mul(*dimension))
}

fn host_capability_type(name: &str, span: Span) -> Result<Type, Diagnostic> {
    match name {
        "Args" => Ok(Type::HostArgs),
        "Console" => Ok(Type::HostConsole),
        "Main" => Err(error(
            "NAME_NOT_FOUND",
            "HOST.Main was withdrawn in 0.2; use HOST.Args",
            span,
        )),
        "Clock" => Ok(Type::HostClock),
        "Random" => Ok(Type::HostRandom),
        "FileSystem" => Ok(Type::HostFileSystem),
        _ => Err(error(
            "NAME_NOT_FOUND",
            format!("HOST.{name} is not a Basic Next 0.2 capability"),
            span,
        )),
    }
}

fn length_type(ty: &Type, span: Span) -> Result<Type, Diagnostic> {
    if is_numeric(ty) {
        return Ok(Type::Integer(IntegerType::Int32));
    }
    match ty {
        Type::HostArgs
        | Type::String
        | Type::Pointer {
            length: PointerLength::Dynamic,
            ..
        } => Ok(Type::Integer(IntegerType::Int32)),
        Type::Vector { dimensions, .. } => {
            require_integer_fit(dimension_product(dimensions), span)?;
            Ok(Type::Integer(IntegerType::Int32))
        }
        Type::Pointer {
            length: PointerLength::Fixed(length),
            ..
        } => {
            require_integer_fit(Some(*length), span)?;
            Ok(Type::Integer(IntegerType::Int32))
        }
        _ => Err(error(
            "TYPE_MISMATCH",
            "LEN requires a numeric value, STRING, vector, or pointer region",
            span,
        )),
    }
}

fn require_integer_fit(value: Option<u64>, span: Span) -> Result<(), Diagnostic> {
    match value {
        Some(value) if value <= 2_147_483_647 => Ok(()),
        None | Some(_) => Err(error(
            "NUMERIC_OVERFLOW",
            "result does not fit INTEGER",
            span,
        )),
    }
}
fn requires_initializer(reference: &TypeReference) -> bool {
    reference.alternatives.len() > 1
        || reference
            .alternatives
            .iter()
            .any(|atom| matches!(atom.name.as_str(), "POINTER" | "FUNCTION"))
}
fn compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Alternative(expected), Type::Alternative(actual)) => actual
            .iter()
            .all(|actual| expected.iter().any(|expected| compatible(expected, actual))),
        (Type::Alternative(expected), actual) => {
            expected.iter().any(|expected| compatible(expected, actual))
        }
        (expected, Type::Alternative(actual)) => {
            actual.iter().all(|actual| compatible(expected, actual))
        }
        (Type::Integer(expected), Type::IntegerLiteral(value)) => {
            integer_literal_fits(value, *expected)
        }
        (Type::Float(_), Type::FloatLiteral) => true,
        (
            Type::Vector {
                element: expected,
                dimensions: expected_dimensions,
            },
            Type::Vector {
                element: actual,
                dimensions: actual_dimensions,
            },
        ) => {
            (expected_dimensions == actual_dimensions
                || (expected_dimensions.len() == 1 && expected_dimensions[0] == u64::MAX))
                && compatible(expected, actual)
        }
        (
            Type::Pointer {
                element: expected,
                length: expected_length,
            },
            Type::Pointer {
                element: actual,
                length: actual_length,
            },
        ) => {
            (is_void(expected) || is_void(actual) || compatible(expected, actual))
                && pointer_lengths_compatible(*expected_length, *actual_length)
        }
        (expected, actual) => expected == actual,
    }
}
fn pointer_lengths_compatible(expected: PointerLength, actual: PointerLength) -> bool {
    match (expected, actual) {
        (PointerLength::One, PointerLength::One)
        | (PointerLength::Dynamic, PointerLength::Fixed(_) | PointerLength::Dynamic)
        | (PointerLength::Fixed(_), PointerLength::Dynamic) => true,
        (PointerLength::Fixed(expected), PointerLength::Fixed(actual)) => expected == actual,
        _ => false,
    }
}
fn is_void(ty: &Type) -> bool {
    matches!(ty, Type::Named(name) if name == "VOID")
}
fn pointer_literal_length_mismatch(expected: &Type, actual: &Type) -> bool {
    matches!(
        (expected, actual),
        (
            Type::Pointer {
                length: PointerLength::Fixed(expected),
                ..
            },
            Type::Pointer {
                length: PointerLength::Fixed(actual),
                ..
            }
        ) if expected != actual
    )
}
fn is_integer(ty: &Type) -> bool {
    matches!(ty, Type::Integer(_) | Type::IntegerLiteral(_))
        || matches!(ty, Type::Alternative(alternatives) if alternatives.iter().all(is_integer))
}
fn is_float(ty: &Type) -> bool {
    matches!(ty, Type::Float(_) | Type::FloatLiteral)
        || matches!(ty, Type::Alternative(alternatives) if alternatives.iter().all(is_float))
}
fn integer_literal(expression: &Expression) -> Option<i64> {
    let ExpressionKind::Literal(Literal::Integer(value)) = &expression.kind else {
        return None;
    };
    if let Some(value) = value.strip_prefix("0b") {
        i64::from_str_radix(value, 2).ok()
    } else if let Some(value) = value.strip_prefix("0x") {
        i64::from_str_radix(value, 16).ok()
    } else {
        value.parse().ok()
    }
}
fn vector_shape(expression: &Expression) -> Option<Vec<usize>> {
    let ExpressionKind::Vector { values } = &expression.kind else {
        return Some(Vec::new());
    };
    let mut element_shape = None;
    for value in values {
        let shape = vector_shape(value)?;
        if let Some(expected) = &element_shape {
            if expected != &shape {
                return None;
            }
        } else {
            element_shape = Some(shape);
        }
    }
    let mut shape = vec![values.len()];
    if let Some(element_shape) = element_shape {
        shape.extend(element_shape);
    }
    Some(shape)
}
fn display(ty: &Type) -> String {
    match ty {
        Type::Named(name) | Type::TypeName(name) => name.clone(),
        Type::ImportedNamed { module, name } | Type::ImportedTypeName { module, name } => {
            format!("MODULE#{}.{}", module.0, name)
        }
        Type::System => "SYSTEM".into(),
        Type::HostClock => "HOST.Clock".into(),
        Type::HostRandom => "HOST.Random".into(),
        Type::HostConsole => "HOST.Console".into(),
        Type::HostFileSystem => "HOST.FileSystem".into(),
        Type::Module(id) => format!("MODULE {}", id.0),
        Type::Alternative(alternatives) => alternatives
            .iter()
            .map(display)
            .collect::<Vec<_>>()
            .join(" OR "),
        Type::Function {
            parameters,
            return_type,
        } => format!(
            "FUNCTION({}) AS {}",
            parameters
                .iter()
                .map(display)
                .collect::<Vec<_>>()
                .join(", "),
            display(return_type)
        ),
        Type::Vector {
            element,
            dimensions,
        } => format!(
            "{}{}",
            display(element),
            dimensions
                .iter()
                .fold(String::new(), |mut name, dimension| {
                    use std::fmt::Write;
                    if *dimension == u64::MAX {
                        name.push_str("[]");
                    } else {
                        let _ = write!(name, "[{dimension}]");
                    }
                    name
                })
        ),
        Type::Pointer { element, length } => match length {
            PointerLength::One => format!("POINTER TO {}", display(element)),
            PointerLength::Fixed(length) => {
                format!("POINTER TO {}[{length}]", display(element))
            }
            PointerLength::Dynamic => format!("POINTER TO {}[]", display(element)),
        },
        other => format!("{other:?}").to_uppercase(),
    }
}
fn member_target(object: &Type, name: &str) -> Option<MemberTarget> {
    match object {
        Type::Module(module) => Some(MemberTarget {
            module: Some(*module),
            owner: None,
            name: name.into(),
        }),
        Type::Named(owner) | Type::TypeName(owner) => Some(MemberTarget {
            module: None,
            owner: Some(owner.clone()),
            name: name.into(),
        }),
        Type::ImportedNamed {
            module,
            name: owner,
        }
        | Type::ImportedTypeName {
            module,
            name: owner,
        } => Some(MemberTarget {
            module: Some(*module),
            owner: Some(owner.clone()),
            name: name.into(),
        }),
        Type::System => Some(MemberTarget {
            module: None,
            owner: Some("SYSTEM".into()),
            name: name.into(),
        }),
        Type::HostClock => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Clock".into()),
            name: name.into(),
        }),
        Type::HostRandom => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Random".into()),
            name: name.into(),
        }),
        Type::HostFileSystem => Some(MemberTarget {
            module: None,
            owner: Some("HOST.FileSystem".into()),
            name: name.into(),
        }),
        Type::HostConsole => Some(MemberTarget {
            module: None,
            owner: Some("HOST.Console".into()),
            name: name.into(),
        }),
        Type::Alternative(types) => {
            let mut targets = types.iter().filter_map(|ty| member_target(ty, name));
            let target = targets.next()?;
            targets.all(|other| other == target).then_some(target)
        }
        _ => None,
    }
}
fn constructor_target(ty: &Type) -> Option<MemberTarget> {
    match ty {
        Type::Named(owner) => Some(MemberTarget {
            module: None,
            owner: Some(owner.clone()),
            name: "CONSTRUCTOR".into(),
        }),
        Type::ImportedNamed {
            module,
            name: owner,
        } => Some(MemberTarget {
            module: Some(*module),
            owner: Some(owner.clone()),
            name: "CONSTRUCTOR".into(),
        }),
        Type::Pointer { .. } => Some(MemberTarget {
            module: None,
            owner: None,
            name: "NEW".into(),
        }),
        _ => None,
    }
}
fn validate_type_reference(reference: &TypeReference) -> Result<(), Diagnostic> {
    let mut types = Vec::new();
    for alternative in &reference.alternatives {
        if alternative.name == "POINTER" && !valid_pointer_parts(&alternative.parts) {
            return Err(error(
                "INVALID_POINTER_TYPE",
                "POINTER must name a numeric or declared element type, optionally followed by [literal] or []",
                alternative.span,
            ));
        }
        let ty = type_from_atom(alternative);
        if matches!(ty, Type::System) {
            return Err(error(
                "NAME_NOT_FOUND",
                "SYSTEM was withdrawn in 0.2; use HOST.Args",
                alternative.span,
            ));
        }
        if types.contains(&ty) {
            return Err(error(
                "TYPE_MISMATCH",
                format!("duplicate alternative type '{}'", display(&ty)),
                alternative.span,
            ));
        }
        types.push(ty);
    }
    Ok(())
}

fn valid_pointer_parts(parts: &[String]) -> bool {
    if parts.len() < 2 || parts[0] != "TO" {
        return false;
    }
    let shape = parts
        .iter()
        .position(|part| part == "LeftBracket")
        .unwrap_or(parts.len());
    let element = pointer_element_type(&parts[..shape]);
    if !matches!(element, Type::Integer(_) | Type::Float(_) | Type::Named(_)) {
        return false;
    }
    match &parts[shape..] {
        [] => true,
        [open, close] => open == "LeftBracket" && close == "RightBracket",
        [open, length, close] => {
            open == "LeftBracket" && parse_integer(length).is_some() && close == "RightBracket"
        }
        _ => false,
    }
}
fn error(code: &'static str, message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic {
        code,
        message: message.into(),
        span,
    }
}
fn validate_returns(
    statements: &[Statement],
    return_type: &TypeReference,
    is_start: bool,
) -> Result<(), Diagnostic> {
    validate_return_statements(statements, return_type, is_start)?;
    let is_void = return_type
        .alternatives
        .first()
        .is_some_and(|atom| atom.name == "VOID");
    if !is_void && !guarantees_return(statements) {
        return Err(error(
            "MISSING_RETURN",
            "non-VOID FUNCTION can complete without RETURN expression",
            return_type.span,
        ));
    }
    Ok(())
}

fn validate_return_statements(
    statements: &[Statement],
    return_type: &TypeReference,
    is_start: bool,
) -> Result<(), Diagnostic> {
    let is_void = return_type
        .alternatives
        .first()
        .is_some_and(|atom| atom.name == "VOID");
    for statement in statements {
        match statement {
            Statement::Return {
                value: Some(value), ..
            } if is_start
                && integer_literal(value).is_some_and(|code| !(0..=255).contains(&code)) =>
            {
                return Err(error(
                    "INVALID_EXIT_CODE",
                    "Start return code must be in 0..255",
                    value.span,
                ));
            }
            Statement::Return { value, span } if is_void && value.is_some() => {
                return Err(error(
                    "TYPE_MISMATCH",
                    "VOID FUNCTION cannot return a value",
                    *span,
                ));
            }
            Statement::Return { value: None, span } if !is_void => {
                return Err(error(
                    "TYPE_MISMATCH",
                    "non-VOID FUNCTION must return a value",
                    *span,
                ));
            }
            Statement::If {
                branches,
                otherwise,
                ..
            } => {
                for branch in branches {
                    validate_return_statements(&branch.body.statements, return_type, is_start)?;
                }
                if let Some(otherwise) = otherwise {
                    validate_return_statements(&otherwise.statements, return_type, is_start)?;
                }
            }
            Statement::While { body, .. }
            | Statement::Repeat { body, .. }
            | Statement::For { body, .. } => {
                validate_return_statements(&body.statements, return_type, is_start)?;
            }
            _ => {}
        }
    }
    Ok(())
}
fn guarantees_return(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Return { .. } | Statement::Stop { .. } => true,
        Statement::If {
            branches,
            otherwise: Some(otherwise),
            ..
        } => {
            branches
                .iter()
                .all(|branch| guarantees_return(&branch.body.statements))
                && guarantees_return(&otherwise.statements)
        }
        _ => false,
    })
}
fn statement_span(statement: &Statement) -> Span {
    match statement {
        Statement::If { span, .. }
        | Statement::While { span, .. }
        | Statement::Repeat { span, .. }
        | Statement::For { span, .. }
        | Statement::Binding { span, .. }
        | Statement::Assignment { span, .. }
        | Statement::Return { span, .. }
        | Statement::Print { span, .. }
        | Statement::ClearScreen { span, .. }
        | Statement::Beep { span, .. }
        | Statement::Delete { span, .. }
        | Statement::Stop { span, .. }
        | Statement::Control { span, .. }
        | Statement::Call { span, .. }
        | Statement::MemberFunction { span, .. } => *span,
    }
}
fn validate_member_names(statements: &[Statement]) -> Result<(), Diagnostic> {
    let mut names = std::collections::HashSet::new();
    for statement in statements {
        let (name, span) = match statement {
            Statement::Binding { name, span, .. }
            | Statement::MemberFunction { name, span, .. } => (name, *span),
            _ => continue,
        };
        if !names.insert(name) {
            return Err(error(
                "DUPLICATE_NAME",
                format!("duplicate member '{name}'"),
                span,
            ));
        }
    }
    Ok(())
}
fn block_uses_self(statements: &[Statement]) -> bool {
    statements.iter().any(|statement| match statement {
        Statement::Assignment { target, value, .. } => {
            expression_uses_self(target) || expression_uses_self(value)
        }
        Statement::Return { value, .. } => value.as_ref().is_some_and(expression_uses_self),
        Statement::Print { values, .. } => values.iter().any(expression_uses_self),
        Statement::ClearScreen { console: value, .. }
        | Statement::Beep { console: value, .. }
        | Statement::Delete { value, .. }
        | Statement::Stop { code: value, .. }
        | Statement::Call {
            expression: value, ..
        } => expression_uses_self(value),
        Statement::Binding { initializer, .. } => {
            initializer.as_ref().is_some_and(expression_uses_self)
        }
        Statement::If {
            branches,
            otherwise,
            ..
        } => {
            branches.iter().any(|branch| {
                expression_uses_self(&branch.condition) || block_uses_self(&branch.body.statements)
            }) || otherwise
                .as_ref()
                .is_some_and(|block| block_uses_self(&block.statements))
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::Repeat {
            condition, body, ..
        } => expression_uses_self(condition) || block_uses_self(&body.statements),
        Statement::For { body, .. }
        | Statement::MemberFunction {
            body: Some(body), ..
        } => block_uses_self(&body.statements),
        _ => false,
    })
}
fn statement_uses_super(statement: &Statement) -> bool {
    match statement {
        Statement::Assignment { target, value, .. } => {
            expression_uses_super(target) || expression_uses_super(value)
        }
        Statement::Return { value, .. } => value.as_ref().is_some_and(expression_uses_super),
        Statement::Print { values, .. } => values.iter().any(expression_uses_super),
        Statement::ClearScreen { console, .. }
        | Statement::Beep { console, .. }
        | Statement::Delete { value: console, .. }
        | Statement::Stop { code: console, .. }
        | Statement::Call {
            expression: console,
            ..
        } => expression_uses_super(console),
        Statement::Binding { initializer, .. } => {
            initializer.as_ref().is_some_and(expression_uses_super)
        }
        Statement::If {
            branches,
            otherwise,
            ..
        } => {
            branches.iter().any(|branch| {
                expression_uses_super(&branch.condition)
                    || statement_block_uses_super(&branch.body.statements)
            }) || otherwise
                .as_ref()
                .is_some_and(|body| statement_block_uses_super(&body.statements))
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::Repeat {
            condition, body, ..
        } => expression_uses_super(condition) || statement_block_uses_super(&body.statements),
        Statement::For { body, .. }
        | Statement::MemberFunction {
            body: Some(body), ..
        } => statement_block_uses_super(&body.statements),
        _ => false,
    }
}
fn statement_has_invalid_super(statement: &Statement) -> bool {
    match statement {
        Statement::Assignment { target, value, .. } => {
            expression_has_invalid_super(target) || expression_has_invalid_super(value)
        }
        Statement::Return { value, .. } => value.as_ref().is_some_and(expression_has_invalid_super),
        Statement::Print { values, .. } => values.iter().any(expression_has_invalid_super),
        Statement::ClearScreen { console, .. }
        | Statement::Beep { console, .. }
        | Statement::Delete { value: console, .. }
        | Statement::Stop { code: console, .. }
        | Statement::Call {
            expression: console,
            ..
        } => expression_has_invalid_super(console),
        Statement::Binding { initializer, .. } => initializer
            .as_ref()
            .is_some_and(expression_has_invalid_super),
        Statement::If {
            branches,
            otherwise,
            ..
        } => {
            branches.iter().any(|branch| {
                expression_has_invalid_super(&branch.condition)
                    || branch
                        .body
                        .statements
                        .iter()
                        .any(statement_has_invalid_super)
            }) || otherwise
                .as_ref()
                .is_some_and(|body| body.statements.iter().any(statement_has_invalid_super))
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::Repeat {
            condition, body, ..
        } => {
            expression_has_invalid_super(condition)
                || body.statements.iter().any(statement_has_invalid_super)
        }
        Statement::For { body, .. }
        | Statement::MemberFunction {
            body: Some(body), ..
        } => body.statements.iter().any(statement_has_invalid_super),
        _ => false,
    }
}
fn statement_block_uses_super(statements: &[Statement]) -> bool {
    statements.iter().any(statement_uses_super)
}
fn expression_uses_self(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Name { name } => name == "SELF",
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { value: operand, .. }
        | ExpressionKind::Length { operand }
        | ExpressionKind::SizeOf { operand } => expression_uses_self(operand),
        ExpressionKind::Binary { left, right, .. } => {
            expression_uses_self(left) || expression_uses_self(right)
        }
        ExpressionKind::Call { callee, arguments } => {
            expression_uses_self(callee) || arguments.iter().any(expression_uses_self)
        }
        ExpressionKind::Member { object, .. } => expression_uses_self(object),
        ExpressionKind::Index { object, index } => {
            expression_uses_self(object) || expression_uses_self(index)
        }
        ExpressionKind::Vector { values }
        | ExpressionKind::New {
            arguments: values, ..
        } => values.iter().any(expression_uses_self),
        _ => false,
    }
}
fn expression_uses_super(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Super => true,
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { value: operand, .. }
        | ExpressionKind::Length { operand }
        | ExpressionKind::SizeOf { operand } => expression_uses_super(operand),
        ExpressionKind::Binary { left, right, .. } => {
            expression_uses_super(left) || expression_uses_super(right)
        }
        ExpressionKind::Call { callee, arguments } => {
            expression_uses_super(callee) || arguments.iter().any(expression_uses_super)
        }
        ExpressionKind::Member { object, .. } => expression_uses_super(object),
        ExpressionKind::Index { object, index } => {
            expression_uses_super(object) || expression_uses_super(index)
        }
        ExpressionKind::Vector { values }
        | ExpressionKind::New {
            arguments: values, ..
        } => values.iter().any(expression_uses_super),
        _ => false,
    }
}
fn expression_has_invalid_super(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Super => true,
        ExpressionKind::Call { callee, arguments }
            if matches!(callee.kind, ExpressionKind::Super)
                || matches!(callee.kind, ExpressionKind::Member { ref object, .. } if matches!(object.kind, ExpressionKind::Super)) =>
        {
            arguments.iter().any(expression_has_invalid_super)
        }
        ExpressionKind::Call { callee, arguments } => {
            expression_has_invalid_super(callee)
                || arguments.iter().any(expression_has_invalid_super)
        }
        ExpressionKind::Member { object, .. } => {
            matches!(object.kind, ExpressionKind::Super) || expression_has_invalid_super(object)
        }
        ExpressionKind::Unary { operand, .. }
        | ExpressionKind::Cast { value: operand, .. }
        | ExpressionKind::Length { operand }
        | ExpressionKind::SizeOf { operand } => expression_has_invalid_super(operand),
        ExpressionKind::Binary { left, right, .. } => {
            expression_has_invalid_super(left) || expression_has_invalid_super(right)
        }
        ExpressionKind::Index { object, index } => {
            expression_has_invalid_super(object) || expression_has_invalid_super(index)
        }
        ExpressionKind::Vector { values }
        | ExpressionKind::New {
            arguments: values, ..
        } => values.iter().any(expression_has_invalid_super),
        _ => false,
    }
}
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
