// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::{Arc, Mutex};

use bn_frontend::{frontend_session::FrontendSession, prepare::prepare};
use bn_interp::{HostEnv, execute_validated_with_host_emitter};
use bn_runtime_instrumentation::{Event, EventConsumer, EventContext, EventEmitter};

#[derive(Clone, Debug, Default)]
struct RecordingConsumer {
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventConsumer for RecordingConsumer {
    fn on_event(&mut self, event: &Event, _ctx: &EventContext) {
        self.events.lock().unwrap().push(event.clone());
    }
}

struct BufferWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run_program_with_events(source: &str) -> (Result<u8, bn_diag::Diagnostic>, Vec<Event>) {
    let mut session = FrontendSession::default();
    let mut overlays = std::collections::BTreeMap::new();
    let path = std::path::PathBuf::from("test.bn");
    overlays.insert(path.clone(), source.to_string());
    let prepared = prepare("test.bn", &mut session, &overlays, &[]);
    let validated = prepared.expect("program compiles and validates").validated;

    let recorded = Arc::new(Mutex::new(Vec::new()));
    let consumer = RecordingConsumer {
        events: Arc::clone(&recorded),
    };

    let mut emitter = EventEmitter::new();
    emitter.add_consumer(Box::new(consumer));

    let mut input = std::io::Cursor::new(Vec::new());
    let mut output = Vec::new();
    let host = HostEnv::system(Vec::new());

    let result = execute_validated_with_host_emitter(
        &validated,
        &mut input,
        &mut output,
        &host,
        Some(&mut emitter),
    );

    let events = recorded.lock().unwrap().clone();
    (result, events)
}

#[test]
fn instrumentation_emits_function_lifecycle_and_statements() {
    let program = r"
FUNCTION Start() AS INTEGER
    LET x AS INTEGER = 42
    RETURN x
END FUNCTION
";

    let (result, events) = run_program_with_events(program);
    assert_eq!(result.expect("success"), 42);

    let has_entry = events
        .iter()
        .any(|e| matches!(e, Event::FunctionEntry { function, .. } if function == "Start"));
    let has_return = events
        .iter()
        .any(|e| matches!(e, Event::FunctionReturn { function, .. } if function == "Start"));
    let has_stmt = events.iter().any(|e| matches!(e, Event::Statement { .. }));
    let has_line = events.iter().any(|e| matches!(e, Event::LineChange { .. }));
    let has_write = events
        .iter()
        .any(|e| matches!(e, Event::VariableWrite { .. }));
    let has_read = events
        .iter()
        .any(|e| matches!(e, Event::VariableRead { .. }));

    assert!(has_entry, "FunctionEntry must be emitted");
    assert!(has_return, "FunctionReturn must be emitted");
    assert!(has_stmt, "Statement must be emitted");
    assert!(has_line, "LineChange must be emitted");
    assert!(has_write, "VariableWrite must be emitted");
    assert!(has_read, "VariableRead must be emitted");
}

#[test]
fn instrumentation_emits_nested_calls_with_correct_depth() {
    let program = r"
FUNCTION AddOne(n AS INTEGER) AS INTEGER
    RETURN n + 1
END FUNCTION

FUNCTION Start() AS INTEGER
    LET res AS INTEGER = AddOne(10)
    RETURN res
END FUNCTION
";

    let (result, events) = run_program_with_events(program);
    assert_eq!(result.expect("success"), 11);

    let start_entry = events
        .iter()
        .find(|e| matches!(e, Event::FunctionEntry { function, .. } if function == "Start"))
        .expect("Start entry");
    let add_entry = events
        .iter()
        .find(|e| matches!(e, Event::FunctionEntry { function, .. } if function == "AddOne"))
        .expect("AddOne entry");

    assert_eq!(start_entry.depth(), 0);
    assert_eq!(add_entry.depth(), 1);
}

#[test]
fn instrumentation_emits_error_thrown_on_runtime_failure() {
    let program = r"
FUNCTION Fail() AS INTEGER
    LET b AS INTEGER = 0
    RETURN 10 DIV b
END FUNCTION

FUNCTION Start() AS INTEGER
    RETURN Fail()
END FUNCTION
";

    let (result, events) = run_program_with_events(program);
    assert!(result.is_err(), "division by zero must fail");

    let has_error = events
        .iter()
        .any(|e| matches!(e, Event::ErrorThrown { .. }));
    assert!(has_error, "ErrorThrown must be emitted on runtime failure");
}

#[test]
fn event_json_serialization_is_valid() {
    let program = r"
FUNCTION Start() AS INTEGER
    RETURN 0
END FUNCTION
";

    let (result, events) = run_program_with_events(program);
    assert_eq!(result.expect("success"), 0);

    for event in &events {
        let json = event.to_json();
        assert!(json.is_object());
        assert!(json.get("type").is_some());
    }
}

#[test]
fn tracer_consumer_emits_ndjson_with_metadata() {
    let program = r"
FUNCTION Start() AS INTEGER
    LET total AS INTEGER = 100
    RETURN total
END FUNCTION
";
    let mut session = FrontendSession::default();
    let mut overlays = std::collections::BTreeMap::new();
    let path = std::path::PathBuf::from("test.bn");
    overlays.insert(path, program.to_string());
    let prepared = prepare("test.bn", &mut session, &overlays, &[]);
    let validated = prepared.expect("program compiles and validates").validated;

    let buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

    let tracer = bn_runtime_instrumentation::TracerConsumer::to_writer(
        Box::new(BufferWriter(std::sync::Arc::clone(&buffer))),
        "test.bn",
    )
    .expect("tracer initialized");

    let mut emitter = EventEmitter::new();
    emitter.add_consumer(Box::new(tracer));

    let mut input = std::io::Cursor::new(Vec::new());
    let mut output = Vec::new();
    let host = HostEnv::system(Vec::new());

    let result = execute_validated_with_host_emitter(
        &validated,
        &mut input,
        &mut output,
        &host,
        Some(&mut emitter),
    );
    assert_eq!(result.expect("success"), 100);

    let bytes = buffer.lock().unwrap().clone();
    let text = String::from_utf8(bytes).expect("valid utf8");
    let lines: Vec<&str> = text.lines().collect();

    assert!(!lines.is_empty(), "trace lines must not be empty");
    let first_line: serde_json::Value =
        serde_json::from_str(lines[0]).expect("first line valid json");
    assert_eq!(
        first_line.get("type").and_then(|v| v.as_str()),
        Some("Metadata")
    );
    assert_eq!(
        first_line.get("program").and_then(|v| v.as_str()),
        Some("test.bn")
    );
}

#[test]
fn profiler_consumer_measures_call_counts_and_timings() {
    let program = r"
FUNCTION Work() AS INTEGER
    RETURN 1
END FUNCTION

FUNCTION Start() AS INTEGER
    LET a AS INTEGER = Work()
    LET b AS INTEGER = Work()
    RETURN a + b
END FUNCTION
";
    let mut session = FrontendSession::default();
    let mut overlays = std::collections::BTreeMap::new();
    let path = std::path::PathBuf::from("test.bn");
    overlays.insert(path, program.to_string());
    let prepared = prepare("test.bn", &mut session, &overlays, &[]);
    let validated = prepared.expect("program compiles and validates").validated;

    let profiler = bn_runtime_instrumentation::ProfilerConsumer::new();
    let stats = profiler.shared_stats();

    let mut emitter = EventEmitter::new();
    emitter.add_consumer(Box::new(profiler));

    let mut input = std::io::Cursor::new(Vec::new());
    let mut output = Vec::new();
    let host = HostEnv::system(Vec::new());

    let result = execute_validated_with_host_emitter(
        &validated,
        &mut input,
        &mut output,
        &host,
        Some(&mut emitter),
    );
    assert_eq!(result.expect("success"), 2);

    let stats_map = stats.lock().unwrap();
    let work_stat = stats_map.get("Work").expect("Work was called");
    assert_eq!(work_stat.calls, 2);

    let start_stat = stats_map.get("Start").expect("Start was called");
    assert_eq!(start_stat.calls, 1);

    let table = bn_runtime_instrumentation::render_table(&stats_map);
    assert!(table.contains("Work"));
    assert!(table.contains("Start"));

    let json = bn_runtime_instrumentation::render_json(&stats_map);
    assert!(json.get("functions").is_some());
}
