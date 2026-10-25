// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    collections::{BTreeSet, HashMap},
    sync::{Arc, Mutex},
};

use bn_source::Span;

use crate::{consumer::EventConsumer, context::EventContext, events::Event};

/// Interactive execution stepping mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepMode {
    Next(usize),
    In,
    Out(usize),
}

/// Call frame on the debugger's event-driven stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackFrame {
    pub function: String,
    pub depth: usize,
    pub span: Span,
}

/// State of an interactive debugging session.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Default)]
pub struct DebuggerState {
    pub paused: bool,
    pub pause_requested: bool,
    pub step: Option<StepMode>,
    pub stop_on_entry: bool,
    pub started: bool,
    pub current_line: usize,
    pub current_depth: usize,
    pub current_function: String,
    pub call_stack: Vec<StackFrame>,
    pub pause_reason: Option<String>,
}

/// Event consumer managing breakpoints, stepping, and call stack tracking.
#[derive(Debug)]
pub struct DebuggerConsumer {
    file_path: String,
    breakpoints: Arc<Mutex<HashMap<String, BTreeSet<usize>>>>,
    state: Arc<Mutex<DebuggerState>>,
}

impl DebuggerConsumer {
    /// Creates a new debugger consumer bound to a file and shared state.
    #[must_use]
    pub fn new(
        file_path: impl Into<String>,
        breakpoints: Arc<Mutex<HashMap<String, BTreeSet<usize>>>>,
        state: Arc<Mutex<DebuggerState>>,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            breakpoints,
            state,
        }
    }

    /// Access to the shared debugger state.
    #[must_use]
    pub fn state(&self) -> Arc<Mutex<DebuggerState>> {
        Arc::clone(&self.state)
    }

    /// Access to the breakpoints registry.
    #[must_use]
    pub fn breakpoints(&self) -> Arc<Mutex<HashMap<String, BTreeSet<usize>>>> {
        Arc::clone(&self.breakpoints)
    }
}

impl EventConsumer for DebuggerConsumer {
    fn on_event(&mut self, event: &Event, _ctx: &EventContext) {
        let Ok(mut state) = self.state.lock() else {
            return;
        };

        match event {
            Event::FunctionEntry {
                function,
                depth,
                span,
                ..
            } => {
                state.current_function.clone_from(function);
                state.current_depth = *depth;
                state.current_line = span.start.line;
                state.call_stack.push(StackFrame {
                    function: function.clone(),
                    depth: *depth,
                    span: *span,
                });

                if state.stop_on_entry && !state.started {
                    state.started = true;
                    state.paused = true;
                    state.pause_reason = Some("entry".into());
                }
            }
            Event::FunctionReturn { .. } => {
                state.call_stack.pop();
                if let Some(top) = state.call_stack.last() {
                    let function = top.function.clone();
                    let depth = top.depth;
                    let line = top.span.start.line;
                    state.current_function = function;
                    state.current_depth = depth;
                    state.current_line = line;
                }
            }
            Event::LineChange {
                new_line,
                depth,
                span,
            } => {
                let prev_line = state.current_line;
                let prev_depth = state.current_depth;
                state.current_line = *new_line;
                state.current_depth = *depth;

                if let Some(top) = state.call_stack.last_mut() {
                    top.span = *span;
                }

                let at_breakpoint = self
                    .breakpoints
                    .lock()
                    .ok()
                    .and_then(|reg| reg.get(&self.file_path).cloned())
                    .is_some_and(|lines| lines.contains(new_line));

                let step_pause = match state.step {
                    Some(StepMode::Next(target)) => {
                        (*depth < target) || (*depth <= target && *new_line != prev_line)
                    }
                    Some(StepMode::In) => *new_line != prev_line || *depth > prev_depth,
                    Some(StepMode::Out(target)) => {
                        *depth <= target && (*depth < prev_depth || *new_line != prev_line)
                    }
                    None => false,
                };

                let pause_requested = state.pause_requested;

                if at_breakpoint || step_pause || pause_requested {
                    state.started = true;
                    state.step = None;
                    state.pause_requested = false;
                    state.paused = true;
                    state.pause_reason = Some(if at_breakpoint {
                        "breakpoint".into()
                    } else if pause_requested {
                        "pause".into()
                    } else {
                        "step".into()
                    });
                }
            }
            Event::Statement { .. }
            | Event::VariableWrite { .. }
            | Event::VariableRead { .. }
            | Event::ErrorThrown { .. }
            | Event::ErrorCaught { .. } => {}
        }
    }
}
