use std::{
    collections::{BTreeSet, HashMap},
    io::{self, BufRead, Write},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use serde_json::{Value, json};

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
struct DebugFrame {
    function: String,
    depth: usize,
    line: u64,
    variables: Vec<crate::runtime::DebugVariable>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Default)]
struct SessionState {
    paused: bool,
    step: Option<StepMode>,
    terminate: bool,
    started: bool,
    frame: Option<DebugFrame>,
    events: Vec<Value>,
}

#[derive(Clone, Copy, Debug)]
enum StepMode {
    Next(usize),
    In,
    Out(usize),
}

type SharedSession = Arc<(Mutex<SessionState>, Condvar)>;

/// Runs the bounded DAP lifecycle service over standard input/output.
/// # Errors
///
/// Returns a framing, JSON, or I/O error when the client sends malformed data
/// or the service cannot write a response.
#[allow(clippy::too_many_lines)]
pub fn run_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let output = Arc::new(Mutex::new(io::stdout()));
    let sequence = Arc::new(AtomicU64::new(1));
    let breakpoints = Arc::new(Mutex::new(HashMap::<String, BTreeSet<u64>>::new()));
    let mut launched = false;
    let mut configured = false;
    let mut terminated = false;
    let session: SharedSession = Arc::new((Mutex::new(SessionState::default()), Condvar::new()));
    let event_output = Arc::clone(&output);
    let event_session = Arc::clone(&session);
    let event_sequence = Arc::clone(&sequence);
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_millis(5));
            let events = {
                let (lock, _) = &*event_session;
                let Ok(mut state) = lock.lock() else { break };
                std::mem::take(&mut state.events)
            };
            if let Ok(mut output) = event_output.lock() {
                for mut event in events {
                    if let Some(object) = event.as_object_mut() {
                        object.insert(
                            "seq".into(),
                            json!(event_sequence.fetch_add(1, Ordering::Relaxed)),
                        );
                    }
                    if write_message(&mut *output, &event).is_err() {
                        return;
                    }
                }
            } else {
                break;
            }
        }
    });
    let mut execution = None;
    while let Some(message) = read_message(&mut input)? {
        let request_seq = message.get("seq").and_then(Value::as_u64).unwrap_or(0);
        let command = message.get("command").and_then(Value::as_str).unwrap_or("");
        let (success, body, message_text) = match command {
            "initialize" => (
                true,
                json!({
                    "supportsConfigurationDoneRequest": true,
                    "supportsTerminateRequest": true,
                    "supportsEvaluateForHovers": true
                }),
                None,
            ),
            "launch" => match validate_launch(&message) {
                Ok(()) if !launched && !terminated => {
                    launched = true;
                    let program = message["arguments"]["program"]
                        .as_str()
                        .ok_or("launch requires a program path")?
                        .to_owned();
                    let state = Arc::clone(&session);
                    let breakpoints_for_thread = Arc::clone(&breakpoints);
                    let event_state = Arc::clone(&state);
                    execution = Some(thread::spawn(move || {
                        let result = execute_program(&program, &state, &breakpoints_for_thread);
                        let (lock, _) = &*event_state;
                        if let Ok(mut state) = lock.lock() {
                            state.events.push(json!({
                                "type": "event",
                                "event": "exited",
                                "body": {"exitCode": result.unwrap_or(1)},
                            }));
                            state
                                .events
                                .push(json!({"type": "event", "event": "terminated", "body": {}}));
                        }
                    }));
                    (true, json!({}), None)
                }
                Ok(()) => (
                    false,
                    json!({}),
                    Some("launch is not valid in the current state".into()),
                ),
                Err(error) => (false, json!({}), Some(error)),
            },
            "configurationDone" if launched && !terminated && !configured => {
                configured = true;
                (true, json!({}), None)
            }
            "configurationDone" => (
                false,
                json!({}),
                Some("configurationDone requires launch".into()),
            ),
            "continue" => {
                resume_session(&session, None);
                (true, json!({"allThreadsContinued": true}), None)
            }
            "pause" => {
                request_pause(&session);
                (true, json!({}), None)
            }
            "next" => {
                let depth = current_depth(&session);
                resume_session(&session, Some(StepMode::Next(depth)));
                (true, json!({}), None)
            }
            "stepIn" => {
                resume_session(&session, Some(StepMode::In));
                (true, json!({}), None)
            }
            "stepOut" => {
                let depth = current_depth(&session);
                resume_session(&session, Some(StepMode::Out(depth.saturating_sub(1))));
                (true, json!({}), None)
            }
            "threads" => (true, json!({"threads": [{"id": 1, "name": "Start"}]}), None),
            "stackTrace" => (true, stack_response(&session), None),
            "scopes" => (
                true,
                json!({"scopes": [{"name": "Locals", "variablesReference": 1, "expensive": false}]}),
                None,
            ),
            "variables" => (true, variables_response(&session), None),
            "evaluate" => (true, evaluate_response(&session, &message), None),
            "disconnect" | "terminate" => {
                terminate_session(&session);
                terminated = true;
                (true, json!({}), None)
            }
            "setBreakpoints" => {
                let response = breakpoints.lock().map_or_else(
                    |_| json!({"breakpoints": []}),
                    |mut registry| breakpoint_response(&message, &mut registry),
                );
                (true, response, None)
            }
            "setExceptionBreakpoints" => (true, json!({"breakpoints": []}), None),
            _ => (false, json!({}), Some("request is not implemented".into())),
        };
        let response_sequence = sequence.fetch_add(1, Ordering::Relaxed);
        let mut output_guard = output
            .lock()
            .map_err(|_| "DAP output lock poisoned".to_string())?;
        write_message(
            &mut *output_guard,
            &json!({
                "seq": response_sequence,
                "type": "response",
                "request_seq": request_seq,
                "success": success,
                "command": command,
                "message": message_text,
                "body": body,
            }),
        )?;
        if command == "disconnect" || command == "terminate" {
            break;
        }
    }
    terminate_session(&session);
    if let Some(handle) = execution {
        let _ = handle.join();
    }
    Ok(())
}

fn validate_launch(message: &Value) -> Result<(), String> {
    let program = message
        .get("arguments")
        .and_then(|arguments| arguments.get("program"))
        .and_then(Value::as_str)
        .ok_or_else(|| "launch requires a program path".to_string())?;
    if !std::path::Path::new(program)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("bn"))
    {
        return Err("launch program must have a .bn extension".into());
    }
    let metadata = std::fs::metadata(program)
        .map_err(|error| format!("cannot open launch program: {error}"))?;
    if !metadata.is_file() {
        return Err("launch program is not a regular file".into());
    }
    if metadata.len() > 8 * 1024 * 1024 {
        return Err("launch program exceeds 8 MiB".into());
    }
    let graph = crate::module_graph::load(program)
        .map_err(|error| format!("cannot load launch program: {}", error.diagnostic.message))?;
    if graph.modules.len() > 256 {
        return Err("launch module graph exceeds 256 modules".into());
    }
    let models = crate::semantic::analyze_modules(&graph).map_err(|error| {
        format!(
            "launch semantic analysis failed: {}",
            error.diagnostic.message
        )
    })?;
    crate::ir::lower_graph_validated(&graph, &models)
        .map_err(|error| format!("launch lowering failed: {}", error.message))?;
    Ok(())
}

fn execute_program(
    path: &str,
    session: &SharedSession,
    breakpoints: &Arc<Mutex<HashMap<String, BTreeSet<u64>>>>,
) -> Result<u8, String> {
    let graph =
        crate::module_graph::load(path).map_err(|error| error.diagnostic.message.clone())?;
    let models = crate::semantic::analyze_modules(&graph)
        .map_err(|error| error.diagnostic.message.clone())?;
    let module =
        crate::ir::lower_graph_validated(&graph, &models).map_err(|error| error.message.clone())?;
    let mut input = io::Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let session_for_hook = Arc::clone(session);
    let mut control = move |function: &str,
                            depth: usize,
                            span: crate::source::Span,
                            variables: &[crate::runtime::DebugVariable]| {
        let (lock, condvar) = &*session_for_hook;
        let Ok(mut state) = lock.lock() else {
            return crate::runtime::DebugDecision::Terminate;
        };
        state.frame = Some(DebugFrame {
            function: function.to_owned(),
            depth,
            line: u64::try_from(span.start.line).unwrap_or(u64::MAX),
            variables: variables.to_vec(),
        });
        let at_breakpoint = breakpoints
            .lock()
            .ok()
            .and_then(|registry| registry.get(path).cloned())
            .is_some_and(|lines| {
                lines.contains(&state.frame.as_ref().map_or(0, |frame| frame.line))
            });
        let step_pause = match state.step {
            Some(StepMode::Next(target)) => depth <= target,
            Some(StepMode::In) => true,
            Some(StepMode::Out(target)) => depth < target,
            None => false,
        };
        if !state.started || step_pause || at_breakpoint {
            state.started = true;
            state.step = None;
            state.paused = true;
            state.events.push(json!({
                "type": "event", "event": "stopped",
                "body": {"reason": if at_breakpoint { "breakpoint" } else { "step" }, "threadId": 1}
            }));
            condvar.notify_all();
        }
        while state.paused && !state.terminate {
            state = match condvar.wait(state) {
                Ok(guard) => guard,
                Err(_) => return crate::runtime::DebugDecision::Terminate,
            };
        }
        if state.terminate {
            crate::runtime::DebugDecision::Terminate
        } else {
            crate::runtime::DebugDecision::Continue
        }
    };
    crate::runtime::execute_validated_with_host_debug_control(
        &module,
        &mut input,
        &mut output,
        &crate::runtime::HostEnv::system(vec![path.to_owned()]),
        &mut control,
    )
    .map_err(|error| error.message)
}

fn resume_session(session: &SharedSession, step: Option<StepMode>) {
    let (lock, condvar) = &**session;
    if let Ok(mut state) = lock.lock() {
        state.step = step;
        state.paused = false;
        state
            .events
            .push(json!({"type": "event", "event": "continued", "body": {"threadId": 1}}));
        condvar.notify_all();
    }
}

fn current_depth(session: &SharedSession) -> usize {
    let (lock, _) = &**session;
    lock.lock()
        .ok()
        .and_then(|state| state.frame.as_ref().map(|frame| frame.depth))
        .unwrap_or(0)
}

fn request_pause(session: &SharedSession) {
    let (lock, _) = &**session;
    if let Ok(mut state) = lock.lock() {
        state.paused = true;
    }
}

fn terminate_session(session: &SharedSession) {
    let (lock, condvar) = &**session;
    if let Ok(mut state) = lock.lock() {
        state.terminate = true;
        state.paused = false;
        condvar.notify_all();
    }
}

fn stack_response(session: &SharedSession) -> Value {
    let (lock, _) = &**session;
    let frame = lock.lock().ok().and_then(|state| state.frame.clone());
    let frames = frame
        .into_iter()
        .map(|frame| {
            json!({
                "id": 1,
                "name": frame.function,
                "line": frame.line,
                "column": 1,
                "source": {"name": "Basic Next"}
            })
        })
        .collect::<Vec<_>>();
    json!({"stackFrames": frames, "totalFrames": frames.len()})
}

fn variables_response(session: &SharedSession) -> Value {
    let (lock, _) = &**session;
    let variables = lock
        .lock()
        .ok()
        .and_then(|state| state.frame.clone())
        .map(|frame| {
            frame
                .variables
                .into_iter()
                .map(|variable| json!({"name": variable.name, "value": variable.value, "variablesReference": 0}))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"variables": variables})
}

fn evaluate_response(session: &SharedSession, request: &Value) -> Value {
    let expression = request
        .get("arguments")
        .and_then(|arguments| arguments.get("expression"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let value = session
        .0
        .lock()
        .ok()
        .and_then(|state| state.frame.clone())
        .and_then(|frame| {
            frame
                .variables
                .into_iter()
                .find(|variable| variable.name == expression)
        })
        .map_or_else(|| "<unavailable>".into(), |variable| variable.value);
    json!({"result": value, "variablesReference": 0})
}

fn breakpoint_response(message: &Value, registry: &mut HashMap<String, BTreeSet<u64>>) -> Value {
    let arguments = message.get("arguments").and_then(Value::as_object);
    let source = arguments
        .and_then(|arguments| arguments.get("source"))
        .and_then(Value::as_object)
        .and_then(|source| source.get("path"))
        .and_then(Value::as_str)
        .unwrap_or("<unknown>")
        .to_owned();
    let requested = arguments
        .and_then(|arguments| arguments.get("breakpoints"))
        .and_then(Value::as_array)
        .map(|points| {
            points
                .iter()
                .filter_map(|point| point.get("line").and_then(Value::as_u64))
                .filter(|line| *line > 0)
                .take(1024)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let executable = std::fs::read_to_string(&source)
        .ok()
        .and_then(|text| executable_lines_from_source(&text, source.clone()).ok());
    let accepted = requested
        .iter()
        .copied()
        .filter(|line| executable.as_ref().is_none_or(|lines| lines.contains(line)))
        .collect::<BTreeSet<_>>();
    registry.insert(source, accepted.clone());
    json!({"breakpoints": requested.into_iter().map(|line| {
        let verified = executable.as_ref().is_some_and(|lines| lines.contains(&line));
        let message = if verified {
            "breakpoint mapped to an executable statement"
        } else if executable.is_some() {
            "line is not an executable statement"
        } else {
            "source is unavailable for breakpoint mapping"
        };
        json!({"verified": verified, "line": line, "message": message})
    }).collect::<Vec<_>>()})
}

fn executable_lines_from_source(source: &str, name: String) -> Result<BTreeSet<u64>, String> {
    if source.len() > 8 * 1024 * 1024 {
        return Err("source exceeds 8 MiB".into());
    }
    let tokens = crate::lexer::lex(&crate::source::SourceFile::new(name.clone(), source))
        .map_err(|error| error.message)?;
    let program = crate::parser::parse_named(&tokens, name).map_err(|error| error.message)?;
    let mut lines = BTreeSet::new();
    for item in program.items {
        if let crate::ast::Item::Declaration { statements, .. } = item {
            collect_statement_lines(&statements, &mut lines);
        }
    }
    Ok(lines)
}

fn collect_statement_lines(statements: &[crate::ast::Statement], lines: &mut BTreeSet<u64>) {
    for statement in statements {
        let span = match statement {
            crate::ast::Statement::If {
                span,
                branches,
                otherwise,
            } => {
                for branch in branches {
                    collect_statement_lines(&branch.body.statements, lines);
                }
                if let Some(block) = otherwise {
                    collect_statement_lines(&block.statements, lines);
                }
                *span
            }
            crate::ast::Statement::While { span, body, .. }
            | crate::ast::Statement::Repeat { span, body, .. }
            | crate::ast::Statement::For { span, body, .. } => {
                collect_statement_lines(&body.statements, lines);
                *span
            }
            crate::ast::Statement::MemberFunction { span, body, .. } => {
                if let Some(block) = body {
                    collect_statement_lines(&block.statements, lines);
                }
                *span
            }
            crate::ast::Statement::Binding { span, .. }
            | crate::ast::Statement::Assignment { span, .. }
            | crate::ast::Statement::Return { span, .. }
            | crate::ast::Statement::Print { span, .. }
            | crate::ast::Statement::ClearScreen { span, .. }
            | crate::ast::Statement::Beep { span, .. }
            | crate::ast::Statement::Delete { span, .. }
            | crate::ast::Statement::Stop { span, .. }
            | crate::ast::Statement::Control { span, .. }
            | crate::ast::Statement::Call { span, .. } => *span,
        };
        if let Ok(line) = u64::try_from(span.start.line) {
            lines.insert(line);
        }
    }
}

fn read_message(input: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid Content-Length")?,
            );
        }
    }
    let length = length.ok_or("missing Content-Length")?;
    if length > MAX_MESSAGE_BYTES {
        return Err("DAP message exceeds 1 MiB".into());
    }
    let mut payload = vec![0; length];
    input
        .read_exact(&mut payload)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|error| format!("invalid DAP JSON: {error}"))
}

fn write_message(output: &mut impl Write, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err("DAP response exceeds 1 MiB".into());
    }
    write!(output, "Content-Length: {}\r\n\r\n", payload.len())
        .map_err(|error| error.to_string())?;
    output
        .write_all(&payload)
        .map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
