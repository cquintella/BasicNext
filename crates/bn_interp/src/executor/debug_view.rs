// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! What an interactive debugger sees at a pause (`bni dap`): the bindings
//! and values of the frame, with each object shown from the ARC core (class,
//! id, strong count; its fields and an `[arc]` child), and every live
//! object (proposal `arc-shared-core-0.6.5`, "Inspection").

#![allow(clippy::wildcard_imports)]
use super::ownership::id_of;
use super::*;

impl Executor<'_, '_> {
    /// The frame's bindings and values, and the core's live objects.
    pub(crate) fn debug_view(
        &self,
        symbols: &HashMap<SymbolId, Value>,
        values: &HashMap<ValueId, Value>,
    ) -> DebugView {
        let weak = self
            .ownership_frames
            .last()
            .map(|frame| &frame.weak_symbols);
        let mut variables = symbols
            .iter()
            .map(|(symbol, value)| {
                let is_weak = weak.is_some_and(|weak| weak.contains(symbol));
                self.debug_variable(format!("symbol#{}", symbol.value()), value, is_weak)
            })
            .collect::<Vec<_>>();
        variables.extend(values.iter().map(|(value_id, value)| {
            self.debug_variable(format!("value#{}", value_id.value()), value, false)
        }));
        variables.sort_by(|left, right| left.name.cmp(&right.name));
        let arc = self
            .arc
            .snapshot()
            .into_iter()
            .map(|object| DebugVariable {
                name: object.id.to_string(),
                value: format!("{} (strong {})", object.class, object.strong),
                children: Vec::new(),
            })
            .collect();
        DebugView { variables, arc }
    }

    /// One binding or value: an object (or a weak binding to one) is shown
    /// from the core, with its fields and an `[arc]` child; anything else as
    /// its value.
    fn debug_variable(&self, name: String, value: &Value, weak: bool) -> DebugVariable {
        let Value::Object { handle, class } = value else {
            return DebugVariable {
                name,
                value: format!("{value:?}"),
                children: Vec::new(),
            };
        };
        let id = id_of(*handle);
        let Some(info) = self.arc.info(id) else {
            return DebugVariable {
                name,
                value: if weak { "→ dead (NULL)" } else { "dead" }.into(),
                children: Vec::new(),
            };
        };
        let shown = format!("{}{}", info.class, info.id);
        let value = if weak {
            format!("→ {shown} (alive)")
        } else {
            format!("{shown} (strong {})", info.strong)
        };
        let mut children = self
            .module
            .field_layouts
            .get(class.as_ref())
            .zip(self.objects.get(*handle, 0, default_span()).ok())
            .map(|(layout, instance)| {
                layout
                    .fields
                    .iter()
                    .zip(instance.fields.iter())
                    .map(|(entry, field)| DebugVariable {
                        name: usize::try_from(entry.id.value())
                            .ok()
                            .and_then(|index| self.module.field_names.get(index))
                            .cloned()
                            .unwrap_or_default(),
                        value: self.debug_field(field),
                        children: Vec::new(),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        children.push(DebugVariable {
            name: "[arc]".into(),
            value: format!(
                "id {}, strong {}, generation {}",
                info.id,
                info.strong,
                info.id.generation()
            ),
            children: Vec::new(),
        });
        DebugVariable {
            name,
            value,
            children,
        }
    }

    /// A field's value, one level deep: an object shows as `Class#id`.
    fn debug_field(&self, field: &Value) -> String {
        match field {
            Value::Object { handle, .. } => self.arc.info(id_of(*handle)).map_or_else(
                || "dead".into(),
                |info| format!("{}{} (strong {})", info.class, info.id, info.strong),
            ),
            other => format!("{other:?}"),
        }
    }
}
