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
                            return Err(integer_out_of_range(
                                kind,
                                &value.to_string(),
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
                return Err(value_mismatch(
                    &declared,
                    &element,
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
                self.reject_parameter_component(object, expression, locals)?;
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
                self.reject_parameter_component(object, expression, locals)?;
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
    /// A write into `container` (a field or element store) that lands inside a
    /// parameter's own value is `TYPE_MISMATCH` (0.6.md, "Parameter
    /// immutability and place mutation"): the path from the parameter to the
    /// target crosses only value types (`STRUCT`, fixed or open vectors). A
    /// path through an object or a pointer region writes to the heap and stays
    /// allowed.
    fn reject_parameter_component(
        &mut self,
        container: &Expression,
        target: &Expression,
        locals: &HashMap<String, Symbol>,
    ) -> Result<(), Diagnostic> {
        match self.parameter_owning(container, locals)? {
            Some(name) => Err(type_mismatch(
                bn_catalog::text("frontend", "msg.mutable_binding"),
                bn_catalog::fill("frontend", "msg.parameter_component", &[("name", &name)]),
                bn_catalog::text("frontend", "msg.assignment_target"),
                target.span,
            )),
            None => Ok(()),
        }
    }

    /// The parameter whose value storage holds `container`, if every step from
    /// the parameter to it is a value type.
    fn parameter_owning(
        &mut self,
        container: &Expression,
        locals: &HashMap<String, Symbol>,
    ) -> Result<Option<String>, Diagnostic> {
        let container_type = self.expression(container, locals)?;
        let is_value = matches!(container_type, Type::Vector { .. })
            || self.struct_field_list(&container_type).is_some();
        if !is_value {
            return Ok(None);
        }
        match &container.kind {
            ExpressionKind::Name { name } => Ok(self
                .lookup(name, locals)
                .filter(|symbol| symbol.parameter)
                .map(|_| name.clone())),
            ExpressionKind::Member { object, .. } | ExpressionKind::Index { object, .. } => {
                self.parameter_owning(object, locals)
            }
            _ => Ok(None),
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
        if float32_destination(expected) {
            check_float_literal(
                expression,
                FloatType::Float32,
                bn_catalog::text("frontend", "msg.float32_value"),
            )?;
        }
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
            && let Some(shape) = self.declared_vector_shape(expression, element, dimensions.len())
            && shape.len() == dimensions.len()
        {
            return self.vector_literal_as(expression, element, &shape, locals);
        }
        self.expression(expression, locals)
    }

    /// The shape of a vector literal for a declared vector of `rank`
    /// dimensions: for `STRUCT` elements only the declared dimensions are
    /// measured, because the brackets below them are `STRUCT` literals.
    fn declared_vector_shape(
        &self,
        expression: &Expression,
        element: &Type,
        rank: usize,
    ) -> Option<Vec<usize>> {
        if self.struct_field_list(element).is_some() {
            vector_shape_to_depth(expression, rank)
        } else {
            vector_shape(expression)
        }
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
                    // An element is an assignment to the element type, so a
                    // `STRUCT` element may be a bracket literal (0.6.md,
                    // "STRUCT literals").
                    let actual = self.expression_as(value, element, locals)?;
                    if !self.compatible(element, &actual) {
                        return Err(value_mismatch(
                            element,
                            &actual,
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
                return Err(value_mismatch(
                    &field.ty,
                    &actual,
                    bn_catalog::text("frontend", "msg.struct_literal_element"),
                    value.span,
                ));
            }
        }
        self.record_expression(expression.span, expected, None);
        Ok(expected.clone())
    }
}
