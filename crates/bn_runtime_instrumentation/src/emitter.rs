// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;

use bn_source::Span;

use crate::{consumer::EventConsumer, context::EventContext, events::Event};

/// Dispatches runtime execution events to all registered consumers.
pub struct EventEmitter {
    consumers: Vec<Box<dyn EventConsumer>>,
    ctx: EventContext,
}

impl fmt::Debug for EventEmitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventEmitter")
            .field("consumer_count", &self.consumers.len())
            .field("ctx", &self.ctx)
            .finish()
    }
}

impl Default for EventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl EventEmitter {
    /// Creates a new, empty event emitter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            consumers: Vec::new(),
            ctx: EventContext::default(),
        }
    }

    /// Registers a new consumer.
    pub fn add_consumer(&mut self, consumer: Box<dyn EventConsumer>) {
        self.consumers.push(consumer);
    }

    /// Whether any consumers are registered.
    #[must_use]
    pub fn has_consumers(&self) -> bool {
        !self.consumers.is_empty()
    }

    /// Number of active consumers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.consumers.len()
    }

    /// Whether no consumers are active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.consumers.is_empty()
    }

    /// Dispatches an event to all consumers, updating the internal context.
    pub fn emit(&mut self, event: &Event) {
        if self.consumers.is_empty() {
            return;
        }

        match event {
            Event::FunctionEntry {
                function,
                depth,
                span,
                ..
            } => {
                self.ctx.current_function = Some(function.clone());
                self.ctx.depth = *depth;
                self.ctx.last_line = Some(span.start.line);
            }
            Event::FunctionReturn { depth, span, .. } => {
                self.ctx.depth = depth.saturating_sub(1);
                self.ctx.last_line = Some(span.end.line);
            }
            Event::LineChange {
                new_line, depth, ..
            } => {
                self.ctx.last_line = Some(*new_line);
                self.ctx.depth = *depth;
            }
            Event::Statement { depth, span, .. }
            | Event::VariableWrite { depth, span, .. }
            | Event::VariableRead { depth, span, .. }
            | Event::ErrorThrown { depth, span, .. }
            | Event::ErrorCaught { depth, span, .. } => {
                self.ctx.depth = *depth;
                self.ctx.last_line = Some(span.start.line);
            }
        }

        for consumer in &mut self.consumers {
            consumer.on_event(event, &self.ctx);
        }
    }

    /// Current ambient context.
    #[must_use]
    pub fn context(&self) -> &EventContext {
        &self.ctx
    }

    /// Emits a line change event if the line differs from the last emitted line.
    pub fn check_line_change(&mut self, span: Span, depth: usize) {
        let current_line = span.start.line;
        if self.ctx.last_line != Some(current_line) {
            self.emit(&Event::LineChange {
                new_line: current_line,
                depth,
                span,
            });
        }
    }
}
