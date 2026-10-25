// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Runtime execution observability, event instrumentation, debugger, tracer and profiler.

pub mod consumer;
pub mod context;
pub mod debugger;
pub mod emitter;
pub mod events;
pub mod profiler;
pub mod tracer;

pub use consumer::EventConsumer;
pub use context::EventContext;
pub use debugger::{DebuggerConsumer, DebuggerState, StackFrame, StepMode};
pub use emitter::EventEmitter;
pub use events::Event;
pub use profiler::{FunctionStats, ProfilerConsumer, render_json, render_table};
pub use tracer::TracerConsumer;
