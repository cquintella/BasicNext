#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn declare_bases(&mut self, program: &Program) -> Result<(), Diagnostic> {
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

    pub(crate) fn inherit_members(&mut self) -> Result<(), Diagnostic> {
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

    pub(crate) fn validate_super_constructors(&self, program: &Program) -> Result<(), Diagnostic> {
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
    #[allow(clippy::too_many_lines)] // Class members, constructors, and SELF share one pass.
    pub(crate) fn analyze_declarations(&mut self, program: &Program) -> Result<(), Diagnostic> {
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
}
