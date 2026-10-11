#![allow(clippy::wildcard_imports)]
use super::*;

impl Analyzer {
    pub(crate) fn for_header(
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
                        return Err(type_mismatch(
                            "INTEGER",
                            "non-INTEGER value",
                            "counted FOR bound",
                            expression.span,
                        ));
                    }
                }
                if let Some(step) = step
                    && !is_integer(&self.expression(step, locals)?)
                {
                    return Err(type_mismatch(
                        "INTEGER",
                        "non-INTEGER value",
                        "FOR STEP",
                        step.span,
                    ));
                }
                if let Some(step) = step
                    && integer_literal(step) == Some(0)
                {
                    return Err(type_mismatch(
                        "non-zero INTEGER",
                        "0",
                        "FOR STEP",
                        step.span,
                    ));
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
                    return Err(type_mismatch(
                        "fixed-length vector",
                        display(&iterable_type),
                        "FOR EACH iterable",
                        iterable.span,
                    ));
                };
                if dimensions.contains(&u64::MAX) {
                    return Err(type_mismatch(
                        "fixed-length vector",
                        "dynamic-length vector",
                        "FOR EACH iterable",
                        iterable.span,
                    ));
                }
                let declared = self.resolve_reference(type_ref);
                if declared != *element {
                    return Err(type_mismatch(
                        display(&element),
                        display(&declared),
                        "FOR EACH binding",
                        type_ref.span,
                    ));
                }
                self.declare_local(locals, variable, declared, true, type_ref.span)
            }
        }
    }

    pub(crate) fn assignment_target(
        &mut self,
        expression: &Expression,
        locals: &mut HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        match &expression.kind {
            ExpressionKind::Name { name } => {
                let symbol = self
                    .lookup(name, locals)
                    .cloned()
                    .ok_or_else(|| undefined_name(name, "assignment target", expression.span))?;
                if symbol.constant {
                    let found = if symbol.parameter {
                        format!("parameter '{name}' (parameters are constant)")
                    } else {
                        format!("CONST '{name}'")
                    };
                    return Err(type_mismatch(
                        "mutable binding",
                        found,
                        "assignment target",
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
                    return Err(type_mismatch(
                        "assignable field",
                        format!("non-assignable member '{name}'"),
                        "member assignment",
                        expression.span,
                    ));
                }
                Ok(ty)
            }
            ExpressionKind::Index { object, .. } if matches!(object.kind, ExpressionKind::HostCapability { ref name } if name == "Args") => {
                Err(type_mismatch(
                    "mutable value",
                    "HOST.Args entry",
                    "HOST.Args assignment",
                    expression.span,
                ))
            }
            ExpressionKind::Index { object, .. } => {
                let object_type = self.expression(object, locals)?;
                if object_type == Type::String {
                    return Err(type_mismatch(
                        "mutable indexed value",
                        "STRING index",
                        "indexed assignment",
                        expression.span,
                    ));
                }
                self.expression(expression, locals)
            }
            _ => Err(type_mismatch(
                "mutable assignment target",
                "non-mutable value",
                "assignment",
                expression.span,
            )),
        }
    }
    pub(crate) fn require_boolean(
        &mut self,
        expression: &Expression,
        locals: &HashMap<String, Symbol>,
    ) -> Result<(), Diagnostic> {
        let ty = self.expression(expression, locals)?;
        if compatible(&Type::Boolean, &ty) {
            Ok(())
        } else {
            Err(type_mismatch(
                "BOOLEAN",
                display(&ty),
                "condition",
                expression.span,
            ))
        }
    }
    pub(crate) fn expression_as(
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
        if let (
            ExpressionKind::Vector { .. },
            Type::Vector {
                element,
                dimensions,
            },
        ) = (&expression.kind, expected)
            && let Some(shape) = vector_shape(expression)
            && shape.len() == dimensions.len()
        {
            return self.vector_literal_as(expression, element, &shape, locals);
        }
        self.expression(expression, locals)
    }

    /// A vector literal with a declared destination (0.6.md, "Vector literals
    /// take a declared destination"): each element is checked against the
    /// declared element type by the assignment rules, and the literal (with
    /// each nested row) takes that element type. A literal whose shape is not
    /// rectangular or has another rank is typed from its elements, so the
    /// caller reports it as before.
    fn vector_literal_as(
        &mut self,
        expression: &Expression,
        element: &Type,
        shape: &[usize],
        locals: &HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        let ty = Type::Vector {
            element: Box::new(element.clone()),
            dimensions: shape
                .iter()
                .map(|dimension| u64::try_from(*dimension).expect("usize fits u64"))
                .collect(),
        };
        if let ExpressionKind::Vector { values } = &expression.kind {
            for value in values {
                if shape.len() > 1 {
                    self.vector_literal_as(value, element, &shape[1..], locals)?;
                } else {
                    let actual = self.expression(value, locals)?;
                    if !self.compatible(element, &actual) {
                        return Err(type_mismatch(
                            display(element),
                            display(&actual),
                            VECTOR_LITERAL_ELEMENT,
                            value.span,
                        ));
                    }
                }
            }
        }
        self.record_expression(expression.span, &ty, None);
        Ok(ty)
    }
}
