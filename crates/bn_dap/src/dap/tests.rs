use super::{
    SessionState, StepMode, breakpoint_response, evaluate_response, executable_lines_from_source,
    execute_program, read_message, request_pause, resume_session, terminate_session,
    validate_launch, variables_response, write_message,
};
use serde_json::json;
use std::{
    collections::{BTreeSet, HashMap},
    io::Cursor,
    sync::{Arc, Condvar, Mutex},
};

#[test]
fn framing_round_trips_bounded_json() {
    let payload = serde_json::to_vec(&json!({"seq": 1, "command": "initialize"})).unwrap();
    let framed = format!("Content-Length: {}\r\n\r\n", payload.len());
    let mut input = Cursor::new([framed.as_bytes(), payload.as_slice()].concat());
    assert_eq!(read_message(&mut input).unwrap().unwrap()["seq"], 1);
    let mut output = Vec::new();
    write_message(&mut output, &json!({"ok": true})).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .starts_with("Content-Length:")
    );
}

#[test]
fn breakpoint_registry_deduplicates_and_bounds_lines() {
    let mut registry = HashMap::new();
    let response = breakpoint_response(
        &json!({"arguments": {"source": {"path": "main.bn"}, "breakpoints": [
            {"line": 4}, {"line": 4}, {"line": 0}
        ]}}),
        &mut registry,
    );
    assert_eq!(response["breakpoints"].as_array().unwrap().len(), 1);
    assert_eq!(registry["main.bn"].len(), 1);
    assert_eq!(response["breakpoints"][0]["verified"], false);
}

#[test]
fn executable_line_mapping_uses_statement_spans() {
    let lines = executable_lines_from_source(
        "FUNCTION Start() AS VOID\nPRINT \"ok\"\nEND FUNCTION\n",
        "main.bn".into(),
    )
    .unwrap();
    assert!(lines.contains(&2));
    assert!(!lines.contains(&1));
}

#[test]
fn launch_requires_a_bounded_bn_file() {
    assert!(validate_launch(&json!({"arguments": {"program": "missing.txt"}})).is_err());
    assert!(validate_launch(&json!({"arguments": {"program": "missing.bn"}})).is_err());
}

// Unit tests run with the crate directory as cwd; the fixture lives at the
// workspace root.
const HELLO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/hello.bn");

#[test]
fn launch_accepts_a_valid_frontend_program() {
    assert!(validate_launch(&json!({"arguments": {"program": HELLO}})).is_ok());
}

#[test]
fn execution_session_pauses_then_resumes() {
    let session = Arc::new((
        Mutex::new(SessionState {
            stop_on_entry: true,
            ..SessionState::default()
        }),
        Condvar::new(),
    ));
    let breakpoints = Arc::new(Mutex::new(HashMap::new()));
    let worker_session = Arc::clone(&session);
    let worker_breakpoints = Arc::clone(&breakpoints);
    let worker =
        std::thread::spawn(move || execute_program(HELLO, &worker_session, &worker_breakpoints));
    let (lock, condvar) = &*session;
    let mut state = lock.lock().unwrap();
    while !state.paused {
        state = condvar.wait(state).unwrap();
    }
    drop(state);
    resume_session(&session, None);
    assert_eq!(worker.join().unwrap().unwrap(), 0);
}

/// The ARC inspection of `bni dap` (proposal `arc-shared-core-0.6.5`): an
/// object shows its class, id and strong count, expands into its fields and
/// an `[arc]` child; a weak binding shows whether its object lives; the `ARC`
/// scope lists the live objects; `:arc` and `:arc name` read the core.
#[test]
fn a_paused_session_shows_objects_from_the_arc_core() {
    let directory = std::env::temp_dir().join(format!("bn-dap-arc-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("arc.bn");
    std::fs::write(
        &path,
        "CLASS Box\n    PUBLIC label AS STRING = \"b\"\n    PUBLIC FUNCTION CONSTRUCTOR()\n    END FUNCTION\nEND CLASS\n\nFUNCTION Start() AS VOID\n    LET b AS Box = NEW Box()\n    LET w AS WEAK Box OR NULL = b\n    PRINT b.label\nEND FUNCTION\n",
    )
    .unwrap();
    let program = path.to_string_lossy().into_owned();
    let session = Arc::new((Mutex::new(SessionState::default()), Condvar::new()));
    let breakpoints = Arc::new(Mutex::new(HashMap::from([(
        program.clone(),
        BTreeSet::from([10_u64]),
    )])));
    let worker_session = Arc::clone(&session);
    let worker_breakpoints = Arc::clone(&breakpoints);
    let worker_program = program.clone();
    let worker = std::thread::spawn(move || {
        execute_program(&worker_program, &worker_session, &worker_breakpoints)
    });
    // The first pause is at the start; continue to the breakpoint (line 10).
    let wait_line = |line: u64| {
        let (lock, condvar) = &*session;
        let mut state = lock.lock().unwrap();
        while !(state.paused && state.frame.as_ref().is_some_and(|frame| frame.line == line)) {
            if state.paused {
                drop(state);
                resume_session(&session, None);
                state = lock.lock().unwrap();
            }
            state = condvar
                .wait_timeout(state, std::time::Duration::from_millis(50))
                .unwrap()
                .0;
        }
    };
    wait_line(10);
    let locals = variables_response(&session, &json!({"arguments": {"variablesReference": 1}}));
    let locals = locals["variables"].as_array().unwrap();
    let object = locals
        .iter()
        .find(|variable| variable["value"].as_str().unwrap().contains("(strong 1)"))
        .expect("the object binding shows its strong count");
    assert!(
        object["value"].as_str().unwrap().starts_with("Box#"),
        "{object}"
    );
    let weak = locals
        .iter()
        .find(|variable| variable["value"].as_str().unwrap().starts_with("→ Box#"))
        .expect("the weak binding shows its object");
    assert!(
        weak["value"].as_str().unwrap().ends_with("(alive)"),
        "{weak}"
    );
    let reference = object["variablesReference"].as_u64().unwrap();
    assert_ne!(reference, 0, "an object expands");
    let children = variables_response(
        &session,
        &json!({"arguments": {"variablesReference": reference}}),
    );
    let children = children["variables"].as_array().unwrap();
    assert!(children.iter().any(|child| child["name"] == "label"));
    let arc = children
        .iter()
        .find(|child| child["name"] == "[arc]")
        .expect("an [arc] child");
    assert!(
        arc["value"]
            .as_str()
            .unwrap()
            .contains("strong 1, generation 1"),
        "{arc}"
    );
    let scope = variables_response(&session, &json!({"arguments": {"variablesReference": 2}}));
    let scope = scope["variables"].as_array().unwrap();
    assert_eq!(scope.len(), 1, "{scope:?}");
    assert_eq!(scope[0]["value"], "Box (strong 1)");
    let listed = evaluate_response(&session, &json!({"arguments": {"expression": ":arc"}}));
    assert!(
        listed["result"]
            .as_str()
            .unwrap()
            .ends_with("Box (strong 1)"),
        "{listed}"
    );
    let name = object["name"].as_str().unwrap();
    let one = evaluate_response(
        &session,
        &json!({"arguments": {"expression": format!(":arc {name}")}}),
    );
    assert!(
        one["result"].as_str().unwrap().contains("generation 1"),
        "{one}"
    );
    terminate_session(&session);
    let _ = worker.join();
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn dap_stepping_next_step_in_step_out_and_pause() {
    let directory = std::env::temp_dir().join(format!("bn-dap-step-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("step.bn");
    std::fs::write(
        &path,
        "FUNCTION Helper(x AS INTEGER) AS INTEGER\n    LET y AS INTEGER = x * 2\n    RETURN y\nEND FUNCTION\n\nFUNCTION Start() AS VOID\n    LET a AS INTEGER = 10\n    LET b AS INTEGER = Helper(a)\n    PRINT b\nEND FUNCTION\n",
    )
    .unwrap();
    let program = path.to_string_lossy().into_owned();
    let session = Arc::new((
        Mutex::new(SessionState {
            stop_on_entry: true,
            ..SessionState::default()
        }),
        Condvar::new(),
    ));
    let breakpoints = Arc::new(Mutex::new(HashMap::new()));
    let worker_session = Arc::clone(&session);
    let worker_breakpoints = Arc::clone(&breakpoints);
    let worker_program = program.clone();
    let worker = std::thread::spawn(move || {
        execute_program(&worker_program, &worker_session, &worker_breakpoints)
    });

    let wait_for_pause = |session: &Arc<(Mutex<SessionState>, Condvar)>| {
        let (lock, condvar) = &**session;
        let mut state = lock.lock().unwrap();
        while !state.paused {
            state = condvar.wait(state).unwrap();
        }
        (
            state
                .frame
                .as_ref()
                .map(|f| f.function.clone())
                .unwrap_or_default(),
            state.frame.as_ref().map_or(0, |f| f.depth),
            state.frame.as_ref().map_or(0, |f| f.line),
        )
    };

    // 1. Initial pause at entry
    let (func, depth, line) = wait_for_pause(&session);
    assert_eq!(func, "Start");
    assert_eq!(depth, 0);
    assert_eq!(line, 7);

    // 2. Step in -> advances to LET b = Helper(a) at line 8
    resume_session(&session, Some(StepMode::In));
    let (func, depth, line) = wait_for_pause(&session);
    assert_eq!(func, "Start");
    assert_eq!(depth, 0);
    assert_eq!(line, 8);

    // 3. Step in -> steps into Helper at line 2
    resume_session(&session, Some(StepMode::In));
    let (func, depth, line) = wait_for_pause(&session);
    assert_eq!(func, "Helper");
    assert_eq!(depth, 1);
    assert_eq!(line, 2);

    // 4. Next -> advances to RETURN y at line 3 in Helper
    resume_session(&session, Some(StepMode::Next(1)));
    let (func, depth, line) = wait_for_pause(&session);
    assert_eq!(func, "Helper");
    assert_eq!(depth, 1);
    assert_eq!(line, 3);

    // 5. Step out -> returns to Start at depth 0
    resume_session(&session, Some(StepMode::Out(0)));
    let (func, depth, _line) = wait_for_pause(&session);
    assert_eq!(func, "Start");
    assert_eq!(depth, 0);

    // 6. Test pause command
    resume_session(&session, None);
    request_pause(&session);
    // Should stop due to pause
    let (lock, condvar) = &*session;
    let mut state = lock.lock().unwrap();
    while !state.paused {
        state = condvar.wait(state).unwrap();
    }
    assert!(state.paused);
    drop(state);

    // 7. Resume to finish
    resume_session(&session, None);
    let exit_code = worker.join().unwrap().unwrap();
    assert_eq!(exit_code, 0);
    let _ = std::fs::remove_dir_all(&directory);
}
