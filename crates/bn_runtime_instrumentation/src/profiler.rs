// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    collections::HashMap,
    fmt::Write,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

use serde_json::{Value as JsonValue, json};

use crate::{consumer::EventConsumer, context::EventContext, events::Event};

/// Execution statistics for a single function.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionStats {
    pub calls: usize,
    pub self_time_nanos: u128,
    pub cumulative_time_nanos: u128,
}

/// Event consumer measuring function execution times and invocation counts.
#[derive(Debug, Default)]
pub struct ProfilerConsumer {
    call_stack: Vec<(String, Instant, Duration)>, // (func, start_instant, children_duration)
    stats: Arc<Mutex<HashMap<String, FunctionStats>>>,
}

impl ProfilerConsumer {
    /// Creates a new profiler consumer with internal storage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            call_stack: Vec::new(),
            stats: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Access to the shared thread-safe statistics map.
    #[must_use]
    pub fn shared_stats(&self) -> Arc<Mutex<HashMap<String, FunctionStats>>> {
        Arc::clone(&self.stats)
    }

    /// Renders collected profile data as a formatted text table.
    #[must_use]
    pub fn summary_table(&self) -> String {
        let stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
        render_table(&stats)
    }

    /// Renders collected profile data as a JSON value.
    #[must_use]
    pub fn summary_json(&self) -> JsonValue {
        let stats = self.stats.lock().unwrap_or_else(PoisonError::into_inner);
        render_json(&stats)
    }
}

/// Formats a statistics map as a text table.
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn render_table<S: std::hash::BuildHasher>(
    stats: &HashMap<String, FunctionStats, S>,
) -> String {
    let mut rows = stats.iter().collect::<Vec<_>>();
    rows.sort_by_key(|a| std::cmp::Reverse(a.1.self_time_nanos));

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<24} {:>8} {:>14} {:>14}",
        "Function", "Calls", "Self (ms)", "Total (ms)"
    );
    let _ = writeln!(out, "{:-<64}", "");

    for (name, stat) in rows {
        let self_ms = stat.self_time_nanos as f64 / 1_000_000.0;
        let total_ms = stat.cumulative_time_nanos as f64 / 1_000_000.0;
        let _ = writeln!(
            out,
            "{:<24} {:>8} {:>14.3} {:>14.3}",
            name, stat.calls, self_ms, total_ms
        );
    }
    out
}

/// Formats a statistics map as a JSON value.
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn render_json<S: std::hash::BuildHasher>(
    stats: &HashMap<String, FunctionStats, S>,
) -> JsonValue {
    let mut functions = stats
        .iter()
        .map(|(name, stat)| {
            json!({
                "function": name,
                "calls": stat.calls,
                "self_time_nanos": stat.self_time_nanos,
                "cumulative_time_nanos": stat.cumulative_time_nanos,
                "self_time_ms": stat.self_time_nanos as f64 / 1_000_000.0,
                "cumulative_time_ms": stat.cumulative_time_nanos as f64 / 1_000_000.0,
            })
        })
        .collect::<Vec<_>>();
    functions.sort_by(|a, b| {
        let a_time = a
            .get("self_time_nanos")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0);
        let b_time = b
            .get("self_time_nanos")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0);
        b_time.cmp(&a_time)
    });
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "functions": functions,
    })
}

impl EventConsumer for ProfilerConsumer {
    fn on_event(&mut self, event: &Event, _ctx: &EventContext) {
        match event {
            Event::FunctionEntry { function, .. } => {
                self.call_stack
                    .push((function.clone(), Instant::now(), Duration::ZERO));
            }
            Event::FunctionReturn { function, .. } => {
                let Ok(mut stats) = self.stats.lock() else {
                    return;
                };
                if let Some((entry_func, start_time, children_duration)) = self.call_stack.pop() {
                    let total_elapsed = start_time.elapsed();
                    let self_time = total_elapsed.saturating_sub(children_duration);

                    // Add total_elapsed to parent's children_duration if exists
                    if let Some(parent) = self.call_stack.last_mut() {
                        parent.2 += total_elapsed;
                    }

                    let stat = stats.entry(entry_func).or_default();
                    stat.calls += 1;
                    stat.cumulative_time_nanos += total_elapsed.as_nanos();
                    stat.self_time_nanos += self_time.as_nanos();
                } else {
                    let stat = stats.entry(function.clone()).or_default();
                    stat.calls += 1;
                }
            }
            Event::Statement { .. }
            | Event::LineChange { .. }
            | Event::VariableWrite { .. }
            | Event::VariableRead { .. }
            | Event::ErrorThrown { .. }
            | Event::ErrorCaught { .. } => {}
        }
    }
}
