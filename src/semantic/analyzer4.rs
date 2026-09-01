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

    pub(crate) fn assignment_target(
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
    pub(crate) fn require_boolean(
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
        self.expression(expression, locals)
    }
}
