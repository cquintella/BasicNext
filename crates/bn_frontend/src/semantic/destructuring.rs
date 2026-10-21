// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Destructuring `LET` (0.6.md, "Destructuring LET and STRUCT literals").
//! The source is typed before any binding is declared, so the names are out
//! of scope in it. Each binding then becomes an ordinary local.

#![allow(clippy::wildcard_imports)]
use super::*;

/// A vector dimension written `T[n]` or `T[]`: the length is not a literal.
const RUNTIME_DIMENSION: u64 = u64::MAX;

impl Analyzer {
    pub(crate) fn destructure(
        &mut self,
        bindings: &[DestructuringBinding],
        source: &Expression,
        locals: &mut HashMap<String, Symbol>,
        declaration_kind: DeclarationKind,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if declaration_kind != DeclarationKind::Function {
            return Err(type_mismatch(
                bn_catalog::text("frontend", "msg.statement_in_a_function"),
                bn_catalog::text("frontend", "msg.destructuring_let"),
                bn_catalog::text("frontend", "msg.struct_or_class_body"),
                span,
            ));
        }
        let source_type = self.expression(source, locals)?;
        let received = self.destructured_values(&source_type, bindings, source.span)?;
        for (binding, value_type) in bindings.iter().zip(received) {
            let ty = self.binding_type(binding, &value_type)?;
            self.declare_local(locals, &binding.name, ty, false, binding.span)?;
        }
        Ok(())
    }

    /// The type each binding receives, in binding order. A `STRUCT` looks
    /// the field up by name; a vector or pointer region uses position.
    fn destructured_values(
        &self,
        source_type: &Type,
        bindings: &[DestructuringBinding],
        span: Span,
    ) -> Result<Vec<Type>, Diagnostic> {
        if let Some(fields) = self.struct_field_list(source_type) {
            let mut values = Vec::with_capacity(bindings.len());
            for binding in bindings {
                let field = fields
                    .iter()
                    .find(|field| field.name == binding.name)
                    .ok_or_else(|| {
                        name_not_found(
                            &binding.name,
                            bn_catalog::fill(
                                "frontend",
                                "msg.the_fields_of",
                                &[("type", &display(source_type))],
                            ),
                            binding.span,
                        )
                    })?;
                values.push(field.ty.clone());
            }
            return Ok(values);
        }
        let (element, length_known) = Self::region_element(source_type, span)?;
        if let Some(length) = length_known
            && length != u64::try_from(bindings.len()).unwrap_or(u64::MAX)
        {
            return Err(type_mismatch(
                bn_catalog::fill("frontend", "msg.vector_of_length", &[("length", &length)]),
                format!("{} bindings", bindings.len()),
                bn_catalog::text("frontend", "msg.destructuring_let"),
                span,
            ));
        }
        Ok(vec![element; bindings.len()])
    }

    /// `(element type, compile-time length)`. `None` for the length means the
    /// program checks it at run time.
    fn region_element(source_type: &Type, span: Span) -> Result<(Type, Option<u64>), Diagnostic> {
        match source_type {
            Type::Vector {
                element,
                dimensions,
            } => {
                let Some(first) = dimensions.first() else {
                    return Err(not_a_region(source_type, span));
                };
                let row = if dimensions.len() > 1 {
                    Type::Vector {
                        element: element.clone(),
                        dimensions: dimensions[1..].to_vec(),
                    }
                } else {
                    (**element).clone()
                };
                let length = (*first != RUNTIME_DIMENSION).then_some(*first);
                Ok((row, length))
            }
            Type::Pointer { element, length } => match length {
                PointerLength::Fixed(length) => Ok(((**element).clone(), Some(*length))),
                PointerLength::Dynamic => Ok(((**element).clone(), None)),
                PointerLength::One => Err(not_a_region(source_type, span)),
            },
            _ => Err(not_a_region(source_type, span)),
        }
    }
}

fn not_a_region(source_type: &Type, span: Span) -> Diagnostic {
    type_mismatch(
        bn_catalog::text("frontend", "msg.vector_or_struct"),
        display(source_type),
        bn_catalog::text("frontend", "msg.destructuring_source"),
        span,
    )
}

impl Analyzer {
    fn binding_type(
        &mut self,
        binding: &DestructuringBinding,
        received: &Type,
    ) -> Result<Type, Diagnostic> {
        let Some(type_ref) = &binding.type_ref else {
            return Ok(received.clone());
        };
        self.validate_type_reference(type_ref, false)?;
        let declared = self.resolve_reference(type_ref);
        if !self.compatible(&declared, received) {
            return Err(type_mismatch(
                display(&declared),
                display(received),
                bn_catalog::text("frontend", "msg.destructuring_binding"),
                binding.span,
            ));
        }
        Ok(declared)
    }

    /// Declaration-order fields when `ty` is a `STRUCT`.
    pub(crate) fn struct_field_list(&self, ty: &Type) -> Option<Vec<RecordMember>> {
        match ty {
            Type::Named(name)
                if self.declaration_kinds.get(name) == Some(&DeclarationKind::Struct) =>
            {
                self.struct_fields.get(name).cloned()
            }
            Type::ImportedNamed { module, name } => {
                let info = self.imported_types.get(&(*module, name.clone()))?;
                (info.kind == DeclarationKind::Struct).then(|| info.field_order.clone())
            }
            _ => None,
        }
    }
}
