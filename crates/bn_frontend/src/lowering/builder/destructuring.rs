// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Lowering of destructuring `LET` and positional `STRUCT` literals.
//! Both become ordinary values: a binding is a `Store`, a literal is the
//! struct's default value with every field overwritten.

#![allow(clippy::wildcard_imports)]
use super::*;

/// A vector dimension written `T[n]` or `T[]`: the length is not a literal.
const RUNTIME_DIMENSION: u64 = u64::MAX;

impl Builder<'_> {
    pub(super) fn destructure(
        &mut self,
        bindings: &[DestructuringBinding],
        source: &Expression,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let source_type = type_at(self.model, source.span)?;
        let mut value = self.expression(source)?;
        if self.is_arc(&source_type) {
            value = self.owned_value(value, span);
            let local = self.hidden_local();
            self.declare_owner(local, source_type.clone(), span);
            self.emit(Instruction::Store {
                previous: None,
                symbol: local,
                value,
                ty: source_type.clone(),
                span,
            });
            value = self.load(local, source_type.clone(), span);
        }
        if Self::region_length_is_runtime(&source_type) {
            let element = Self::region_element_type(&source_type)?;
            self.reject_wrong_length(value, bindings.len(), element, span)?;
        }
        let fields = self.model.struct_fields_of(&source_type);
        for (index, binding) in bindings.iter().enumerate() {
            let binding_type = type_at(self.model, binding.span)?;
            let received = if let Some(fields) = fields {
                let field = fields
                    .iter()
                    .find(|field| field.name == binding.name)
                    .ok_or_else(|| {
                        ir_error(
                            bn_catalog::text(
                                "frontend",
                                "msg.destructuring_field_was_not_resolved",
                            ),
                            binding.span,
                        )
                    })?;
                self.read_struct_field(
                    value,
                    &source_type,
                    &field.name,
                    field.ty.clone(),
                    binding.span,
                )?
            } else {
                let element = Self::region_element_type(&source_type)?;
                self.read_region_index(value, index, element, binding.span)?
            };
            let received_type = self
                .value_types
                .get(&received)
                .cloned()
                .unwrap_or(binding_type.clone());
            let received = self.cast_to(received, &received_type, &binding_type, binding.span);
            self.store_binding(binding, received, &binding_type)?;
        }
        Ok(())
    }

    /// `STRUCT` literal `[...]` where the declared type is `ty`.
    pub(super) fn struct_literal(
        &mut self,
        expression: &Expression,
        values: &[Expression],
        ty: &Type,
    ) -> Result<ValueId, Diagnostic> {
        let fields = self
            .model
            .struct_fields_of(ty)
            .ok_or_else(|| {
                ir_error(
                    bn_catalog::text("frontend", "msg.struct_literal_has_no_field_list"),
                    expression.span,
                )
            })?
            .to_vec();
        let function = self.struct_default_name(ty).ok_or_else(|| {
            ir_error(
                bn_catalog::text("frontend", "msg.struct_literal_has_no_default_value"),
                expression.span,
            )
        })?;
        let record = self.value();
        let callee = self.function_constant(&function, expression.span);
        self.emit(Instruction::Call {
            destination: record,
            callee,
            arguments: Vec::new(),
            ty: ty.clone(),
            span: expression.span,
        });
        let owner = struct_value_owner(ty, &self.prefix).ok_or_else(|| {
            ir_error(
                bn_catalog::text("frontend", "msg.struct_literal_has_no_owner"),
                expression.span,
            )
        })?;
        let owner = self.emitted_member_owner(owner);
        for (value, field) in values.iter().zip(fields) {
            let element = self.expression(value)?;
            let element_type = self
                .value_types
                .get(&element)
                .cloned()
                .unwrap_or_else(|| field.ty.clone());
            let element = self.cast_to(element, &element_type, &field.ty, value.span);
            // A STRUCT field is copied by the witness, which destroys the
            // previous contents itself and does not define `previous`.
            // Defining one and releasing it reads a value LLVM never assigned.
            let field_is_struct = self.struct_default_name(&field.ty).is_some();
            let (stored, previous) = if self.is_arc(&field.ty) && !field_is_struct {
                (self.owned_value(element, value.span), Some(self.value()))
            } else if field_is_struct {
                (self.owned_value(element, value.span), None)
            } else {
                (element, None)
            };
            self.emit(Instruction::SetMember {
                previous,
                object: record,
                field: None,
                name: field.name,
                owner: owner.clone(),
                value: stored,
                ty: field.ty.clone(),
                span: value.span,
            });
            if let Some(previous) = previous {
                self.emit(Instruction::Release {
                    value: previous,
                    destructor: None,
                    span: value.span,
                });
            }
        }
        Ok(record)
    }

    fn region_length_is_runtime(ty: &Type) -> bool {
        match ty {
            Type::Vector { dimensions, .. } => dimensions.first() == Some(&RUNTIME_DIMENSION),
            Type::Pointer {
                length: PointerLength::Dynamic,
                ..
            } => true,
            _ => false,
        }
    }

    fn region_element_type(ty: &Type) -> Result<Type, Diagnostic> {
        match ty {
            Type::Vector {
                element,
                dimensions,
            } if dimensions.len() > 1 => Ok(Type::Vector {
                element: element.clone(),
                dimensions: dimensions[1..].to_vec(),
            }),
            Type::Vector { element, .. } | Type::Pointer { element, .. } => Ok((**element).clone()),
            _ => Err(ir_error(
                bn_catalog::text(
                    "frontend",
                    "msg.destructuring_source_is_not_a_vector_or_pointer",
                ),
                default_span(),
            )),
        }
    }

    /// When the length is known only at run time, a different length stops
    /// with `INDEX_OUT_OF_BOUNDS`. The failing path indexes at `LEN`, which
    /// is never a valid index, so both backends raise that diagnostic.
    fn reject_wrong_length(
        &mut self,
        source: ValueId,
        bindings: usize,
        element: Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let length = self.value();
        self.emit(Instruction::Length {
            destination: length,
            vector: source,
            span,
        });
        let expected = self.value();
        let count = i64::try_from(bindings).map_err(|_| {
            ir_error(
                bn_catalog::text("frontend", "msg.destructuring_binding_count_does_not_fit"),
                span,
            )
        })?;
        self.emit(Instruction::Constant {
            destination: expected,
            value: Constant::Integer(count.to_string()),
            ty: Type::Integer(IntegerType::Int32),
            span,
        });
        let differs = self.value();
        self.emit(Instruction::Binary {
            destination: differs,
            operator: "NotEqual".into(),
            left: length,
            right: expected,
            ty: Type::Boolean,
            span,
        });
        let fail = self.block();
        let ok = self.block();
        self.terminate(Terminator::Branch {
            condition: differs,
            then_block: fail,
            else_block: ok,
        });
        self.current = fail;
        let out_of_range = self.value();
        self.emit(Instruction::Index {
            destination: out_of_range,
            object: source,
            index: length,
            ty: element,
            span,
        });
        self.jump_if_open(ok);
        self.current = ok;
        Ok(())
    }

    fn read_region_index(
        &mut self,
        source: ValueId,
        index: usize,
        ty: Type,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let index_value = self.value();
        let position = i64::try_from(index).map_err(|_| {
            ir_error(
                bn_catalog::text("frontend", "msg.destructuring_index_does_not_fit"),
                span,
            )
        })?;
        self.emit(Instruction::Constant {
            destination: index_value,
            value: Constant::Integer(position.to_string()),
            ty: Type::Integer(IntegerType::Int32),
            span,
        });
        let destination = self.value();
        self.emit(Instruction::Index {
            destination,
            object: source,
            index: index_value,
            ty,
            span,
        });
        Ok(destination)
    }

    fn read_struct_field(
        &mut self,
        source: ValueId,
        source_type: &Type,
        name: &str,
        ty: Type,
        span: Span,
    ) -> Result<ValueId, Diagnostic> {
        let owner = struct_value_owner(source_type, &self.prefix).ok_or_else(|| {
            ir_error(
                bn_catalog::text("frontend", "msg.destructuring_struct_has_no_owner"),
                span,
            )
        })?;
        let owner = self.emitted_member_owner(owner);
        let destination = self.value();
        self.emit(Instruction::Member {
            destination,
            object: source,
            field: None,
            name: name.into(),
            owner,
            ty,
            span,
        });
        Ok(destination)
    }

    fn cast_to(&mut self, value: ValueId, from: &Type, to: &Type, span: Span) -> ValueId {
        if from == to {
            return value;
        }
        let destination = self.value();
        self.emit(Instruction::Cast {
            destination,
            value,
            ty: to.clone(),
            span,
        });
        destination
    }

    fn store_binding(
        &mut self,
        binding: &DestructuringBinding,
        value: ValueId,
        ty: &Type,
    ) -> Result<(), Diagnostic> {
        let symbol = self.symbol(binding.span)?;
        let weak = binding.type_ref.as_ref().is_some_and(|type_ref| {
            type_ref
                .alternatives
                .first()
                .is_some_and(|atom| atom.name == "WEAK")
        });
        let value = if self.is_arc(ty) && !weak {
            let value = self.owned_value(value, binding.span);
            self.declare_owner(symbol, ty.clone(), binding.span);
            value
        } else {
            value
        };
        self.emit(Instruction::Store {
            previous: None,
            symbol,
            value,
            ty: ty.clone(),
            span: binding.span,
        });
        Ok(())
    }
}

fn struct_value_owner(ty: &Type, prefix: &str) -> Option<String> {
    match ty {
        Type::Named(name) => Some(qualified_class_name(prefix, name)),
        Type::ImportedNamed { module, name } => Some(format!("#{}.{name}", module.0)),
        _ => None,
    }
}
