use std::collections::HashMap;

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
    BuiltinFunction(String),
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
    implementations: HashMap<String, Vec<String>>,
    imported_types: HashMap<(ModuleId, String), ImportedTypeInfo>,
    next_symbol: u32,
    next_type: u32,
    symbols: Vec<ResolvedSymbol>,
    expressions: Vec<ResolvedExpression>,
}

/// Resolves names and applies the currently implemented static semantic rules.
///
/// # Errors
///
/// Returns the first diagnostic found in source order.
pub fn analyze(program: &Program) -> Result<SemanticModel, Diagnostic> {
    analyze_with_modules(program, HashMap::new(), HashMap::new(), HashMap::new())
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
                matches!(item, Item::Import { path, .. } if path == &["HOST".to_string(), "main".to_string()])
            })
        {
            return Err(ModuleAnalysisError {
                module: module.id,
                diagnostic: error(
                    "HOST_IMPORT_SCOPE",
                    "only the executable module may import HOST.main",
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
        let model = analyze_with_modules(
            &module.program,
            exports.clone(),
            imported_modules,
            imported_types.clone(),
        )
        .map_err(|diagnostic| ModuleAnalysisError {
            module: module.id,
            diagnostic,
        })?;
        models.push(model);
    }
    Ok(models)
}

fn analyze_with_modules(
    program: &Program,
    module_exports: HashMap<ModuleId, HashMap<String, Type>>,
    module_imports: HashMap<String, ModuleId>,
    imported_types: HashMap<(ModuleId, String), ImportedTypeInfo>,
) -> Result<SemanticModel, Diagnostic> {
    let mut analyzer = Analyzer {
        globals: HashMap::new(),
        members: HashMap::new(),
        module_exports,
        module_imports,
        current_class: None,
        declaration_kinds: HashMap::new(),
        constructors: HashMap::new(),
        implementations: HashMap::new(),
        imported_types,
        next_symbol: 0,
        next_type: 0,
        symbols: Vec::new(),
        expressions: Vec::new(),
    };
    analyzer.declare_globals(program)?;
    validate_implemented_interfaces(program)?;
    analyzer.analyze_declarations(program)?;
    Ok(SemanticModel {
        symbols: analyzer.symbols,
        expressions: analyzer.expressions,
    })
}

fn exported_declarations(module: ModuleId, program: &Program) -> HashMap<String, Type> {
    program
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration {
                exported: true,
                name,
                kind,
                signature,
                ..
            } => Some((name.clone(), {
                let ty = signature
                    .as_ref()
                    .map_or_else(|| declaration_type(*kind, name), function_type);
                qualify_local_type(module, program, ty)
            })),
            _ => None,
        })
        .collect()
}

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

fn validate_implemented_interfaces(program: &Program) -> Result<(), Diagnostic> {
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
            interfaces: implemented,
            span,
            statements,
            ..
        } = item
        else {
            continue;
        };
        for interface in implemented {
            let Some(required) = interfaces.get(interface.as_str()) else {
                return Err(error(
                    "NAME_NOT_FOUND",
                    format!("interface '{interface}' is not declared"),
                    *span,
                ));
            };
            for (method, required_signature) in
                required.iter().filter_map(|statement| match statement {
                    Statement::MemberFunction {
                        name,
                        signature: Some(signature),
                        ..
                    } => Some((name, signature)),
                    _ => None,
                })
            {
                let implementation = statements.iter().find_map(|statement| match statement {
                    Statement::MemberFunction {
                        name,
                        visibility: Some(crate::ast::Visibility::Public),
                        is_static: false,
                        signature: Some(signature),
                        ..
                    } if name == method => Some(signature),
                    _ => None,
                });
                if implementation.is_none_or(|signature| {
                    function_type(signature) != function_type(required_signature)
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
        self.declare_global("Math", Type::TypeName("Math".into()), false, default_span())?;
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
                        [host, capability] if host == "HOST" && capability == "main" => {
                            Type::System
                        }
                        [host, capability] if host == "HOST" && capability == "clock" => {
                            Type::HostClock
                        }
                        [host, capability] if host == "HOST" => {
                            return Err(error(
                                "NAME_NOT_FOUND",
                                format!("HOST.{capability} is not a Basic Next 0.1 capability"),
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
            "SYSTEM".into(),
            HashMap::from([
                (
                    "ArgumentCount".into(),
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
                    "Argument".into(),
                    Member {
                        ty: Type::Function {
                            parameters: vec![Type::Integer(IntegerType::Int32)],
                            return_type: Box::new(Type::String),
                        },
                        is_static: false,
                        private: false,
                        mutable: false,
                    },
                ),
            ]),
        );
        self.members.insert(
            "HOST.clock".into(),
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
            "Float".into(),
            HashMap::from([(
                "TryParse".into(),
                Member {
                    ty: Type::Function {
                        parameters: vec![Type::String],
                        return_type: Box::new(Type::Alternative(vec![
                            Type::Float(FloatType::Float64),
                            Type::Named("Error".into()),
                        ])),
                    },
                    is_static: true,
                    private: false,
                    mutable: false,
                },
            )]),
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
        self.members.insert(
            "Math".into(),
            [
                "ABS",
                "MIN",
                "MAX",
                "SIGN",
                "FLOOR",
                "CEIL",
                "TRUNC",
                "ROUND",
                "EXP",
                "LOG",
                "LOG10",
                "LOG2",
                "POW",
                "SIN",
                "COS",
                "TAN",
                "ASIN",
                "ACOS",
                "ATAN",
                "ATAN2",
                "SQRT",
                "HYPOT",
                "FMA",
                "TOHOUR",
                "TOWEEKDAY",
                "TODATE",
                "TOTIME",
                "TOTIMESTAMP",
            ]
            .into_iter()
            .map(|name| {
                (
                    name.into(),
                    Member {
                        ty: Type::BuiltinFunction(name.into()),
                        is_static: true,
                        private: false,
                        mutable: false,
                    },
                )
            })
            .collect(),
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
        }
        Ok(())
    }

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
                    for branch in branches {
                        self.require_boolean(&branch.condition, &remaining_locals)?;
                        let mut branch_locals =
                            Self::narrowed_locals(&branch.condition, &remaining_locals, true)?;
                        self.block(
                            &branch.body.statements,
                            &mut branch_locals,
                            loops,
                            declaration_kind,
                            declaration_name,
                            return_type,
                        )?;
                        remaining_locals =
                            Self::narrowed_locals(&branch.condition, &remaining_locals, false)?;
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
                self.expression(iterable, locals)?;
                self.declare_local(
                    locals,
                    variable,
                    self.resolve_reference(type_ref),
                    false,
                    type_ref.span,
                )
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
                        format!("member '{}' is not an assignable field", name),
                        expression.span,
                    ));
                }
                Ok(ty)
            }
            ExpressionKind::Index { .. } => self.expression(expression, locals),
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
                    Self::validate_is_test(&left_type, right, expression.span)?;
                    Ok(Type::Boolean)
                } else {
                    let right_type = self.expression(right, locals)?;
                    binary_type(operator, &left_type, &right_type, expression)
                }
            }
            ExpressionKind::Call { callee, arguments } => {
                self.call(callee, arguments, locals, expression.span)
            }
            ExpressionKind::Member { object, name } => {
                let object_type = self.expression(object, locals)?;
                self.member_type(&object_type, name, expression.span)
            }
            ExpressionKind::Index { object, index } => {
                let object_type = self.expression(object, locals)?;
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
                    Type::Pointer { element, .. } => Ok(*element),
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
                self.expression(value, locals)?;
                Ok(self.resolve_reference(type_ref))
            }
        };
        let ty = result?;
        let symbol_id = match &expression.kind {
            ExpressionKind::Name { name } => self.lookup(name, locals).map(|symbol| symbol.id),
            _ => None,
        };
        self.record_expression(expression.span, &ty, symbol_id);
        if let ExpressionKind::Member { object, name } = &expression.kind {
            let object_type = self
                .expressions
                .iter()
                .find(|resolved| resolved.span == object.span)
                .map(|resolved| resolved.ty.clone());
            if let Some(target) = object_type.and_then(|ty| member_target(&ty, name))
                && let Some(resolved) = self
                    .expressions
                    .iter_mut()
                    .find(|resolved| resolved.span == expression.span)
            {
                resolved.member_target = Some(target);
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
            Type::HostClock => ("HOST.clock", false),
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
    fn validate_is_test(subject: &Type, test: &Expression, span: Span) -> Result<(), Diagnostic> {
        let test_type = is_test_type(test).ok_or_else(|| {
            error(
                "INVALID_ALTERNATIVE_USE",
                "IS requires a type, NULL, NA, EOF, NAN, or INF test",
                test.span,
            )
        })?;
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
        condition: &Expression,
        locals: &HashMap<String, Symbol>,
        truth: bool,
    ) -> Result<HashMap<String, Symbol>, Diagnostic> {
        if let ExpressionKind::Unary { operator, operand } = &condition.kind
            && operator == "NOT"
        {
            return Self::narrowed_locals(operand, locals, !truth);
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
            let left_locals = Self::narrowed_locals(left, locals, true)?;
            return Self::narrowed_locals(right, &left_locals, true);
        }
        if operator == "OR" {
            if truth {
                return Ok(locals.clone());
            }
            let left_locals = Self::narrowed_locals(left, locals, false)?;
            return Self::narrowed_locals(right, &left_locals, false);
        }
        if !matches!(operator.as_str(), "IS" | "Assign" | "NotEqual") {
            return Ok(locals.clone());
        }
        let ExpressionKind::Name { name } = &left.kind else {
            return Ok(locals.clone());
        };
        let test_type = if operator == "IS" {
            is_test_type(right).ok_or_else(|| {
                error(
                    "INVALID_ALTERNATIVE_USE",
                    "IS requires a valid type test",
                    right.span,
                )
            })?
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
    fn call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
        locals: &HashMap<String, Symbol>,
        span: Span,
    ) -> Result<Type, Diagnostic> {
        let callee_type = self.expression(callee, locals)?;
        if let Type::BuiltinFunction(name) = &callee_type {
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
        Ok(*return_type)
    }

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
        if arguments.len() != expected_count {
            return Err(error(
                "TYPE_MISMATCH",
                format!("Math.{name} expects {expected_count} argument(s)"),
                span,
            ));
        }
        let types = arguments
            .iter()
            .map(|argument| self.expression(argument, locals))
            .collect::<Result<Vec<_>, _>>()?;
        if matches!(name, "TOHOUR" | "TOWEEKDAY") {
            if !matches!(types[0], Type::Integer(IntegerType::Int64)) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("Math.{name} expects TIMESTAMP"),
                    span,
                ));
            }
            return Ok(Type::Integer(IntegerType::Int32));
        }
        if matches!(name, "TODATE" | "TOTIME") {
            if !matches!(types[0], Type::Integer(IntegerType::Int64)) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("Math.{name} expects TIMESTAMP"),
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
                    "Math.TOTIMESTAMP expects DATE and TIME",
                    span,
                ));
            }
            return Ok(Type::Integer(IntegerType::Int64));
        }
        if name == "ROUND" {
            if !is_float(&types[0]) || !is_integer(&types[1]) {
                return Err(error(
                    "TYPE_MISMATCH",
                    "Math.ROUND expects a floating value and an INTEGER digit count",
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
                        format!("Math.{name} requires compatible numeric arguments"),
                        span,
                    )
                })?;
            }
            if !is_numeric(&result) {
                return Err(error(
                    "TYPE_MISMATCH",
                    format!("Math.{name} requires numeric arguments"),
                    span,
                ));
            }
            return Ok(default_literal_type(result));
        }
        if types.iter().any(|ty| !is_float(ty)) {
            return Err(error(
                "TYPE_MISMATCH",
                format!("Math.{name} requires floating-point arguments"),
                span,
            ));
        }
        let result = types
            .into_iter()
            .reduce(|left, right| numeric_result(&left, &right).expect("floats are compatible"))
            .expect("Math functions have arguments");
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
            ) => expected_dimensions == actual_dimensions && self.compatible(expected, actual),
            _ => false,
        }
    }

    fn deletable(&self, ty: &Type) -> bool {
        match ty {
            Type::Pointer { .. } | Type::Null => true,
            Type::Named(name) => self.declaration_kinds.get(name) == Some(&DeclarationKind::Class),
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
        for atom in &reference.alternatives {
            let Type::Named(name) = type_from_atom(atom) else {
                continue;
            };
            if matches!(
                name.as_str(),
                "DATE" | "TIME" | "TIMEZONE" | "Error" | "VOID"
            ) || self.declaration_kinds.contains_key(&name)
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
        let element = atom
            .parts
            .iter()
            .skip_while(|part| part.as_str() != "TO")
            .nth(1)
            .map_or(Type::Unknown, |name| type_from_name(name));
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
    if dimensions.is_empty() {
        base
    } else {
        Type::Vector {
            element: Box::new(base),
            dimensions,
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
        ) => expected_dimensions == actual_dimensions && compatible(expected, actual),
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
            compatible(expected, actual)
                && pointer_lengths_compatible(*expected_length, *actual_length)
        }
        (expected, actual) => expected == actual,
    }
}
fn pointer_lengths_compatible(expected: PointerLength, actual: PointerLength) -> bool {
    match (expected, actual) {
        (PointerLength::One, PointerLength::One) => true,
        (PointerLength::Dynamic, PointerLength::Fixed(_) | PointerLength::Dynamic)
        | (PointerLength::Fixed(_), PointerLength::Dynamic) => true,
        (PointerLength::Fixed(expected), PointerLength::Fixed(actual)) => expected == actual,
        _ => false,
    }
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
        Type::HostClock => "HOST.clock".into(),
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
                .map(|dimension| format!("[{dimension}]"))
                .collect::<String>()
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
            owner: Some("HOST.clock".into()),
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
                "POINTER must be POINTER TO numeric-type, optionally followed by [literal] or []",
                alternative.span,
            ));
        }
        let ty = type_from_atom(alternative);
        if types.contains(&ty) {
            return Err(error(
                "TYPE_MISMATCH",
                format!("duplicate alternative type '{}'", display(&ty)),
                alternative.span,
            ));
        }
        if let Type::Pointer { element, .. } = &ty
            && !is_numeric(&element)
        {
            return Err(error(
                "INVALID_POINTER_TYPE",
                "POINTER element type must be numeric in Basic Next 0.1",
                alternative.span,
            ));
        }
        types.push(ty);
    }
    Ok(())
}

fn valid_pointer_parts(parts: &[String]) -> bool {
    if parts.len() < 2
        || parts[0] != "TO"
        || integer_type(&parts[1]).is_none() && float_type(&parts[1]).is_none()
    {
        return false;
    }
    match &parts[2..] {
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
        Statement::Return { value: Some(_), .. } | Statement::Stop { .. } => true,
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
        Statement::Delete { value, .. }
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
fn expression_uses_self(expression: &Expression) -> bool {
    match &expression.kind {
        ExpressionKind::Name { name } => name == "SELF",
        ExpressionKind::Unary { operand, .. } | ExpressionKind::Cast { value: operand, .. } => {
            expression_uses_self(operand)
        }
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
