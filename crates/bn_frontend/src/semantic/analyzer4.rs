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
                variable_span,
                start,
                end,
                step,
            } => {
                let declared = match type_ref {
                    Some(type_ref) => self.resolve_reference(type_ref),
                    None => Type::Integer(IntegerType::Int32),
                };
                let Type::Integer(kind) = declared else {
                    return Err(type_mismatch(
                        "INTEGER",
                        display(&declared),
                        bn_catalog::text("frontend", "msg.counted_for_variable"),
                        type_ref
                            .as_ref()
                            .map_or(*variable_span, |type_ref| type_ref.span),
                    ));
                };
                for expression in [start, end] {
                    let bound = self.expression(expression, locals)?;
                    if !is_integer(&bound) {
                        return Err(type_mismatch(
                            "INTEGER",
                            display(&bound),
                            bn_catalog::text("frontend", "msg.counted_for_bound"),
                            expression.span,
                        ));
                    }
                    if let Some(value) = constant_integer(expression) {
                        let (minimum, maximum) = integer_range(kind);
                        if !(minimum..=maximum).contains(&value) {
                            return Err(type_mismatch(
                                display(&declared),
                                value.to_string(),
                                bn_catalog::text("frontend", "msg.counted_for_bound"),
                                expression.span,
                            ));
                        }
                    }
                }
                if let Some(step) = step {
                    let step_type = self.expression(step, locals)?;
                    if !is_integer(&step_type) {
                        return Err(type_mismatch(
                            "INTEGER",
                            display(&step_type),
                            "FOR STEP",
                            step.span,
                        ));
                    }
                    if integer_literal(step) == Some(0) {
                        return Err(type_mismatch(
                            "non-zero INTEGER",
                            "0",
                            "FOR STEP",
                            step.span,
                        ));
                    }
                }
                self.declare_local(
                    locals,
                    variable,
                    declared,
                    false,
                    type_ref
                        .as_ref()
                        .map_or(*variable_span, |type_ref| type_ref.span),
                )
            }
            ForHeader::Each {
                variable,
                type_ref,
                variable_span,
                iterable,
            } => self.each_header(
                variable,
                type_ref.as_ref(),
                *variable_span,
                iterable,
                locals,
            ),
        }
    }

    /// `FOR EACH`: the iterable is a STRING or a fixed vector, and an explicit
    /// `AS` must accept that element.
    fn each_header(
        &mut self,
        variable: &str,
        type_ref: Option<&TypeReference>,
        variable_span: Span,
        iterable: &Expression,
        locals: &mut HashMap<String, Symbol>,
    ) -> Result<(), Diagnostic> {
        let iterable_type = self.expression(iterable, locals)?;
        let element = match &iterable_type {
            Type::String => Type::String,
            Type::Vector {
                element,
                dimensions,
            } => {
                if dimensions.contains(&u64::MAX) {
                    return Err(type_mismatch(
                        bn_catalog::text("frontend", "msg.fixed_length_vector_or_string"),
                        "dynamic-length vector",
                        "FOR EACH iterable",
                        iterable.span,
                    ));
                }
                (**element).clone()
            }
            _ => {
                return Err(type_mismatch(
                    bn_catalog::text("frontend", "msg.fixed_length_vector_or_string"),
                    display(&iterable_type),
                    "FOR EACH iterable",
                    iterable.span,
                ));
            }
        };
        let (declared, span) = if let Some(type_ref) = type_ref {
            let declared = self.resolve_reference(type_ref);
            if !self.compatible(&declared, &element) {
                return Err(type_mismatch(
                    display(&declared),
                    display(&element),
                    "FOR EACH binding",
                    type_ref.span,
                ));
            }
            (declared, type_ref.span)
        } else {
            (element, variable_span)
        };
        self.declare_local(locals, variable, declared, true, span)
    }

    /// Checks a write target and returns the type it stores (a local's
    /// declared type). It leaves the target's narrowing alone: the value is
    /// analyzed first, and the caller ends the narrowing after the write
    /// (0.6.md, "`IS` narrowing": "from that point").
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
        if let ExpressionKind::Vector { values } = &expression.kind
            && let Some(fields) = self.struct_field_list(expected)
        {
            return self.struct_literal_as(expression, values, expected, &fields, locals);
        }
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

    /// A bracket literal whose declared destination is a `STRUCT` (0.6.md,
    /// "STRUCT literals"): element `i` is an assignment to field `i`.
    fn struct_literal_as(
        &mut self,
        expression: &Expression,
        values: &[Expression],
        expected: &Type,
        fields: &[RecordMember],
        locals: &HashMap<String, Symbol>,
    ) -> Result<Type, Diagnostic> {
        if values.len() != fields.len() {
            return Err(type_mismatch(
                bn_catalog::fill(
                    "frontend",
                    "msg.struct_with_fields",
                    &[("count", &fields.len())],
                ),
                bn_catalog::fill(
                    "frontend",
                    "msg.literal_with_elements",
                    &[("count", &values.len())],
                ),
                bn_catalog::text("frontend", "msg.struct_literal"),
                expression.span,
            ));
        }
        for (value, field) in values.iter().zip(fields) {
            let actual = self.expression_as(value, &field.ty, locals)?;
            if !self.compatible(&field.ty, &actual) {
                return Err(type_mismatch(
                    display(&field.ty),
                    display(&actual),
                    bn_catalog::text("frontend", "msg.struct_literal_element"),
                    value.span,
                ));
            }
        }
        self.record_expression(expression.span, expected, None);
        Ok(expected.clone())
    }
}
