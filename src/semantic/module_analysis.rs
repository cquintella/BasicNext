#![allow(clippy::wildcard_imports)]
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze_with_modules(
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

pub(crate) fn exported_declarations(module: ModuleId, program: &Program) -> HashMap<String, Type> {
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
pub(crate) fn imported_type_catalog(
    graph: &ModuleGraph,
) -> HashMap<(ModuleId, String), ImportedTypeInfo> {
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

pub(crate) fn qualify_local_type(module: ModuleId, program: &Program, ty: Type) -> Type {
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

pub(crate) fn module_imports(
    module: &crate::module_graph::LoadedModule,
) -> HashMap<String, ModuleId> {
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

pub(crate) fn validate_implemented_interfaces(
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
