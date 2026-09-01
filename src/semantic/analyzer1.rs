#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    #[allow(clippy::too_many_lines)] // Global and standard namespaces share one declaration pass.
    pub(crate) fn declare_globals(&mut self, program: &Program) -> Result<(), Diagnostic> {
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
                        [host, capability] if host == "HOST" && capability == "Net" => {
                            Type::HostNet
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
                    let ty = signature.as_ref().map_or_else(
                        || declaration_type(*kind, name),
                        |signature| self.resolve_type(function_type(signature)),
                    );
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
        self.declare_standard_members();
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
}
