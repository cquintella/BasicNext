#![allow(clippy::wildcard_imports)]
use super::*;
#[path = "lowering_callable.rs"]
mod lowering_callable;
use lowering_callable::*;

pub(crate) fn module_prefix(root: ModuleId, id: ModuleId) -> String {
    if id == root {
        String::new()
    } else {
        format!("#{}.", id.0)
    }
}

pub(crate) fn class_method_name(prefix: &str, class: &str, method: &str) -> String {
    if class.starts_with('#') {
        format!("{class}.{method}")
    } else {
        format!("{prefix}{class}.{method}")
    }
}

pub(crate) fn collect_methods(program: &Program, prefix: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &program.items {
        let Item::Declaration {
            kind,
            name,
            base_class,
            statements,
            ..
        } = item
        else {
            continue;
        };
        match kind {
            DeclarationKind::Class => {
                for statement in statements {
                    if let Statement::MemberFunction { name: method, .. } = statement {
                        names.insert(format!("{prefix}{name}.{method}"));
                    }
                }
                names.insert(format!("{prefix}{name}.$init"));
                names.insert(format!("{prefix}{name}.$fields"));
                if base_class.is_some() {
                    names.insert(format!("{prefix}{name}.CONSTRUCTOR"));
                    names.insert(format!("{prefix}{name}.DESTRUCTOR"));
                }
            }
            DeclarationKind::Struct => {
                names.insert(format!("{prefix}{name}.$default"));
            }
            DeclarationKind::Function => {
                names.insert(format!("{prefix}{name}"));
            }
            DeclarationKind::Interface => {}
        }
    }
    names
}

#[allow(clippy::too_many_lines)] // Class lowering keeps construction order explicit.
pub(crate) fn lower_program(
    program: &Program,
    model: &SemanticModel,
    prefix: &str,
    method_names: &HashSet<String>,
) -> Result<Vec<Function>, Diagnostic> {
    let mut functions = Vec::new();
    for item in &program.items {
        let Item::Declaration {
            kind,
            name: type_name,
            base_class,
            statements,
            span,
            ..
        } = item
        else {
            continue;
        };
        match kind {
            DeclarationKind::Class => {
                let resolved_base = model.base_classes.get(type_name).cloned();
                if let Some(init) =
                    lower_static_init(model, prefix, type_name, statements, *span, method_names)?
                {
                    functions.push(init);
                }
                functions.push(lower_instance_fields(
                    model,
                    prefix,
                    type_name,
                    statements,
                    *span,
                    method_names,
                )?);
                for statement in statements {
                    let Statement::MemberFunction {
                        name,
                        is_static,
                        parameters,
                        signature,
                        body: Some(body),
                        span,
                        ..
                    } = statement
                    else {
                        continue;
                    };
                    functions.push(lower_callable(
                        model,
                        &format!("{prefix}{type_name}.{name}"),
                        false,
                        signature.as_ref(),
                        parameters,
                        &body.statements,
                        *span,
                        (!*is_static).then_some(body.span),
                        (name == "CONSTRUCTOR")
                            .then(|| resolved_base.clone())
                            .flatten()
                            .filter(|_| {
                                !body.statements.first().is_some_and(|statement| {
                                    matches!(statement, Statement::Call { expression, .. } if matches!(expression.kind, ExpressionKind::Call { ref callee, .. } if matches!(callee.kind, ExpressionKind::Super)))
                                })
                            }),
                        (name == "DESTRUCTOR")
                            .then(|| resolved_base.clone())
                            .flatten()
                            .filter(|base| method_names.contains(&class_method_name(prefix, base, "DESTRUCTOR"))),
                        method_names.clone(),
                        prefix,
                    )?);
                }
                if base_class.is_some()
                    && !statements.iter().any(|statement| {
                        matches!(statement, Statement::MemberFunction { name, .. } if name == "CONSTRUCTOR")
                    })
                {
                    functions.push(lower_inherited_constructor(
                        prefix,
                        type_name,
                        resolved_base.as_deref().expect("base class exists"),
                        *span,
                        method_names,
                    )?);
                }
                if base_class.is_some()
                    && !statements.iter().any(|statement| {
                        matches!(statement, Statement::MemberFunction { name, .. } if name == "DESTRUCTOR")
                    })
                {
                    functions.push(lower_inherited_destructor(
                        prefix,
                        type_name,
                        resolved_base.as_deref().expect("base class exists"),
                        *span,
                        method_names,
                    )?);
                }
            }
            DeclarationKind::Struct => {
                functions.push(lower_struct_default(
                    model,
                    prefix,
                    type_name,
                    statements,
                    *span,
                    method_names,
                )?);
            }
            _ => {}
        }
    }
    for item in &program.items {
        let Item::Declaration {
            kind: DeclarationKind::Function,
            name,
            signature: Some(signature),
            asynchronous,
            statements,
            span,
            ..
        } = item
        else {
            continue;
        };
        functions.push(lower_callable(
            model,
            &format!("{prefix}{name}"),
            *asynchronous,
            Some(signature),
            &signature.parameters,
            statements,
            *span,
            None,
            None,
            None,
            method_names.clone(),
            prefix,
        )?);
    }
    Ok(functions)
}

pub(crate) fn lower_static_init(
    model: &SemanticModel,
    prefix: &str,
    class_name: &str,
    statements: &[Statement],
    span: Span,
    method_names: &HashSet<String>,
) -> Result<Option<Function>, Diagnostic> {
    let statics: Vec<_> = statements
        .iter()
        .filter(|statement| {
            matches!(
                statement,
                Statement::Binding {
                    is_static: true,
                    ..
                }
            )
        })
        .collect();
    if statics.is_empty() {
        return Ok(None);
    }
    let mut builder = Builder::new(model, method_names.clone(), prefix);
    let class = format!("{prefix}{class_name}");
    for statement in statics {
        let Statement::Binding {
            name,
            initializer,
            type_ref,
            span,
            ..
        } = statement
        else {
            continue;
        };
        let ty = type_at(model, *span).unwrap_or_else(|_| named_or_void(type_ref));
        let value = if let Some(initializer) = initializer {
            builder.expression(initializer)?
        } else {
            builder.default_value(ty.clone(), type_ref, *span)?
        };
        builder.emit(Instruction::StoreStatic {
            class: class.clone(),
            field: name.clone(),
            value,
            ty,
            span: *span,
        });
    }
    if !builder.terminated() {
        builder.terminate(Terminator::Return { value: None });
    }
    Ok(Some(Function {
        name: format!("{class}.$init"),
        asynchronous: false,
        parameters: Vec::new(),
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    }))
}

const SYNTHETIC_SELF: SymbolId = {
    // $fields has no AST SELF binding; runtime maps this parameter to the instance.
    SymbolId::from_raw(u32::MAX)
};

pub(crate) fn lower_instance_fields(
    model: &SemanticModel,
    prefix: &str,
    class_name: &str,
    statements: &[Statement],
    span: Span,
    method_names: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let mut builder = Builder::new(model, method_names.clone(), prefix);
    builder.receiver = Some((SYNTHETIC_SELF, Type::Named(class_name.into())));
    let receiver = builder.load(SYNTHETIC_SELF, Type::Named(class_name.into()), span);
    emit_field_inits(&mut builder, model, statements, receiver, class_name)?;
    if !builder.terminated() {
        builder.terminate(Terminator::Return { value: None });
    }
    Ok(Function {
        name: format!("{prefix}{class_name}.$fields"),
        asynchronous: false,
        parameters: vec![SYNTHETIC_SELF],
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

pub(crate) fn lower_inherited_constructor(
    prefix: &str,
    class_name: &str,
    base_class: &str,
    span: Span,
    methods: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let empty_model = SemanticModel {
        symbols: Vec::new(),
        expressions: Vec::new(),
        layouts: std::collections::HashMap::new(),
        base_classes: std::collections::HashMap::new(),
        bnmath_modules: HashSet::new(),
        module_constants: std::collections::HashMap::new(),
    };
    let mut builder = Builder::new(&empty_model, methods.clone(), prefix);
    builder.derived_fields = Some(format!("{prefix}{class_name}.$fields"));
    let receiver = builder.load(SYNTHETIC_SELF, Type::Named(class_name.into()), span);
    builder.emit_super_construction(base_class, receiver, Vec::new(), span);
    builder.emit_derived_fields(receiver, span);
    builder.terminate(Terminator::Return { value: None });
    Ok(Function {
        name: format!("{prefix}{class_name}.CONSTRUCTOR"),
        asynchronous: false,
        parameters: vec![SYNTHETIC_SELF],
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

pub(crate) fn lower_inherited_destructor(
    prefix: &str,
    class_name: &str,
    base_class: &str,
    span: Span,
    methods: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let empty_model = SemanticModel {
        symbols: Vec::new(),
        expressions: Vec::new(),
        layouts: std::collections::HashMap::new(),
        base_classes: std::collections::HashMap::new(),
        bnmath_modules: HashSet::new(),
        module_constants: std::collections::HashMap::new(),
    };
    let mut builder = Builder::new(&empty_model, methods.clone(), prefix);
    let receiver = builder.load(SYNTHETIC_SELF, Type::Named(class_name.into()), span);
    let callee = builder.function_constant(
        &format!(
            "@super:{}",
            class_method_name(prefix, base_class, "DESTRUCTOR")
        ),
        span,
    );
    let destination = builder.value();
    builder.emit(Instruction::Call {
        destination,
        callee,
        arguments: vec![receiver],
        ty: Type::Named("VOID".into()),
        span,
    });
    builder.terminate(Terminator::Return { value: None });
    Ok(Function {
        name: format!("{prefix}{class_name}.DESTRUCTOR"),
        asynchronous: false,
        parameters: vec![SYNTHETIC_SELF],
        return_type: Type::Named("VOID".into()),
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

pub(crate) fn lower_struct_default(
    model: &SemanticModel,
    prefix: &str,
    struct_name: &str,
    statements: &[Statement],
    span: Span,
    method_names: &HashSet<String>,
) -> Result<Function, Diagnostic> {
    let mut builder = Builder::new(model, method_names.clone(), prefix);
    let ty = Type::Named(struct_name.into());
    let record = builder.value();
    builder.emit(Instruction::Default {
        destination: record,
        ty: ty.clone(),
        dimensions: Vec::new(),
        dynamic_dimensions: Vec::new(),
        span,
    });
    emit_field_inits(&mut builder, model, statements, record, struct_name)?;
    if !builder.terminated() {
        builder.terminate(Terminator::Return {
            value: Some(record),
        });
    }
    Ok(Function {
        name: format!("{prefix}{struct_name}.$default"),
        asynchronous: false,
        parameters: Vec::new(),
        return_type: ty,
        entry: BlockId(0),
        blocks: builder.finish()?,
        span,
    })
}

pub(crate) fn emit_field_inits(
    builder: &mut Builder<'_>,
    model: &SemanticModel,
    statements: &[Statement],
    object: ValueId,
    owner: &str,
) -> Result<(), Diagnostic> {
    for statement in statements {
        let Statement::Binding {
            name,
            initializer,
            type_ref,
            is_static: false,
            span,
            ..
        } = statement
        else {
            continue;
        };
        let ty = type_at(model, *span).unwrap_or_else(|_| named_or_void(type_ref));
        let value = if let Some(initializer) = initializer {
            builder.expression(initializer)?
        } else {
            builder.default_value(ty.clone(), type_ref, *span)?
        };
        builder.emit(Instruction::SetMember {
            object,
            name: name.clone(),
            owner: owner.into(),
            value,
            ty,
            span: *span,
        });
    }
    Ok(())
}
