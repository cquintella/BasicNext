// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

// Stores into a place below a `STRUCT` value that no single IR instruction
// reaches — `b[0].x`, `Keep.p.x`, `b[0].items[1]`, `SELF.p.x` (0.6.md,
// "Values and copies"; bucket 0.6.5 V.3). The place is resolved once, each
// index evaluated once; the `STRUCT` is copied into a hidden local, the store
// applies to that local, and the local is stored back into its own place.

#[allow(clippy::wildcard_imports)]
use super::*;

/// A resolved assignment place.
pub(super) enum Place<'e> {
    /// A place one IR store instruction reaches; `target` gives the types of
    /// the reads along it.
    Direct {
        place: AssignPlace,
        target: &'e Expression,
    },
    /// A field (and indices below it) of a `STRUCT` value held in `parent`.
    Through(Box<Through<'e>>),
}

pub(super) struct Through<'e> {
    parent: Place<'e>,
    parent_target: &'e Expression,
    owner: String,
    member: &'e Expression,
    field: String,
    /// Index values below the field, innermost first, with their expressions.
    indices: Vec<(ValueId, &'e Expression)>,
    target: &'e Expression,
}

/// `target` without its trailing index suffixes, and those suffixes,
/// innermost first: each index expression with the suffix it belongs to.
fn peel_indices(target: &Expression) -> (&Expression, Vec<(&Expression, &Expression)>) {
    let mut suffixes = Vec::new();
    let mut core = target;
    while let ExpressionKind::Index { object, index } = &core.kind {
        suffixes.push((index.as_ref(), core));
        core = object;
    }
    suffixes.reverse();
    (core, suffixes)
}

impl Builder<'_> {
    /// A fresh local the program cannot name, counted down from below
    /// `SYNTHETIC_SELF`; the analyzer numbers its symbols up from zero.
    fn hidden_local(&mut self) -> SymbolId {
        let symbol = SymbolId::from_raw(self.next_hidden_local);
        self.next_hidden_local -= 1;
        assert!(
            usize::try_from(self.next_hidden_local)
                .is_ok_and(|next| next > self.model.symbols.len())
        );
        symbol
    }

    /// Whether `object` is a field path the IR stores into directly: a
    /// binding, or fields below one (`line.a`, `box.p`).
    fn field_path_of_binding(&self, object: &Expression) -> Result<bool, Diagnostic> {
        match &object.kind {
            ExpressionKind::Name { .. } => {
                Ok(!is_namespace_type(&type_at(self.model, object.span)?))
            }
            ExpressionKind::Member { object, .. } => self.field_path_of_binding(object),
            _ => Ok(false),
        }
    }

    /// Resolves `target` once: a `Through` place when it stores below a
    /// `STRUCT` value that is not a field path of a binding, else the direct
    /// place.
    pub(super) fn resolve_place<'e>(
        &mut self,
        target: &'e Expression,
    ) -> Result<Place<'e>, Diagnostic> {
        let (core, index_nodes) = peel_indices(target);
        if let ExpressionKind::Member { object, name } = &core.kind {
            let object_type = type_at(self.model, object.span)?;
            if let Some(owner) = record_owner(&object_type, &self.prefix)
                && self.struct_default_name(&object_type).is_some()
                && !self.field_path_of_binding(object)?
            {
                let parent = self.resolve_place(object)?;
                let mut indices = Vec::with_capacity(index_nodes.len());
                for (index, node) in index_nodes {
                    indices.push((self.expression(index)?, node));
                }
                return Ok(Place::Through(Box::new(Through {
                    parent,
                    parent_target: object,
                    owner,
                    member: core,
                    field: name.clone(),
                    indices,
                    target,
                })));
            }
        }
        Ok(Place::Direct {
            place: self.assignment_place(target)?,
            target,
        })
    }

    /// Reads the value a resolved place holds, evaluating nothing again.
    fn read_place(&mut self, place: &Place<'_>) -> Result<ValueId, Diagnostic> {
        match place {
            Place::Direct { place, target } => self.read_direct(place, target),
            Place::Through(through) => {
                let parent = self.read_place(&through.parent)?;
                let mut value = self.read_member(parent, &through.field, through.member)?;
                for (index, node) in &through.indices {
                    value = self.read_index(value, *index, node)?;
                }
                Ok(value)
            }
        }
    }

    fn read_direct(
        &mut self,
        place: &AssignPlace,
        target: &Expression,
    ) -> Result<ValueId, Diagnostic> {
        let (core, index_nodes) = peel_indices(target);
        let span = target.span;
        let (mut value, indices) = match place {
            AssignPlace::Binding { symbol, indices } => {
                let ty = type_at(self.model, core.span)?;
                (self.load(*symbol, ty, span), indices.as_slice())
            }
            AssignPlace::Field { symbol, path, .. } => {
                (self.read_field_path(*symbol, path, core)?, &[][..])
            }
            AssignPlace::FieldIndex {
                symbol,
                path,
                indices,
                ..
            } => (
                self.read_field_path(*symbol, path, core)?,
                indices.as_slice(),
            ),
            AssignPlace::Static { class, field } => {
                (self.read_static(class, field, core)?, &[][..])
            }
            AssignPlace::StaticIndex {
                class,
                field,
                indices,
            } => (self.read_static(class, field, core)?, indices.as_slice()),
            AssignPlace::Member { object, name, .. } => {
                (self.read_member(*object, name, core)?, &[][..])
            }
            AssignPlace::MemberIndex {
                object,
                name,
                indices,
                ..
            } => (self.read_member(*object, name, core)?, indices.as_slice()),
        };
        for (index, (_, node)) in indices.iter().copied().zip(index_nodes) {
            value = self.read_index(value, index, node)?;
        }
        Ok(value)
    }

    /// Loads `symbol` and reads the fields of `path` below it; `core` is the
    /// member expression of the last field.
    fn read_field_path(
        &mut self,
        symbol: SymbolId,
        path: &[String],
        core: &Expression,
    ) -> Result<ValueId, Diagnostic> {
        let mut members = Vec::with_capacity(path.len());
        let mut node = core;
        while let ExpressionKind::Member { object, .. } = &node.kind {
            members.push(node);
            node = object;
        }
        members.reverse();
        let mut value = self.load(symbol, type_at(self.model, node.span)?, core.span);
        for (name, member) in path.iter().zip(members) {
            value = self.read_member(value, name, member)?;
        }
        Ok(value)
    }

    fn read_static(
        &mut self,
        class: &str,
        field: &str,
        member: &Expression,
    ) -> Result<ValueId, Diagnostic> {
        self.ensure_class(class, member.span);
        let destination = self.value();
        self.emit(Instruction::LoadStatic {
            destination,
            class: class.to_owned(),
            field: field.to_owned(),
            ty: type_at(self.model, member.span)?,
            span: member.span,
        });
        Ok(destination)
    }

    fn read_member(
        &mut self,
        object: ValueId,
        name: &str,
        member: &Expression,
    ) -> Result<ValueId, Diagnostic> {
        let destination = self.value();
        self.emit(Instruction::Member {
            destination,
            object,
            field: None,
            name: name.to_owned(),
            owner: self.resolved_member_owner(member),
            ty: type_at(self.model, member.span)?,
            span: member.span,
        });
        Ok(destination)
    }

    fn read_index(
        &mut self,
        object: ValueId,
        index: ValueId,
        node: &Expression,
    ) -> Result<ValueId, Diagnostic> {
        let destination = self.value();
        self.emit(Instruction::Index {
            destination,
            object,
            index,
            ty: type_at(self.model, node.span)?,
            span: node.span,
        });
        Ok(destination)
    }

    /// Stores `value` (of type `ty`) into a resolved place, under the
    /// ownership protocol: an owned value goes in, and the previous content
    /// comes out and is released.
    pub(super) fn assign_place(
        &mut self,
        place: Place<'_>,
        value: ValueId,
        ty: &Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        match place {
            Place::Direct { place, target } => {
                let weak_target = matches!(&place, AssignPlace::Binding { symbol, .. } if self.weak_locals.contains(symbol))
                    || self.is_weak_field(target);
                let (value, previous) = if self.is_arc(ty) && !weak_target {
                    (self.owned_value(value, span), Some(self.value()))
                } else {
                    (value, None)
                };
                self.write_place(place, value, previous, ty, span);
                if let Some(previous) = previous {
                    self.emit(Instruction::Release {
                        value: previous,
                        destructor: None,
                        span,
                    });
                }
                Ok(())
            }
            Place::Through(through) => self.assign_through(*through, value, ty, span),
        }
    }

    /// Copies the parent `STRUCT` into a hidden local, stores into the local,
    /// and stores the local back into the parent's place.
    fn assign_through(
        &mut self,
        through: Through<'_>,
        value: ValueId,
        ty: &Type,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let parent_type = type_at(self.model, through.parent_target.span)?;
        let local = self.hidden_local();
        let mut current = self.read_place(&through.parent)?;
        if self.is_arc(&parent_type) {
            current = self.owned_value(current, span);
            self.declare_owner(local, parent_type.clone(), span);
        }
        self.emit(Instruction::Store {
            previous: None,
            symbol: local,
            value: current,
            ty: parent_type.clone(),
            span,
        });
        let path = vec![through.field];
        let inner = if through.indices.is_empty() {
            AssignPlace::Field {
                symbol: local,
                root_owner: through.owner,
                path,
            }
        } else {
            AssignPlace::FieldIndex {
                symbol: local,
                root_owner: through.owner,
                path,
                indices: through.indices.iter().map(|(index, _)| *index).collect(),
            }
        };
        self.assign_place(
            Place::Direct {
                place: inner,
                target: through.target,
            },
            value,
            ty,
            span,
        )?;
        let updated = self.load(local, parent_type.clone(), span);
        self.assign_place(through.parent, updated, &parent_type, span)
    }
}
