// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[allow(clippy::wildcard_imports)]
use super::*;

mod builder_state;
mod control_flow;
mod expressions;
mod statements;

impl<'a> Builder<'a> {
    pub(super) fn new(model: &'a SemanticModel, methods: HashSet<String>, prefix: &str) -> Self {
        Self {
            model,
            methods,
            prefix: prefix.into(),
            blocks: vec![OpenBlock {
                instructions: Vec::new(),
                terminator: None,
            }],
            current: BlockId(0),
            next_value: 0,
            loops: Vec::new(),
            receiver: None,
            derived_fields: None,
        }
    }

    pub(super) fn emit_void_call(&mut self, name: &str, arguments: Vec<ValueId>, span: Span) {
        if !self.methods.contains(name) {
            return;
        }
        let unused = self.value();
        let callee = self.function_constant(name, span);
        self.emit(Instruction::Call {
            destination: unused,
            callee,
            arguments,
            ty: Type::Named("VOID".into()),
            span,
        });
    }

    pub(super) fn emit_super_construction(
        &mut self,
        base: &str,
        receiver: ValueId,
        constructor_arguments: Vec<ValueId>,
        span: Span,
    ) {
        self.emit_void_call(
            &class_method_name(&self.prefix, base, "$fields"),
            vec![receiver],
            span,
        );
        let constructor = class_method_name(&self.prefix, base, "CONSTRUCTOR");
        if self.methods.contains(&constructor) {
            let mut arguments = vec![receiver];
            arguments.extend(constructor_arguments);
            self.emit_void_call(&constructor, arguments, span);
        }
    }

    pub(super) fn emit_derived_fields(&mut self, receiver: ValueId, span: Span) {
        if let Some(fields) = self.derived_fields.take() {
            self.emit_void_call(&fields, vec![receiver], span);
        }
    }

    pub(super) fn integer_one(&mut self, ty: Type, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Integer("1".into()),
            ty,
            span,
        });
        destination
    }

    pub(super) fn default_value(
        &mut self,
        ty: Type,
        type_ref: &TypeReference,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        if matches!(ty, Type::Pointer { .. } | Type::Function { .. }) {
            return Err(ir_error(
                "pointer and function bindings require an initializer",
                span,
            ));
        }
        if let Some(name) = self.struct_default_name(&ty) {
            let destination = self.value();
            let callee = self.function_constant(&name, span);
            self.emit(Instruction::Call {
                destination,
                callee,
                arguments: Vec::new(),
                ty,
                span,
            });
            return Ok(destination);
        }
        let destination = self.value();
        let mut dimensions = Vec::new();
        let mut dynamic_dimensions = Vec::new();
        if let Some(atom) = type_ref.alternatives.first() {
            for dimension in &atom.dimensions {
                match dimension {
                    crate::ast::VectorDimension::Literal { value, .. } => dimensions.push(
                        value
                            .parse::<usize>()
                            .map_err(|_| ir_error("invalid vector dimension", span))?,
                    ),
                    crate::ast::VectorDimension::Expression(expression) => {
                        dynamic_dimensions.push(self.expression(expression)?);
                    }
                }
            }
        }
        self.emit(Instruction::Default {
            destination,
            ty,
            dimensions,
            dynamic_dimensions,
            span,
        });
        Ok(destination)
    }

    pub(super) fn ensure_class(&mut self, class: &str, span: Span) {
        self.emit(Instruction::EnsureClass {
            class: class.into(),
            span,
        });
    }

    pub(super) fn function_constant(&mut self, name: &str, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Constant {
            destination,
            value: Constant::Function(name.into()),
            ty: Type::Unknown,
            span,
        });
        destination
    }

    pub(super) fn load(&mut self, symbol: SymbolId, ty: Type, span: Span) -> ValueId {
        let destination = self.value();
        self.emit(Instruction::Load {
            destination,
            symbol,
            ty,
            span,
        });
        destination
    }

    pub(super) fn expression_symbol(
        &self,
        expression: &Expression,
    ) -> Result<SymbolId, Diagnostic> {
        self.model
            .expression(expression.span)
            .and_then(|expression| expression.symbol_id)
            .ok_or_else(|| ir_error("expression has no resolved SymbolId", expression.span))
    }

    pub(super) fn assignment_place(
        &mut self,
        expression: &Expression,
    ) -> Result<AssignPlace, Diagnostic> {
        let mut indices = Vec::new();
        let mut current = expression;
        while let ExpressionKind::Index { object, index } = &current.kind {
            indices.push(self.expression(index)?);
            current = object;
        }
        indices.reverse();
        match &current.kind {
            ExpressionKind::Name { .. } => Ok(AssignPlace::Binding {
                symbol: self.expression_symbol(current)?,
                indices,
            }),
            ExpressionKind::Member { object, name } if indices.is_empty() => {
                let mut path = vec![name.clone()];
                let mut base = object.as_ref();
                while let ExpressionKind::Member {
                    object: nested,
                    name: nested_name,
                } = &base.kind
                {
                    path.push(nested_name.clone());
                    base = nested;
                }
                path.reverse();
                if let ExpressionKind::Name { .. } = &base.kind {
                    let object_type = type_at(self.model, base.span)?;
                    if is_namespace_type(&object_type) {
                        let class = self
                            .model
                            .expression(current.span)
                            .and_then(|resolved| resolved.member_target.as_ref())
                            .and_then(|target| target.owner.clone())
                            .map_or_else(
                                || static_class_name(&object_type, &self.prefix),
                                |owner| format!("{}{}", self.prefix, owner),
                            );
                        return Ok(AssignPlace::Static {
                            class,
                            field: path.last().cloned().unwrap_or_default(),
                        });
                    }
                    return Ok(AssignPlace::Field {
                        symbol: self.expression_symbol(base)?,
                        path,
                    });
                }
                let object_type = type_at(self.model, object.span)?;
                if is_namespace_type(&object_type) {
                    let class = self
                        .model
                        .expression(current.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .map_or_else(
                            || static_class_name(&object_type, &self.prefix),
                            |owner| format!("{}{}", self.prefix, owner),
                        );
                    Ok(AssignPlace::Static {
                        class,
                        field: name.clone(),
                    })
                } else {
                    let owner = self
                        .model
                        .expression(current.span)
                        .and_then(|resolved| resolved.member_target.as_ref())
                        .and_then(|target| target.owner.clone())
                        .unwrap_or_default();
                    Ok(AssignPlace::Member {
                        object: self.expression(object)?,
                        name: name.clone(),
                        owner,
                    })
                }
            }
            _ => Err(ir_error(
                "assignment target is not a binding, index, or member",
                expression.span,
            )),
        }
    }

    pub(super) fn struct_default_name(&self, ty: &Type) -> Option<String> {
        let name = match ty {
            Type::Named(name) => format!("{}{name}.$default", self.prefix),
            Type::ImportedNamed { module, name } => format!("#{}.{name}.$default", module.0),
            _ => return None,
        };
        self.methods.contains(&name).then_some(name)
    }

    pub(super) fn call_operands(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) -> Result<(ValueId, Vec<ValueId>), Diagnostic> {
        if let ExpressionKind::Name { name } = &callee.kind
            && matches!(name.as_str(), "ASC" | "CHAR")
        {
            let values = arguments
                .iter()
                .map(|argument| self.expression(argument))
                .collect::<Result<Vec<_>, _>>()?;
            return Ok((self.function_constant(name, callee.span), values));
        }
        if let ExpressionKind::Member { object, name } = &callee.kind {
            if matches!(object.kind, ExpressionKind::Super) {
                let owner = self
                    .model
                    .expression(callee.span)
                    .and_then(|resolved| resolved.member_target.as_ref())
                    .and_then(|target| target.owner.clone())
                    .ok_or_else(|| {
                        ir_error("SUPER member has no resolved base class", callee.span)
                    })?;
                let (receiver, receiver_type) = self
                    .receiver
                    .clone()
                    .ok_or_else(|| ir_error("SUPER member has no receiver", callee.span))?;
                let receiver = self.load(receiver, receiver_type, callee.span);
                let mut values = vec![receiver];
                for argument in arguments {
                    values.push(self.expression(argument)?);
                }
                return Ok((
                    self.function_constant(
                        &format!("@super:{}{}.{}", self.prefix, owner, name),
                        callee.span,
                    ),
                    values,
                ));
            }
            let object_type = type_at(self.model, object.span)?;
            let member_type = type_at(self.model, callee.span)?;
            if matches!(member_type, Type::Function { .. }) && !is_namespace_type(&object_type) {
                let receiver = self.expression(object)?;
                let owner = self
                    .model
                    .expression(callee.span)
                    .and_then(|resolved| resolved.member_target.as_ref())
                    .and_then(|target| target.owner.clone())
                    .unwrap_or_else(|| display_type(&object_type));
                let qualified = match &object_type {
                    Type::ImportedNamed {
                        module,
                        name: class,
                    } => format!("#{}.{class}.{name}", module.0),
                    _ => format!("{}{owner}.{name}", self.prefix),
                };
                let callee = self.function_constant(&qualified, callee.span);
                let mut values = vec![receiver];
                for argument in arguments {
                    values.push(self.expression(argument)?);
                }
                return Ok((callee, values));
            }
        }
        let callee = self.expression(callee)?;
        let mut values = Vec::new();
        for argument in arguments {
            values.push(self.expression(argument)?);
        }
        Ok((callee, values))
    }

    pub(super) fn allocate(
        &mut self,
        type_name: &str,
        arguments: &[Expression],
        ty: Type,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let arguments = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let destination = self.value();
        let class = class_ir_name(&ty, type_name, &self.prefix);
        if !is_numeric_type_name(type_name) {
            self.ensure_class(&class, span);
        }
        self.emit(Instruction::Allocate {
            destination,
            type_name: class.clone(),
            arguments: arguments.clone(),
            ty,
            span,
        });
        let constructor = format!("{class}.CONSTRUCTOR");
        if self.methods.contains(&constructor) {
            let callee = self.function_constant(&constructor, span);
            let mut constructor_arguments = vec![destination];
            constructor_arguments.extend(arguments);
            let unused = self.value();
            self.emit(Instruction::Call {
                destination: unused,
                callee,
                arguments: constructor_arguments,
                ty: Type::Named("VOID".into()),
                span,
            });
        } else {
            let fields = format!("{class}.$fields");
            if self.methods.contains(&fields) {
                let callee = self.function_constant(&fields, span);
                let unused = self.value();
                self.emit(Instruction::Call {
                    destination: unused,
                    callee,
                    arguments: vec![destination],
                    ty: Type::Named("VOID".into()),
                    span,
                });
            }
        }
        Ok(destination)
    }
}
