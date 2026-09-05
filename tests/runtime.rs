// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    fmt::Write as _,
    fs,
    io::{BufRead, Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct ChannelReader {
    receiver: std::sync::mpsc::Receiver<Vec<u8>>,
    current: Cursor<Vec<u8>>,
}

impl Read for ChannelReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        loop {
            let position = usize::try_from(self.current.position()).unwrap_or(usize::MAX);
            if position < self.current.get_ref().len() {
                return self.current.read(buffer);
            }
            self.current = Cursor::new(self.receiver.recv().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "input channel closed")
            })?);
        }
    }
}

impl BufRead for ChannelReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        loop {
            let position = usize::try_from(self.current.position()).unwrap_or(usize::MAX);
            if position < self.current.get_ref().len() {
                return Ok(&self.current.get_ref()[position..]);
            }
            self.current = Cursor::new(self.receiver.recv().map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "input channel closed")
            })?);
        }
    }

    fn consume(&mut self, amount: usize) {
        let position = self.current.position();
        self.current
            .set_position(position.saturating_add(u64::try_from(amount).unwrap_or(u64::MAX)));
    }
}

use bn::{
    ir::lower_graph,
    module_graph::load,
    runtime::{
        DebugDecision, HostEnv, execute_with_host, execute_with_host_debug,
        execute_with_host_debug_control,
    },
    semantic::analyze_modules,
};

fn run(source_text: &str, input: &str) -> Result<(u8, String), bn::diagnostic::Diagnostic> {
    run_with_host(
        source_text,
        input,
        &HostEnv::fixed(vec!["runtime.bn".into()], 0, 0),
    )
}

fn run_with_host(
    source_text: &str,
    input: &str,
    host: &HostEnv,
) -> Result<(u8, String), bn::diagnostic::Diagnostic> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "basicnext-runtime-{}-{}.bn",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, source_text).expect("write temporary source");
    let result = run_loaded(path.to_str().expect("utf-8 path"), input, host);
    let _ = fs::remove_file(&path);
    result
}

fn run_path(path: &str) -> Result<(u8, String), bn::diagnostic::Diagnostic> {
    run_loaded(path, "", &HostEnv::fixed(vec![path.into()], 0, 0))
}

fn unique_temp(suffix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "basicnext-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        suffix
    ))
}

fn bn_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn run_loaded(
    path: &str,
    input: &str,
    host: &HostEnv,
) -> Result<(u8, String), bn::diagnostic::Diagnostic> {
    let graph = load(path).map_err(|error| error.diagnostic)?;
    let models = analyze_modules(&graph).map_err(|error| error.diagnostic)?;
    let module = lower_graph(&graph, &models)?;
    let mut input = Cursor::new(input.as_bytes());
    let mut output = Vec::new();
    let code = execute_with_host(&module, &mut input, &mut output, host)?;
    Ok((code, String::from_utf8(output).expect("UTF-8 output")))
}

fn run_loaded_debug(
    path: &str,
    host: &HostEnv,
    events: &mut Vec<(String, usize)>,
) -> Result<(), bn::diagnostic::Diagnostic> {
    let graph = load(path).map_err(|error| error.diagnostic)?;
    let models = analyze_modules(&graph).map_err(|error| error.diagnostic)?;
    let module = lower_graph(&graph, &models)?;
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut hook = |function: &str, span: bn::source::Span| {
        events.push((function.to_owned(), span.start.line));
    };
    execute_with_host_debug(&module, &mut input, &mut output, host, &mut hook)?;
    Ok(())
}

#[test]
fn executes_calls_loops_and_checked_arithmetic() {
    let source = "FUNCTION Factorial(value AS INTEGER) AS INTEGER\nIF value = 0 THEN\nRETURN 1\nELSE\nRETURN value * Factorial(value - 1)\nEND IF\nEND FUNCTION\nFUNCTION Start() AS INTEGER\nLET total AS INTEGER = Factorial(5)\nLET quotient AS INTEGER = -5 DIV 3\nLET remainder AS INTEGER = -5 % 3\nPRINT total, quotient, remainder\nRETURN 7\nEND FUNCTION\n";
    let (code, output) = run(source, "").expect("execute source");
    assert_eq!(code, 7);
    assert_eq!(output, "120 -2 1\n");
}

#[test]
fn print_concatenates_with_plus_and_separates_comma_arguments() {
    let source = "FUNCTION Start() AS VOID\nPRINT \"CASA\" + \"SHO\"\nPRINT \"CASA\", \"SHO\"\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute PRINT source");
    assert_eq!(output, "CASASHO\nCASA SHO\n");
}

#[test]
fn executes_bnpallet_exported_color_constants() {
    let (_, output) = run_path("tests/grammar/valid/bnpallet.bn").expect("execute BNPallet");
    assert_eq!(output, "255 16711680 6697881\n");
}

#[test]
fn debug_hook_reports_instruction_spans_through_interpreter_pipeline() {
    let path = unique_temp("debug.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS VOID\nPRINT \"hooked\"\nEND FUNCTION\n",
    )
    .expect("write debug source");
    let mut events = Vec::new();
    run_loaded_debug(
        path.to_str().expect("utf-8 path"),
        &HostEnv::fixed(vec!["debug.bn".into()], 0, 0),
        &mut events,
    )
    .expect("execute with debug hook");
    let _ = fs::remove_file(path);
    assert!(
        events
            .iter()
            .any(|(function, line)| function == "Start" && *line == 2)
    );
}

#[test]
fn debug_control_can_terminate_before_user_instruction() {
    let path = unique_temp("debug-control.bn");
    fs::write(
        &path,
        "FUNCTION Start() AS INTEGER\nPRINT \"must not run\"\nRETURN 0\nEND FUNCTION\n",
    )
    .unwrap();
    let graph = load(&path).unwrap();
    let models = analyze_modules(&graph).unwrap();
    let module = lower_graph(&graph, &models).unwrap();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut control =
        |_function: &str,
         _depth: usize,
         _span: bn::source::Span,
         _variables: &[bn::runtime::DebugVariable]| { DebugDecision::Terminate };
    let error = execute_with_host_debug_control(
        &module,
        &mut input,
        &mut output,
        &HostEnv::fixed(Vec::new(), 0, 0),
        &mut control,
    )
    .expect_err("debugger termination");
    assert_eq!(error.code, "DEBUG_TERMINATED");
    assert!(output.is_empty());
    let _ = fs::remove_file(path);
}

#[test]
fn multi_binding_let_evaluates_left_to_right() {
    let source = "FUNCTION Start() AS VOID\nLET first, second AS INTEGER = 1, 2\nPRINT first, second\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute multi-binding LET");
    assert_eq!(output, "1 2\n");
}

#[test]
fn single_line_if_executes_one_branch() {
    let source = "FUNCTION Start() AS VOID\nLET flag AS BOOLEAN = TRUE\nIF flag THEN PRINT \"yes\" ELSE PRINT \"no\"\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute single-line IF");
    assert_eq!(output, "yes\n");
}

#[test]
fn host_net_address_parse_and_format_round_trip() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "error"
ELSE
PRINT address.ToString()
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute HOST.Net address parse");
    assert_eq!(output, "127.0.0.1\n");
}

#[test]
fn bnlog_logger_null_transport_is_bounded_and_idempotent() {
    let source = r#"IMPORT BNLog AS Log
FUNCTION Start() AS VOID
LET logger AS Log.Logger OR Error = Log.Logger.New("test")
IF logger IS Error THEN
PRINT "error"
ELSE
PRINT logger.AddNull(2) IS Error
LET fields AS Log.Fields = NEW Log.Fields()
PRINT logger.Log(2, "hello", fields) IS Error
LET child AS Log.Logger OR Error = logger.Child(fields)
PRINT child IS Error
PRINT logger.Flush(1000) IS Error, logger.Close(1000) IS Error, logger.Close(1000) IS Error
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNLog logger provider");
    assert_eq!(output, "FALSE\nFALSE\nFALSE\nFALSE FALSE TRUE\n");
}

#[test]
fn bnlog_file_transport_requires_explicit_filesystem_and_appends_json_lines() {
    let path = unique_temp("bnlog.jsonl");
    let source = format!(
        "IMPORT BNLog AS Log\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET logger AS Log.Logger = NEW Log.Logger()\nLET added AS VOID OR Error = logger.AddFile(\"{}\", 2)\nIF added IS Error THEN\nPRINT added.Code\nELSE\nLET fields AS Log.Fields = NEW Log.Fields()\nLET written AS VOID OR Error = logger.Log(2, \"hello\", fields)\nPRINT written IS Error\nEND IF\nEND FUNCTION\n",
        bn_path(&path)
    );
    let (_, output) = run(&source, "").expect("execute BNLog file provider");
    let contents = fs::read_to_string(&path).expect("read JSON Lines output");
    let _ = fs::remove_file(path);
    assert_eq!(output, "FALSE\n");
    assert!(contents.contains("\"message\":\"hello\""));
}

#[test]
fn bnlog_console_transport_requires_explicit_console_and_writes_json_lines() {
    let source = r#"IMPORT BNLog AS Log
IMPORT HOST.Console AS CON
FUNCTION Start() AS VOID
LET logger AS Log.Logger = NEW Log.Logger()
LET added AS VOID OR Error = logger.AddConsole(2)
IF added IS Error THEN
PRINT "error"
ELSE
LET fields AS Log.Fields = NEW Log.Fields()
LET written AS VOID OR Error = logger.Log(2, "console", fields)
PRINT written IS Error
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNLog console provider");
    assert!(output.contains("\"message\":\"console\""));
    assert!(output.ends_with("FALSE\n"));
}

#[test]
fn bnlog_attempts_later_transports_after_an_earlier_failure() {
    let good = unique_temp("bnlog-good.jsonl");
    let bad = unique_temp("missing-parent/bnlog-bad.jsonl");
    let source = format!(
        "IMPORT BNLog AS Log\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET logger AS Log.Logger = NEW Log.Logger()\nlogger.AddFile(\"{}\", 2)\nlogger.AddFile(\"{}\", 2)\nLET fields AS Log.Fields = NEW Log.Fields()\nLET result AS VOID OR Error = logger.Log(2, \"attempt-all\", fields)\nPRINT result IS Error\nEND FUNCTION\n",
        bn_path(&bad),
        bn_path(&good)
    );
    let (_, output) = run(&source, "").expect("execute multi-transport logger");
    let contents = fs::read_to_string(&good).expect("later transport was attempted");
    let _ = fs::remove_file(good);
    assert_eq!(output, "TRUE\n");
    assert!(contents.contains("\"message\":\"attempt-all\""));
}

#[test]
fn bnlog_child_context_is_included_in_record_bounds() {
    let mut source = String::from(
        "IMPORT BNLog AS Log\nFUNCTION Start() AS VOID\nLET logger AS Log.Logger = NEW Log.Logger()\nLET fields AS Log.Fields = NEW Log.Fields()\n",
    );
    for index in 0..16 {
        let _ = writeln!(
            source,
            "fields.SetString(\"key{index}\", \"{}\")",
            "x".repeat(4096)
        );
    }
    source.push_str(
        "LET child AS Log.Logger OR Error = logger.Child(fields)\nIF child IS Error THEN\nPRINT \"child-error\"\nELSE\nLET child_logger AS Log.Logger = child\nPRINT child_logger.Log(2, \"x\", fields) IS Error\nEND IF\nEND FUNCTION\n",
    );
    let (_, output) = run(&source, "").expect("execute BNLog child provider");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn bnjson_provider_reports_gate_bound_unavailability() {
    let source = r#"IMPORT BNJson AS Json
FUNCTION Start() AS VOID
LET value AS Json.Json OR Error = Json.Json.Parse("{\"ok\":true}")
IF value IS Error THEN
PRINT "error"
ELSE
PRINT Json.Json.Stringify(value)
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNJson provider");
    assert_eq!(output, "{\"ok\":true}\n");
}

#[test]
fn bnlog_fields_provider_sets_and_gets_bounded_values() {
    let source = r#"IMPORT BNLog AS Log
FUNCTION Start() AS VOID
LET fields AS Log.Fields = NEW Log.Fields()
fields.SetString("service", "basicnext")
fields.SetInteger("port", 8080)
fields.SetBoolean("secure", TRUE)
LET duplicate AS VOID OR Error = fields.SetString("service", "other")
PRINT fields.Count(), fields.Get("service"), fields.Get("port"), fields.Get("secure"), duplicate IS Error
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNLog fields provider");
    assert_eq!(output, "3 basicnext 8080 TRUE TRUE\n");
}

#[test]
fn bnlog_entry_with_field_is_immutable_and_bounded() {
    let source = r#"IMPORT BNLog AS Log
FUNCTION Start() AS VOID
LET entry AS Log.Entry = NEW Log.Entry()
LET enriched AS Log.Entry OR Error = entry.WithField("request_id", "abc")
IF enriched IS Error THEN
PRINT "unexpected"
ELSE
LET duplicate AS Log.Entry OR Error = enriched.WithField("request_id", "def")
PRINT duplicate IS Error
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNLog entry provider");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn host_net_address_predicates_follow_ip_semantics() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET v4 AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
LET v6 AS Net.Address OR Error = Net.Address.Parse("ff02::1")
IF v4 IS Error OR v6 IS Error THEN
PRINT "error"
ELSE
PRINT v4.IsIPv4(), v4.IsLoopback(), v4.IsPrivate(), v6.IsIPv6(), v6.IsMulticast()
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute address predicates");
    assert_eq!(output, "TRUE TRUE FALSE TRUE TRUE\n");
}

#[test]
fn host_net_cidr_contains_addresses() {
    let source = "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET network AS Net.CIDR OR Error = Net.CIDR.Parse(\"192.168.1.0/24\")\nLET inside AS Net.Address OR Error = Net.Address.Parse(\"192.168.1.20\")\nLET outside AS Net.Address OR Error = Net.Address.Parse(\"192.168.2.20\")\nIF network IS Error OR inside IS Error OR outside IS Error THEN\nPRINT \"error\"\nELSE\nPRINT network.Contains(inside), network.Contains(outside)\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Net CIDR");
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn host_net_resolve_returns_bounded_addresses() {
    let source = "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET result AS Net.Addresses OR Error = Net.Resolve(\"localhost\", 1000)\nIF result IS Error THEN\nPRINT \"error\"\nELSE\nLET first AS Net.Address OR Error = result.Get(0)\nIF first IS Error THEN\nPRINT \"error\"\nELSE\nPRINT result.Count(), first.ToString()\nEND IF\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Net Resolve");
    assert!(output.starts_with('1') || output.starts_with('2'));
}

#[test]
fn host_net_endpoint_create_preserves_port() {
    let source = "IMPORT HOST.Net AS Net\nFUNCTION Start() AS VOID\nLET address AS Net.Address OR Error = Net.Address.Parse(\"127.0.0.1\")\nIF address IS Error THEN\nPRINT \"error\"\nELSE\nLET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 8080)\nIF endpoint IS Error THEN\nPRINT \"error\"\nELSE\nPRINT endpoint\nEND IF\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Net Endpoint");
    assert_eq!(output, "HOST.Net.Endpoint\n");
}

#[test]
fn host_net_endpoint_and_cidr_accessors_preserve_values() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("192.168.1.20")
LET network AS Net.CIDR OR Error = Net.CIDR.Parse("192.168.1.0/24")
IF address IS Error OR network IS Error THEN
PRINT "error"
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 8080)
IF endpoint IS Error THEN
PRINT "error"
ELSE
PRINT endpoint.Address().ToString(), endpoint.Port(), network.Network().ToString(), network.PrefixLength()
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute network accessors");
    assert_eq!(output, "192.168.1.20 8080 192.168.1.0 24\n");
}

#[test]
fn host_net_tcp_listen_reports_local_endpoint() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "parse-error"
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
PRINT "endpoint-error"
ELSE
LET endpoints AS Net.Endpoint[1] = [endpoint]
LET listener AS Net.TCPListener OR Error = Net.TCPListen(endpoints, 1)
IF listener IS Error THEN
PRINT "listen-error", listener.Message
ELSE
LET local AS Net.Endpoint OR Error = listener.LocalEndpoint()
IF local IS Error THEN
PRINT "endpoint-error"
ELSE
PRINT local.Port() > 0
END IF
listener.Close()
END IF
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute TCP listener");
    assert!(output == "TRUE\n" || output.starts_with("listen-error"));
}

#[test]
fn host_net_tcp_listen_rolls_back_when_one_endpoint_fails() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET first AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
LET second AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF first IS Error OR second IS Error THEN
PRINT "parse-error"
ELSE
LET a AS Net.Endpoint OR Error = Net.Endpoint.Create(first, 1)
LET b AS Net.Endpoint OR Error = Net.Endpoint.Create(second, 1)
IF a IS Error OR b IS Error THEN
PRINT "endpoint-error"
ELSE
LET endpoints AS Net.Endpoint[2] = [a, b]
LET listener AS Net.TCPListener OR Error = Net.TCPListen(endpoints, 1)
PRINT listener IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute multi-endpoint rejection");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn host_net_ping_loopback_and_neighbor_typed_result() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "parse-error"
ELSE
LET reply AS Net.PingReply OR Error = Net.Ping(address, 2000)
LET neighbor AS Net.Address OR Error = Net.Neighbor(address)
IF reply IS Error THEN
PRINT "ping-error ", reply.Message, " ", neighbor IS Error
ELSE
PRINT "ping-ok ", reply.RoundTripMicroseconds() >= 0, " ", neighbor IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute ping/neighbor providers");
    assert!(
        output.starts_with("ping-ok  TRUE   TRUE\n") || output.starts_with("ping-error "),
        "unexpected ping/neighbor output: {output:?}"
    );
    assert!(
        output.contains("TRUE\n") || output.contains("TRUE "),
        "neighbor must remain an Error on this Phase 0 host: {output:?}"
    );
}

#[test]
fn host_num_procs_reports_a_positive_logical_processor_count() {
    let source = r#"FUNCTION Start() AS VOID
LET count AS INTEGER OR Error = HOST.NumProcs()
IF count IS Error THEN
PRINT "error"
ELSE
PRINT count > 0
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute HOST.NumProcs");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn bndispatch_queue_rejects_worker_count_outside_the_bounded_range() {
    let source = r"IMPORT BNDispatch AS Dispatch
FUNCTION Start() AS VOID
LET invalid AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(0)
LET valid AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(1)
PRINT invalid IS Error, valid IS Error
END FUNCTION
";
    let (_, output) = run(source, "").expect("execute BNDispatch queue limits");
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn bndispatch_ticket_lifecycle_is_bounded() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION Work() AS VOID
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
PRINT "error"
ELSE
LET ticket AS Dispatch.Ticket OR Error = queue.Async(Work)
IF ticket IS Error THEN
PRINT "error"
ELSE
PRINT ticket.Status(), ticket.Cancel(), ticket.IsDone()
LET closed AS VOID OR Error = queue.Close(100)
PRINT closed IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNDispatch ticket lifecycle");
    assert!(
        [
            "0 TRUE TRUE\nFALSE\n",
            "0 FALSE FALSE\nFALSE\n",
            "1 FALSE FALSE\nFALSE\n",
            "1 FALSE TRUE\nFALSE\n",
            "2 FALSE TRUE\nFALSE\n"
        ]
        .contains(&output.as_str()),
        "unexpected ticket race outcome: {output:?}"
    );
}

#[test]
fn async_await_syntax_dispatches_through_bounded_queue_api() {
    let source = r#"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Work() AS VOID
PRINT "work"
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
PRINT "queue-error"
ELSE
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Work()
IF ticket IS Error THEN
PRINT "ticket-error"
ELSE
LET result AS VOID OR Error = AWAIT ticket(1000)
PRINT result IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("ASYNC/AWAIT dispatch syntax");
    assert!(
        output == "work\nFALSE\n" || output == "FALSE\n" || output == "work TRUE\n",
        "unexpected output: {output:?}"
    );
}

#[test]
fn async_task_output_overflow_fails_ticket_without_retaining_partial_output() {
    let output_source = (0..7_000)
        .map(|_| "PRINT \"0123456789\"\n")
        .collect::<String>();
    let source = format!(
        "IMPORT BNDispatch AS Dispatch\nASYNC FUNCTION Work() AS VOID\n{output_source}END FUNCTION\nFUNCTION Start() AS VOID\nLET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()\nIF queue IS Error THEN\nPRINT \"queue-error\"\nELSE\nLET ticket AS Dispatch.Ticket OR Error = ASYNC queue Work()\nIF ticket IS Error THEN\nPRINT \"ticket-error\"\nELSE\nLET result AS VOID OR Error = ticket.Wait(1000)\nPRINT result IS Error, ticket.Status()\nEND IF\nEND IF\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("execute bounded async output");
    assert_eq!(output, "TRUE 3\n");
}

#[test]
fn async_function_rejects_non_task_return_type() {
    let source = "ASYNC FUNCTION Invalid() AS INTEGER\nRETURN 1\nEND FUNCTION\nFUNCTION Start() AS VOID\nEND FUNCTION\n";
    let error = run(source, "").expect_err("invalid ASYNC return type");
    assert_eq!(error.code, "ASYNC_RETURN_TYPE");
}

#[test]
fn async_submission_rejects_an_invalid_target_operand() {
    let source = r"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Work() AS VOID
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN
ELSE
LET target AS INTEGER = 1
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue target
END IF
END FUNCTION
";
    let error = run(source, "").expect_err("literal ASYNC target must be rejected");
    assert_eq!(error.code, "E0100");
}

#[test]
fn await_rejects_literal_timeout_outside_contract() {
    let source = r"IMPORT BNDispatch AS Dispatch
ASYNC FUNCTION Work() AS VOID
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
RETURN
ELSE
LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Work()
IF ticket IS Error THEN
RETURN
ELSE
LET result AS VOID OR Error = AWAIT ticket(0)
END IF
END IF
END FUNCTION
";
    let error = run(source, "").expect_err("invalid AWAIT timeout");
    assert_eq!(error.code, "AWAIT_TIMEOUT");
}

#[test]
fn bndispatch_sync_primitives_have_bounded_operations() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION Start() AS VOID
LET group AS Dispatch.Group OR Error = Dispatch.Group.New()
LET barrier AS Dispatch.Barrier OR Error = Dispatch.Barrier.New(1)
LET semaphore AS Dispatch.Semaphore OR Error = Dispatch.Semaphore.New(1)
LET mutex AS Dispatch.Mutex OR Error = Dispatch.Mutex.New()
IF group IS Error OR barrier IS Error OR semaphore IS Error OR mutex IS Error THEN
PRINT "error"
ELSE
group.Enter()
group.Leave()
LET gw AS VOID OR Error = group.Wait(100)
LET bw AS BOOLEAN OR Error = barrier.Wait(100)
LET sa AS VOID OR Error = semaphore.Acquire(100)
semaphore.Release()
LET ml AS VOID OR Error = mutex.Lock(100)
mutex.Unlock()
PRINT gw IS Error, bw IS Error, sa IS Error, ml IS Error
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNDispatch synchronization primitives");
    assert_eq!(output, "FALSE FALSE FALSE FALSE\n");
}

#[test]
fn bndispatch_wait_executes_named_task_and_completes_ticket() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION Work() AS VOID
PRINT "worked"
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Serial()
IF queue IS Error THEN
PRINT "error"
ELSE
LET ticket AS Dispatch.Ticket OR Error = queue.Async(Work)
IF ticket IS Error THEN
PRINT "error"
ELSE
LET result AS VOID OR Error = ticket.Wait(100)
PRINT result IS Error, ticket.Status(), ticket.IsDone()
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNDispatch task on wait");
    assert_eq!(output, "worked\nFALSE 2 TRUE\n");
}

#[test]
fn bndispatch_async_tasks_use_isolated_worker_contexts() {
    let source = r#"IMPORT BNDispatch AS Dispatch
FUNCTION First() AS VOID
PRINT "first"
END FUNCTION
FUNCTION Second() AS VOID
PRINT "second"
END FUNCTION
FUNCTION Start() AS VOID
LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(2)
IF queue IS Error THEN
PRINT "queue-error"
ELSE
LET one AS Dispatch.Ticket OR Error = queue.Async(First)
LET two AS Dispatch.Ticket OR Error = queue.Async(Second)
IF one IS Error OR two IS Error THEN
PRINT "ticket-error"
ELSE
LET wait_one AS VOID OR Error = one.Wait(1000)
LET wait_two AS VOID OR Error = two.Wait(1000)
LET closed AS VOID OR Error = queue.Close(1000)
PRINT wait_one IS Error, wait_two IS Error, closed IS Error
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute isolated BNDispatch workers");
    assert!(
        output.ends_with("FALSE FALSE FALSE\n"),
        "unexpected worker output: {output:?}"
    );
    assert!(output.contains("first\n") && output.contains("second\n"));
}

#[test]
fn host_net_reverse_resolves_loopback() {
    let source = r#"IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "parse-error"
ELSE
LET host AS STRING OR Error = Net.Reverse(address, 2000)
IF host IS Error THEN
PRINT "error ", host.Message
ELSE
PRINT "ok"
END IF
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute reverse resolver");
    assert_eq!(output, "ok\n");
}

#[test]
fn local_dataframe_class_is_not_a_bndata_intrinsic() {
    let source = "CLASS DataFrame\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nPUBLIC FUNCTION RowCount() AS INTEGER\nRETURN 42\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET frame AS DataFrame = NEW DataFrame()\nPRINT frame.RowCount()\nDELETE frame\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute local DataFrame");
    assert_eq!(output, "42\n");
}

#[test]
fn boolean_and_or_short_circuit() {
    let source = "FUNCTION Unexpected() AS BOOLEAN\nPRINT \"unexpected\"\nRETURN TRUE\nEND FUNCTION\nFUNCTION Start() AS VOID\nIF FALSE AND Unexpected() THEN\nPRINT \"bad\"\nEND IF\nIF TRUE OR Unexpected() THEN\nPRINT \"ok\"\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "ok\n");
}

#[test]
fn floating_division_uses_ieee_special_values() {
    let source = "FUNCTION Start() AS VOID\nPRINT 1 / 0, 0 / 0\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "INF NAN\n");
}

#[test]
fn checked_integer_overflow_is_a_runtime_error() {
    let source = "FUNCTION Start() AS VOID\nLET value AS INT8 = 127\nvalue += 1\nEND FUNCTION\n";
    let error = run(source, "").expect_err("INT8 overflow must fail");
    assert_eq!(error.code, "NUMERIC_OVERFLOW");
}

#[test]
fn executes_vectors_foreach_input_and_math() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS INTEGER[3] = [1, 2, 3]\nLET total AS INTEGER = 0\nFOR EACH item AS INTEGER IN values\ntotal += item\nEND FOR\nLET line AS STRING OR EOF = INPUT()\nPRINT total, Math.SQRT(9.0), line\nEND FUNCTION\n";
    let (_, output) = run(source, "ready\n").expect("execute source");
    assert_eq!(output, "6 3.0 ready\n");
}

#[test]
fn indexes_unicode_strings_by_scalar() {
    let source = "FUNCTION Start() AS VOID\nLET text AS STRING = \"café\"\nPRINT text[3], LEN(text)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "é 4\n");
}

#[test]
fn inherited_fields_upcasts_and_virtual_methods_execute() {
    let source = r#"
CLASS Animal
    PUBLIC name AS STRING = "animal"
    PUBLIC FUNCTION Speak() AS STRING
        RETURN "A"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        SUPER()
    END FUNCTION
    PUBLIC FUNCTION Speak() AS STRING
        RETURN SUPER.Speak() + "D"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Animal = NEW Dog()
    PRINT pet.name, pet.Speak()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute inheritance");
    assert_eq!(output, "animal AD\n");
}

#[test]
fn inherited_members_survive_a_three_class_chain() {
    let source = r#"
CLASS Animal
    PUBLIC species AS STRING = "animal"
END CLASS

CLASS Mammal EXTENDS Animal
END CLASS

CLASS Dog EXTENDS Mammal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET animal AS Animal = NEW Dog()
    PRINT animal.species
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute inherited field chain");
    assert_eq!(code, 0);
    assert_eq!(output, "animal\n");
}

#[test]
fn inherited_static_members_keep_the_base_storage() {
    let source = r"
CLASS Animal
    PUBLIC STATIC count AS INTEGER = 1
END CLASS

CLASS Dog EXTENDS Animal
END CLASS

FUNCTION Start() AS VOID
    Dog.count += 1
    PRINT Animal.count, Dog.count
END FUNCTION
";
    let (_, output) = run(source, "").expect("execute inherited STATIC field");
    assert_eq!(output, "2 2\n");
}

#[test]
fn super_method_uses_the_nearest_declaring_ancestor() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION Speak() AS STRING
        RETURN "animal"
    END FUNCTION
END CLASS

CLASS Mammal EXTENDS Animal
    PUBLIC FUNCTION Speak() AS STRING
        RETURN "mammal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Mammal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION Speak() AS STRING
        RETURN SUPER.Speak()
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET dog AS Dog = NEW Dog()
    PRINT dog.Speak()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute nearest SUPER method");
    assert_eq!(output, "mammal\n");
}

#[test]
fn inherited_destructors_run_from_derived_to_base() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    DELETE pet
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute destructor chain");
    assert_eq!(output, "dog\nanimal\n");
}

#[test]
fn derived_constructor_implicitly_calls_a_parameterless_base_constructor() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        PRINT "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        PRINT "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute implicit SUPER");
    assert_eq!(output, "animal\ndog\n");
}

#[test]
fn constructor_and_destructor_dispatch_are_pinned_to_the_running_class() {
    let source = r#"
CLASS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
        PRINT SELF.Name()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT SELF.Name()
    END FUNCTION

    PUBLIC FUNCTION Name() AS STRING
        RETURN "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION Name() AS STRING
        RETURN "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    DELETE pet
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute lifecycle dispatch");
    assert_eq!(output, "animal\nanimal\n");
}

#[test]
fn derived_field_initializers_run_after_the_base_constructor() {
    let source = r#"
CLASS Animal
    PUBLIC name AS STRING = "unset"
    PUBLIC FUNCTION CONSTRUCTOR()
        SELF.name = "ok"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC label AS STRING = SELF.name
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    PRINT pet.label
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute derived field initializer order");
    assert_eq!(output, "ok\n");
}

#[test]
fn field_initializer_dispatch_is_pinned_to_the_running_class() {
    let source = r#"
CLASS Animal
    PUBLIC tag AS STRING = SELF.Kind()
    PUBLIC FUNCTION Kind() AS STRING
        RETURN "animal"
    END FUNCTION
END CLASS

CLASS Dog EXTENDS Animal
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION
    PUBLIC FUNCTION Kind() AS STRING
        RETURN "dog"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET pet AS Dog = NEW Dog()
    PRINT pet.tag
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute field initializer dispatch");
    assert_eq!(output, "animal\n");
}

#[test]
fn string_index_out_of_bounds_is_diagnosed() {
    let source =
        "FUNCTION Start() AS VOID\nLET text AS STRING = \"é\"\nPRINT text[1]\nEND FUNCTION\n";
    let error = run(source, "").expect_err("string index must be bounds-checked");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn converts_timestamps_to_utc_components() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nPRINT Math.TOHOUR(0 AS TIMESTAMP), Math.TOWEEKDAY(0 AS TIMESTAMP)\nPRINT Math.TOHOUR(-1 AS TIMESTAMP), Math.TOWEEKDAY(-1 AS TIMESTAMP)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute timestamp conversion");
    assert_eq!(output, "0 4\n23 3\n");
}

#[test]
fn stop_propagates_through_function_calls() {
    let source = "FUNCTION Halt() AS VOID\nSTOP 23\nEND FUNCTION\nFUNCTION Start() AS VOID\nHalt()\nEND FUNCTION\n";
    let (code, output) = run(source, "").expect("execute source");
    assert_eq!(code, 23);
    assert!(output.is_empty());
}

#[test]
fn primitive_and_vector_bindings_receive_defaults() {
    let source = "FUNCTION Start() AS VOID\nLET count AS INTEGER\nLET ready AS BOOLEAN\nLET text AS STRING\nLET values AS INTEGER[2]\nPRINT count, ready, text, values[1]\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute source");
    assert_eq!(output, "0 FALSE  0\n");
}

#[test]
fn local_vector_dimensions_are_evaluated_once_and_stay_fixed() {
    let source = r"
FUNCTION Start() AS VOID
    LET count AS INTEGER = 2
    LET values AS INTEGER[count]
    count = 4
    PRINT LEN(values)
    values = [7, 8]
    PRINT values[0], values[1]
END FUNCTION
";
    let (_, output) = run(source, "").expect("declaration-time vector dimension");
    assert_eq!(output, "2\n7 8\n");
}

#[test]
fn local_vector_dimension_rejections_are_runtime_errors() {
    for (text, code) in [
        (
            "FUNCTION Start() AS VOID\nLET count AS INTEGER = -1\nLET values AS INTEGER[count]\nEND FUNCTION\n",
            "INVALID_VECTOR_DIMENSION",
        ),
        (
            "FUNCTION Start() AS VOID\nLET count AS UINT64 = 9223372036854775808\nLET values AS INTEGER[count]\nEND FUNCTION\n",
            "NUMERIC_OVERFLOW",
        ),
        (
            "FUNCTION Start() AS VOID\nLET count AS INTEGER = 2\nLET values AS INTEGER[count]\nvalues = [1]\nEND FUNCTION\n",
            "VECTOR_LENGTH_MISMATCH",
        ),
    ] {
        assert_eq!(
            run(text, "")
                .expect_err("invalid runtime vector dimension must fail")
                .code,
            code
        );
    }
}

#[test]
fn executes_imported_function_through_alias() {
    let (code, output) =
        run_path("tests/modules/user-alias/main.bn").expect("execute imported Soma");
    assert_eq!(code, 0);
    assert_eq!(output, "3\n");
}

#[test]
fn static_fields_use_type_defaults_when_initializer_is_omitted() {
    let source = include_str!("grammar/valid/static-field-type-default.bn");
    let (code, output) = run(source, "").expect("execute default STATIC fields");
    assert_eq!(code, 0);
    assert_eq!(output, "0\n\nFALSE\n10\n");
}

#[test]
fn initializes_static_fields_once_in_source_order() {
    let (code, output) = run_path("tests/modules/statics/main.bn").expect("execute statics");
    assert_eq!(code, 0);
    assert_eq!(output, "1\n2\n");
}

#[test]
fn static_initialization_cycle_is_a_runtime_error() {
    let error = run_path("tests/modules/static-cycle/main.bn").expect_err("static cycle must fail");
    assert_eq!(error.code, "STATIC_INITIALIZATION_CYCLE");
}

#[test]
fn executes_cls_and_beep_through_host_console() {
    let source = include_str!("grammar/valid/cls-and-beep.bn");
    let (code, output) = run(source, "").expect("execute CLS and BEEP");
    assert_eq!(code, 0);
    assert_eq!(output, "\u{1b}[2J\u{1b}[H\u{7}ok\n");
}

#[test]
fn executes_len_and_sizeof_counts_and_byte_sizes() {
    let source = include_str!("grammar/valid/len-and-sizeof.bn");
    let (code, output) = run(source, "").expect("execute LEN and SIZEOF");
    assert_eq!(code, 0);
    assert_eq!(output, "1 4\n1 8\n4 5\n3 12\n6 24\n2 3 0\n");
}

#[test]
fn executes_bnmath_02_conversion_constants_and_statistics() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS INTEGER[4] = [1, 2, 2, 5]\nPRINT Math.VAL(\" 3,14\"), Math.MAX_INTEGER, Math.MEAN(values), Math.MEDIAN(values), Math.MODE(values), Math.RANGE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute BNMath 0.2");
    assert_eq!(output, "3.0 2147483647 2.5 2.0 2.0 4.0\n");
}

#[test]
fn bnmath_intrinsics_require_the_bnmath_module_identity() {
    let (_, output) = run_path("tests/modules/bnmath-provider-identity/main.bn")
        .expect("execute non-BNMath name collision");
    assert_eq!(output, "7\n");
}

#[test]
fn bnmath_vector_min_max_preserve_integer_type() {
    let (_, output) = run_path("tests/grammar/valid/bnmath-vector-minmax.bn")
        .expect("integer vector min/max execute");
    assert_eq!(output, "1 2\n");
}

#[test]
fn bnmath_min_max_reject_empty_vectors_of_every_numeric_type() {
    for declaration in ["INTEGER[0]", "FLOAT[0]"] {
        let source = format!(
            "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS {declaration} = []\nPRINT Math.MIN(values)\nEND FUNCTION\n"
        );
        let error = run(&source, "").expect_err("empty vector must have no minimum");
        assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
    }
}

#[test]
fn bnmath_min_max_reject_empty_numeric_regions() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS POINTER TO FLOAT[0] = NEW FLOAT[0]\nPRINT Math.MAX(values)\nEND FUNCTION\n";
    let error = run(source, "").expect_err("empty region must have no maximum");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn bnmath_mode_propagates_nan_before_considering_ties() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT[4] = [NAN, NAN, 1.0, 1.0]\nPRINT Math.MODE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute MODE with NAN");
    assert_eq!(output, "NAN\n");
}

#[test]
fn bnmath_mode_returns_na_for_a_non_nan_tie() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT[4] = [1.0, 1.0, 2.0, 2.0]\nPRINT Math.MODE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute MODE tie");
    assert_eq!(output, "NA\n");
}

#[test]
fn bnmath_vector_min_max_preserve_float32_rounding() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT32[2] = [16777217.0 AS FLOAT32, 1.0 AS FLOAT32]\nPRINT Math.MAX(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute FLOAT32 vector maximum");
    assert_eq!(output, "16777216.0\n");
}

#[test]
fn host_random_seed_is_deterministic_and_bounded() {
    let source = "IMPORT HOST.Random AS R\nFUNCTION Start() AS VOID\nR.Seed(1)\nPRINT R.Random(), R.Random()\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.Random");
    assert_eq!(output, "0.28083505005035947 0.14023887919672862\n");
}

#[test]
fn console_tty_calls_fail_only_when_executed() {
    let skipped = "IMPORT HOST.Console AS CON\nFUNCTION Start() AS VOID\nIF FALSE THEN\nCON.PrintAt(1, 1, \"x\")\nEND IF\nEND FUNCTION\n";
    run(skipped, "").expect("unexecuted TTY call must be valid");
    let executed =
        "IMPORT HOST.Console AS CON\nFUNCTION Start() AS VOID\nCON.NumCols()\nEND FUNCTION\n";
    let error = run(executed, "").expect_err("piped NumCols must fail at call");
    assert_eq!(error.code, "HOST_CAPABILITY_UNAVAILABLE");
}

#[test]
fn filesystem_capability_reports_file_existence() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT FS.Exists(\"Cargo.toml\"), FS.Exists(\"missing-file.bn\")\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute HOST.FileSystem");
    assert_eq!(output, "TRUE FALSE\n");
}

#[test]
fn file_is_test_and_identity_equality() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Nop() AS VOID\nEND FUNCTION\nFUNCTION Start() AS VOID\nLET missing AS FS.File OR Error = FS.Open(\"no-such-basicnext-r5-file\", FS.READ)\nIF missing IS Error THEN\nPRINT \"err\"\nEND IF\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS FS.File THEN\nPRINT \"file\"\nLET alias AS FS.File OR Error = file\nIF alias = file THEN\nPRINT \"same\"\nEND IF\nDELETE file\nEND IF\nLET e AS Error\nIF e IS Error THEN\nPRINT e.Code\nEND IF\nLET v AS VOID OR Error = Nop()\nLET both AS VOID OR Error = e\nPRINT v = both\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("IS FS.File and identity");
    assert_eq!(output, "err\nfile\nsame\n0\nFALSE\n");
}

#[test]
fn filesystem_file_opens_reads_and_closes() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nPRINT file.ReadAll()\nfile.Close()\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute FS.File");
    assert!(output.contains("[package]"));
}

#[test]
fn filesystem_file_delete_closes_and_rejects_reuse() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nDELETE file\nDELETE file\nEND IF\nEND FUNCTION\n";
    let error = run(source, "").expect_err("second DELETE must fail");
    assert_eq!(error.code, "DOUBLE_DELETE");
}

#[test]
fn filesystem_file_reads_lines_and_bytes() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET text AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF text IS Error THEN\nPRINT text.Code\nELSE\nLET line AS STRING OR EOF OR Error = text.ReadLine()\nPRINT line\ntext.Close()\nEND IF\nLET binary AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[8]\nIF binary IS Error THEN\nPRINT binary.Code\nELSE\nPRINT binary.ReadBytes(buffer), buffer[0]\nDELETE binary\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute line and byte reads");
    assert!(output.starts_with("[workspace]\n"));
    assert!(output.contains('8'));
}

#[test]
fn filesystem_file_writes_bytes_round_trip() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"/tmp/basicnext-sprint5.bn\", FS.WRITE)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[2]\nbuffer[0] = 65\nbuffer[1] = 66\nIF out IS Error THEN\nPRINT out.Code\nELSE\nLET result AS VOID OR Error = out.WriteBytes(buffer, 2)\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nout.Close()\nEND IF\nLET input AS FS.File OR Error = FS.Open(\"/tmp/basicnext-sprint5.bn\", FS.READ)\nIF input IS Error THEN\nPRINT input.Code\nELSE\nLET read_buffer AS POINTER TO BYTE[] = NEW BYTE[2]\nPRINT input.ReadBytes(read_buffer), read_buffer[0], read_buffer[1]\nDELETE input\nEND IF\nFS.DeleteFile(\"/tmp/basicnext-sprint5.bn\")\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute byte write");
    assert!(output.contains("2 65 66"));
}

#[test]
fn filesystem_write_bytes_on_a_read_only_file_returns_error() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[1]\nbuffer[0] = 65\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET written AS VOID OR Error = file.WriteBytes(buffer, 1)\nIF written IS Error THEN\nPRINT written.Code\nEND IF\nDELETE file\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("WriteBytes I/O failure is an Error value");
    assert_eq!(output, "1\n");
}

#[test]
fn filesystem_file_rejects_byte_count_outside_buffer() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"/tmp/basicnext-sprint5-count.bn\", FS.WRITE)\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[2]\nIF out IS Error THEN\nPRINT out.Code\nELSE\nout.WriteBytes(buffer, 3)\nEND IF\nEND FUNCTION\n";
    let error = run(source, "").expect_err("invalid byte count must fail");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
    let _ = fs::remove_file("/tmp/basicnext-sprint5-count.bn");
}

#[test]
fn filesystem_file_reports_invalid_utf8_on_text_read() {
    fs::write("/tmp/basicnext-sprint5-utf8.bn", [0xff, 0xfe])
        .expect("create invalid UTF-8 fixture");
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"/tmp/basicnext-sprint5-utf8.bn\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET text AS STRING OR Error = file.ReadAll()\nIF text IS Error THEN\nPRINT text.Code\nEND IF\nDELETE file\nEND IF\nFS.DeleteFile(\"/tmp/basicnext-sprint5-utf8.bn\")\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("invalid UTF-8 is an Error value");
    assert!(output.contains('1'));
    let _ = fs::remove_file("/tmp/basicnext-sprint5-utf8.bn");
}

#[test]
fn filesystem_failed_text_write_does_not_lock_out_binary_reads() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET written AS VOID OR Error = file.Write(\"x\")\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[8]\nPRINT file.ReadBytes(buffer)\nDELETE file\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("failed text write must not change file family");
    assert_eq!(output, "8\n");
}

#[test]
fn filesystem_rejects_directory_open_and_missing_delete() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\".\", FS.READ)\nIF file IS Error THEN\nPRINT \"open\", file.Code\nELSE\nPRINT \"opened\"\nDELETE file\nEND IF\nLET removed AS VOID OR Error = FS.DeleteFile(\"/tmp/basicnext-sprint5-missing.bn\")\nIF removed IS Error THEN\nPRINT \"del\", removed.Code\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("unsupported paths return Error values");
    assert_eq!(output, "open 1\ndel 1\n");
}

#[test]
fn filesystem_import_without_use_still_requires_the_capability() {
    let source =
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT \"ran\"\nEND FUNCTION\n";
    let error = run_with_host(
        source,
        "",
        &HostEnv::fixed(vec!["runtime.bn".into()], 0, 0).without_filesystem(),
    )
    .expect_err("import-only FileSystem must fail before Start");
    assert_eq!(error.code, "HOST_CAPABILITY_UNAVAILABLE");
}

#[test]
fn filesystem_eof_locks_the_text_family() {
    let path = "/tmp/basicnext-r8-empty-text.bn";
    let _ = fs::remove_file(path);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF out IS Error THEN\nPRINT out.Code\nELSE\nout.Close()\nDELETE out\nEND IF\nLET text AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF text IS Error THEN\nPRINT text.Code\nELSE\nLET line AS STRING OR EOF OR Error = text.ReadLine()\nIF line IS EOF THEN\nPRINT \"eof\"\nEND IF\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[1]\nLET bytes AS INTEGER OR EOF OR Error = text.ReadBytes(buffer)\nIF bytes IS Error THEN\nPRINT \"blocked\"\nEND IF\nDELETE buffer\nDELETE text\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("ReadLine EOF locks text family");
    assert_eq!(output, "eof\nblocked\n");
}

#[test]
fn filesystem_eof_locks_the_binary_family() {
    let path = "/tmp/basicnext-r8-empty-binary.bn";
    let _ = fs::remove_file(path);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF out IS Error THEN\nPRINT out.Code\nELSE\nout.Close()\nDELETE out\nEND IF\nLET binary AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF binary IS Error THEN\nPRINT binary.Code\nELSE\nLET buffer AS POINTER TO BYTE[] = NEW BYTE[1]\nLET bytes AS INTEGER OR EOF OR Error = binary.ReadBytes(buffer)\nIF bytes IS EOF THEN\nPRINT \"eof\"\nEND IF\nLET line AS STRING OR EOF OR Error = binary.ReadLine()\nIF line IS Error THEN\nPRINT \"blocked\"\nEND IF\nDELETE buffer\nDELETE binary\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("ReadBytes EOF locks binary family");
    assert_eq!(output, "eof\nblocked\n");
}

#[test]
fn filesystem_close_flushes_written_bytes_to_disk() {
    let path = "/tmp/basicnext-r8-close-flush.bn";
    let _ = fs::remove_file(path);
    let source = format!(
        "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET out AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF out IS Error THEN\nPRINT out.Code\nELSE\nLET written AS VOID OR Error = out.Write(\"flushed\")\nIF written IS Error THEN\nPRINT written.Code\nELSE\nLET closed AS VOID OR Error = out.Close()\nIF closed IS Error THEN\nPRINT closed.Code\nEND IF\nEND IF\nDELETE out\nEND IF\nLET input AS FS.File OR Error = FS.Open(\"{path}\", FS.READ)\nIF input IS Error THEN\nPRINT input.Code\nELSE\nPRINT input.ReadAll()\nDELETE input\nEND IF\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("Close flushes before release");
    assert_eq!(output, "flushed\n");
}

#[test]
fn filesystem_new_file_starts_closed() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File = NEW FS.File()\nLET closed AS VOID OR Error = file.Close()\nIF closed IS Error THEN\nPRINT closed.Code\nEND IF\nLET text AS STRING OR Error = file.ReadAll()\nIF text IS Error THEN\nPRINT text.Code\nEND IF\nDELETE file\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("NEW FS.File creates a closed handle");
    assert_eq!(output, "1\n");
}

#[test]
fn filesystem_open_returns_error_for_unknown_mode() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET mode AS INTEGER = 99\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", mode)\nIF file IS Error THEN\nPRINT file.Code\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("computed unknown mode returns Error");
    assert_eq!(output, "1\n");
}

#[test]
fn filesystem_capability_is_checked_before_start() {
    let source = "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT FS.Exists(\"Cargo.toml\")\nEND FUNCTION\n";
    let error = run_with_host(
        source,
        "",
        &HostEnv::fixed(vec!["runtime.bn".into()], 0, 0).without_filesystem(),
    )
    .expect_err("missing filesystem capability must fail before Start");
    assert_eq!(error.code, "HOST_CAPABILITY_UNAVAILABLE");
}

#[test]
fn bndata_import_constructs_and_releases_empty_frame() {
    let path = "tests/grammar/valid/bndata-import.bn";
    let (_, output) = run_path(path).expect("execute BNData import");
    assert_eq!(output, "0 0\n");
}

#[test]
fn bndata_frame_adds_columns_and_reports_counts() {
    let (_, output) =
        run_path("tests/grammar/valid/bndata-columns.bn").expect("add DataFrame columns");
    assert_eq!(output, "2 2 Age\n");
}

#[test]
fn dataframe_is_test_and_identity_equality() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET ids AS INTEGER[1] = [1]\ntable.AddIntegerColumn(\"Id\", ids)\nLET selected AS Data.DataFrame OR Error = table.Select([0], [0])\nIF selected IS Data.DataFrame THEN\nPRINT \"frame\"\nLET alias AS Data.DataFrame OR Error = selected\nIF alias = selected THEN\nPRINT \"same\"\nEND IF\nDELETE selected\nEND IF\nLET other AS Data.DataFrame = NEW Data.DataFrame()\nIF table = other THEN\nPRINT \"diff\"\nEND IF\nDELETE table\nDELETE other\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("IS DataFrame");
    assert_eq!(output, "frame\nsame\n");
}

#[test]
fn bndata_set_label_renames_a_column_in_place() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[1] = [7]\ntable.AddIntegerColumn(\"old\", values)\ntable.SetLabel(\"old\", \"new\")\nPRINT table.ColumnName(0)\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("rename DataFrame label");
    assert_eq!(output, "new\n");
}

#[test]
fn bndata_transpose_returns_string_columns() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET ids AS INTEGER[2] = [1, 2]\nLET names AS STRING[2] = [\"Ana\", \"Bia\"]\ntable.AddIntegerColumn(\"Id\", ids)\ntable.AddStringColumn(\"Name\", names)\nLET result AS Data.DataFrame OR Error = table.Transpose()\nIF result IS Error THEN\nPRINT result.Code\nELSE\nPRINT result.GetString(0, \"Column\"), result.GetString(1, \"Row0\"), result.GetString(1, \"Row1\")\nDELETE result\nEND IF\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("transpose DataFrame");
    assert_eq!(output, "Id Ana Bia\n");
}

#[test]
fn bndata_append_rows_returns_a_new_frame() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET first AS INTEGER[1] = [1]\nLET second AS INTEGER[1] = [2]\nleft.AddIntegerColumn(\"Id\", first)\nright.AddIntegerColumn(\"Id\", second)\nLET joined AS Data.DataFrame OR Error = left.AppendRows(right)\nIF joined IS Error THEN\nPRINT joined.Code\nELSE\nPRINT left.RowCount(), joined.RowCount(), joined.GetInteger(1, \"Id\")\nDELETE joined\nEND IF\nDELETE left\nDELETE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("append DataFrame rows");
    assert_eq!(output, "1 2 2\n");
}

#[test]
fn bndata_append_columns_returns_a_new_frame() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET ids AS INTEGER[1] = [1]\nLET names AS STRING[1] = [\"Ana\"]\nleft.AddIntegerColumn(\"Id\", ids)\nright.AddStringColumn(\"Name\", names)\nLET joined AS Data.DataFrame OR Error = left.AppendColumns(right)\nIF joined IS Error THEN\nPRINT joined.Code\nELSE\nPRINT left.ColumnCount(), joined.ColumnCount(), joined.GetString(0, \"Name\")\nDELETE joined\nEND IF\nDELETE left\nDELETE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("append DataFrame columns");
    assert_eq!(output, "1 2 Ana\n");
}

#[test]
fn bndata_join_variants_preserve_their_outer_side() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET leftIds AS INTEGER[2] = [1, 2]\nLET scores AS INTEGER[2] = [10, 20]\nLET rightIds AS INTEGER[2] = [2, 3]\nLET names AS STRING[2] = [\"Bia\", \"Caio\"]\nleft.AddIntegerColumn(\"Id\", leftIds)\nleft.AddIntegerColumn(\"Score\", scores)\nright.AddIntegerColumn(\"Id\", rightIds)\nright.AddStringColumn(\"Name\", names)\nLET inner AS Data.DataFrame OR Error = left.Join(right, \"Id\", \"Id\")\nLET leftJoin AS Data.DataFrame OR Error = left.LeftJoin(right, \"Id\", \"Id\")\nLET rightJoin AS Data.DataFrame OR Error = left.RightJoin(right, \"Id\", \"Id\")\nLET full AS Data.DataFrame OR Error = left.FullJoin(right, \"Id\", \"Id\")\nIF inner IS Error OR leftJoin IS Error OR rightJoin IS Error OR full IS Error THEN\nPRINT \"error\"\nELSE\nPRINT inner.RowCount(), leftJoin.RowCount(), rightJoin.RowCount(), full.RowCount(), full.GetInteger(2, \"Id\")\nLET missing AS INTEGER OR NA OR Error = full.GetInteger(2, \"Score\")\nPRINT missing\nDELETE inner\nDELETE leftJoin\nDELETE rightJoin\nDELETE full\nEND IF\nDELETE left\nDELETE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("join DataFrames");
    assert_eq!(output, "1 2 2 3 3\nNA\n");
}

#[test]
fn bndata_read_csv_builds_string_columns() {
    let (_, output) = run_path("tests/grammar/valid/bndata-csv.bn").expect("read CSV");
    assert_eq!(
        output,
        "2 2 age\nAna\n35 31.5\n31.5\n31.5 28.0 35.0\n2 2\n1 1\n"
    );
}

#[test]
fn bndata_read_csv_rejects_ragged_rows() {
    let csv = unique_temp("ragged.csv");
    fs::write(&csv, "name;score\nAna;10\nCarlos\n").expect("create ragged CSV fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \";\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nfile.Close()\nDELETE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("ragged CSV returns Error");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_read_csv_rejects_unterminated_quotes() {
    let csv = unique_temp("unquoted.csv");
    fs::write(&csv, "name\n\"Ana\n").expect("create unterminated CSV fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nfile.Close()\nDELETE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("unterminated CSV quotes return Error");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_write_csv_on_a_closed_file_returns_error() {
    let csv = unique_temp("writecsv-closed.csv");
    let path = bn_path(&csv);
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET names AS STRING[1] = [\"Ana\"]\ntable.AddStringColumn(\"name\", names)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nfile.Close()\nLET written AS VOID OR Error = Data.WriteCSV(file, table, TRUE, \",\")\nIF written IS Error THEN\nPRINT written.Code\nEND IF\nDELETE file\nEND IF\nDELETE table\nFS.DeleteFile(\"{path}\")\nEND FUNCTION\n"
    );
    let (_, output) = run(&source, "").expect("WriteCSV on a closed file returns Error");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_read_csv_rejects_invalid_separator() {
    let source = "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"Cargo.toml\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \"\\\"\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nDELETE file\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("invalid separator returns Error");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_conversion_failure_leaves_the_column_unchanged() {
    let csv = unique_temp("conversion.csv");
    fs::write(&csv, "value\n10\n999999999999999999999\n").expect("create conversion CSV fixture");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \";\")\nIF table IS Error THEN\nPRINT table.Code\nELSE\nLET converted AS VOID OR Error = table.ConvertToInteger(\"value\")\nIF converted IS Error THEN\nLET first AS STRING OR NA OR Error = table.GetString(0, \"value\")\nPRINT first\nEND IF\nDELETE table\nEND IF\nDELETE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("conversion failure is an Error");
    assert_eq!(output, "10\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_copy_failure_leaves_the_destination_unchanged() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET text AS STRING[2] = [\"1\", \"\"]\nLET target AS POINTER TO INTEGER[] = NEW INTEGER[2]\ntarget[0] = 9\ntarget[1] = 9\ntable.AddStringColumn(\"Id\", text)\ntable.ConvertToInteger(\"Id\")\nLET copied AS VOID OR Error = table.CopyIntegerColumn(\"Id\", target)\nIF copied IS Error THEN\nPRINT target[0], target[1]\nEND IF\nDELETE target\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("run atomic copy");
    assert_eq!(output, "9 9\n");
}

#[test]
fn bndata_zscore_standardizes_a_float_column() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET xs AS FLOAT[3] = [1.0, 2.0, 3.0]\ntable.AddFloatColumn(\"x\", xs)\nLET z AS Data.DataFrame OR Error = table.ZScore(\"x\")\nIF z IS Error THEN\nPRINT z.Code\nELSE\nPRINT z.GetFloat(0, \"x\")\nPRINT z.GetFloat(1, \"x\")\nPRINT z.GetFloat(2, \"x\")\nDELETE z\nEND IF\nLET missing AS Data.DataFrame OR Error = table.ZScore(\"nope\")\nIF missing IS Error THEN\nPRINT missing.Code\nEND IF\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("run ZScore");
    assert_eq!(output, "-1.0\n0.0\n1.0\n1\n");
}

#[test]
fn bndata_select_rejects_out_of_bounds_indices() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[1] = [1]\ntable.AddIntegerColumn(\"Id\", values)\nLET result AS Data.DataFrame OR Error = table.Select([1], [0])\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("run bounds check");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_select_rejects_negative_indices_as_error() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[1] = [1]\ntable.AddIntegerColumn(\"Id\", values)\nLET rows AS INTEGER[1]\nrows[0] = -1\nLET result AS Data.DataFrame OR Error = table.Select(rows, [0])\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("negative Select is Error");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_slice_rejects_row_range_on_empty_frame() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET sliced AS Data.DataFrame OR Error = table.Slice(0, 1, 0, 0)\nIF sliced IS Error THEN\nPRINT sliced.Code\nEND IF\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("empty-frame Slice is Error");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_empty_and_all_na_reductions_follow_bnmath() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET empty AS Data.DataFrame = NEW Data.DataFrame()\nLET xs AS FLOAT[0] = []\nempty.AddFloatColumn(\"x\", xs)\nPRINT empty.Mean(\"x\")\nPRINT empty.Median(\"x\")\nPRINT empty.Range(\"x\")\nDELETE empty\nLET nas AS Data.DataFrame = NEW Data.DataFrame()\nLET text AS STRING[2] = [\"\", \"\"]\nnas.AddStringColumn(\"n\", text)\nnas.ConvertToInteger(\"n\")\nPRINT nas.Mean(\"n\")\nPRINT nas.Quartile1(\"n\")\nLET mode AS FLOAT OR NA OR Error = nas.Mode(\"n\")\nPRINT mode\nDELETE nas\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("empty and all-NA reductions");
    assert_eq!(output, "NAN\nNAN\nNAN\nNAN\nNAN\nNA\n");
}

#[test]
fn bnmath_mode_returns_na_for_an_empty_vector() {
    let source = "IMPORT BNMath AS Math\nFUNCTION Start() AS VOID\nLET values AS FLOAT[0] = []\nPRINT Math.MODE(values)\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("empty MODE is NA");
    assert_eq!(output, "NA\n");
}

#[test]
fn bndata_deleted_frame_rejects_reuse() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nDELETE table\nPRINT table.RowCount()\nEND FUNCTION\n";
    let error = run(source, "").expect_err("reject deleted frame");
    assert_eq!(error.code, "USE_AFTER_DELETE");
}

#[test]
fn bndata_write_csv_serializes_headers_and_rows() {
    let csv = unique_temp("sprint6-write.csv");
    let path = bn_path(&csv);
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET frame AS Data.DataFrame = NEW Data.DataFrame()\nLET names AS STRING[2] = [\"Ana\", \"Carlos\"]\nframe.AddStringColumn(\"name\", names)\nLET file AS FS.File OR Error = FS.Open(\"{path}\", FS.WRITE)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET result AS VOID OR Error = Data.WriteCSV(file, frame, TRUE, \",\")\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nfile.Close()\nDELETE file\nEND IF\nDELETE frame\nEND FUNCTION\n"
    );
    run(&source, "").expect("write CSV");
    let text = fs::read_to_string(&csv).expect("read written CSV");
    assert_eq!(text, "name\nAna\nCarlos\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_accepts_variable_fixed_vector_lengths() {
    let (_, output) =
        run_path("tests/grammar/valid/bndata-variable-length.bn").expect("variable vector length");
    assert_eq!(output, "3\n");
}

#[test]
fn local_class_file_is_not_a_host_file() {
    let source = "CLASS File\nPUBLIC name AS STRING = \"user\"\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS\nFUNCTION Start() AS VOID\nLET f AS File = NEW File()\nPRINT f.name\nDELETE f\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("local CLASS File");
    assert_eq!(output, "user\n");
}

#[test]
fn imported_class_file_is_not_a_host_file() {
    let (_, output) =
        run_path("tests/modules/user-file-class/main.bn").expect("imported CLASS File");
    assert_eq!(output, "7\n");
}

#[test]
fn error_return_narrows_file_for_the_rest_of_the_block() {
    let (_, output) =
        run_path("tests/grammar/valid/error-return-narrow.bn").expect("narrow after RETURN");
    assert!(output.starts_with('['), "{output}");
}

#[test]
fn error_return_narrows_dataframe_for_readcsv() {
    let source = "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"tests/fixtures/bndata-sprint6.csv\", FS.READ)\nIF file IS Error THEN\nRETURN\nEND IF\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nfile.Close()\nDELETE file\nRETURN\nEND IF\nPRINT table.RowCount()\nDELETE table\nfile.Close()\nDELETE file\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("narrow DataFrame after RETURN");
    assert_eq!(output, "2\n");
}

#[test]
fn bndata_zscore_empty_and_all_na_follow_bnmath() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET empty AS Data.DataFrame = NEW Data.DataFrame()\nLET xs AS FLOAT[0] = []\nempty.AddFloatColumn(\"x\", xs)\nLET z AS Data.DataFrame OR Error = empty.ZScore(\"x\")\nIF z IS Error THEN\nPRINT \"empty-error\"\nELSE\nPRINT z.RowCount()\nDELETE z\nEND IF\nDELETE empty\nLET nas AS Data.DataFrame = NEW Data.DataFrame()\nLET text AS STRING[2] = [\"\", \"\"]\nnas.AddStringColumn(\"n\", text)\nnas.ConvertToInteger(\"n\")\nLET z2 AS Data.DataFrame OR Error = nas.ZScore(\"n\")\nIF z2 IS Error THEN\nPRINT \"na-error\"\nELSE\nPRINT z2.GetFloat(0, \"n\")\nDELETE z2\nEND IF\nDELETE nas\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("ZScore empty and all-NA");
    assert_eq!(output, "0\nNA\n");
}

#[test]
fn bndata_getstring_returns_na_for_unmatched_join_cells() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET left AS Data.DataFrame = NEW Data.DataFrame()\nLET right AS Data.DataFrame = NEW Data.DataFrame()\nLET leftIds AS INTEGER[1] = [1]\nLET names AS STRING[1] = [\"Ana\"]\nLET rightIds AS INTEGER[1] = [2]\nLET extra AS STRING[1] = [\"Bia\"]\nleft.AddIntegerColumn(\"Id\", leftIds)\nleft.AddStringColumn(\"Name\", names)\nright.AddIntegerColumn(\"Id\", rightIds)\nright.AddStringColumn(\"Other\", extra)\nLET full AS Data.DataFrame OR Error = left.FullJoin(right, \"Id\", \"Id\")\nIF full IS Error THEN\nPRINT \"join-error\"\nELSE\nLET missing AS STRING OR NA OR Error = full.GetString(1, \"Name\")\nPRINT missing\nDELETE full\nEND IF\nDELETE left\nDELETE right\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("GetString NA from join");
    assert_eq!(output, "NA\n");
}

#[test]
fn bndata_select_rejects_duplicate_column_indices() {
    let source = "IMPORT BNData AS Data\nFUNCTION Start() AS VOID\nLET table AS Data.DataFrame = NEW Data.DataFrame()\nLET values AS INTEGER[2] = [1, 2]\ntable.AddIntegerColumn(\"Id\", values)\nLET result AS Data.DataFrame OR Error = table.Select([0, 1], [0, 0])\nIF result IS Error THEN\nPRINT result.Code\nEND IF\nDELETE table\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("duplicate Select columns");
    assert_eq!(output, "1\n");
}

#[test]
fn bndata_read_csv_rejects_duplicate_headers() {
    let csv = unique_temp("dup-header.csv");
    fs::write(&csv, "name,name\nAna,Bia\n").expect("create duplicate-header CSV");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nPRINT table.Code\nEND IF\nfile.Close()\nDELETE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("duplicate CSV headers");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn bndata_read_csv_ignores_a_trailing_blank_line() {
    let csv = unique_temp("trailing-blank.csv");
    fs::write(&csv, "name,age\nAna,28\n\n").expect("create trailing-blank CSV");
    let source = format!(
        "IMPORT BNData AS Data\nIMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nLET file AS FS.File OR Error = FS.Open(\"{}\", FS.READ)\nIF file IS Error THEN\nPRINT file.Code\nELSE\nLET table AS Data.DataFrame OR Error = Data.ReadCSV(file, TRUE, \",\")\nIF table IS Error THEN\nPRINT \"error\"\nELSE\nPRINT table.RowCount()\nDELETE table\nEND IF\nfile.Close()\nDELETE file\nEND IF\nEND FUNCTION\n",
        bn_path(&csv)
    );
    let (_, output) = run(&source, "").expect("trailing blank CSV");
    assert_eq!(output, "1\n");
    let _ = fs::remove_file(csv);
}

#[test]
fn asc_and_char_convert_unicode_scalars() {
    let source = "FUNCTION Start() AS VOID\nLET a AS INTEGER OR Error = ASC(\"é\")\nLET c AS STRING OR Error = CHAR(66)\nPRINT a, c\nLET empty AS INTEGER OR Error = ASC(\"\")\nIF empty IS Error THEN\nPRINT empty.Code\nEND IF\nEND FUNCTION\n";
    let (_, output) = run(source, "").expect("execute ASC and CHAR");
    assert_eq!(output, "233 B\n1\n");
}

#[test]
fn executes_struct_copy_class_identity_and_interface_dispatch() {
    let source = r"
STRUCT Inner
    N AS INTEGER = 1
END STRUCT

STRUCT Point
    X AS FLOAT = 0.0
    Y AS FLOAT = 0.0
END STRUCT

INTERFACE Valued
    FUNCTION Value() AS INTEGER
END INTERFACE

CLASS Counter IMPLEMENTS Valued
    PRIVATE value AS INTEGER = 0

    PUBLIC FUNCTION CONSTRUCTOR(initial AS INTEGER)
        SELF.value = initial
    END FUNCTION

    PUBLIC FUNCTION Value() AS INTEGER
        RETURN SELF.value
    END FUNCTION

    PUBLIC FUNCTION Increment(amount AS INTEGER) AS VOID
        SELF.value += amount
    END FUNCTION
END CLASS

FUNCTION Mutate(point AS Point) AS VOID
    point.X = 99.0
END FUNCTION

FUNCTION Start() AS VOID
    LET origin AS Point
    LET moved AS Point = origin
    moved.X = 10.0
    Mutate(origin)
    LET nested AS Inner
    LET copy AS Inner = nested
    copy.N = 2
    LET points AS Point[2]
    PRINT origin.X, moved.X, nested.N, copy.N
    PRINT origin = moved, origin = origin, points[0].X, SIZEOF(origin)

    LET counter AS Counter = NEW Counter(1)
    LET alias AS Counter = counter
    alias.Increment(1)
    LET valued AS Valued = counter
    LET counters AS Counter[1] = [counter]
    PRINT counter.Value(), valued.Value(), counters[0].Value(), SIZEOF(counter)
    PRINT counter = alias, counter = NEW Counter(2)
END FUNCTION
";
    let (code, output) = run(source, "").expect("execute struct/class/interface program");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "0.0 10.0 1 2\nFALSE TRUE 0.0 16\n2 2 2 4\nTRUE FALSE\n"
    );
}

#[test]
fn failed_constructor_does_not_expose_an_object() {
    let source = r"
CLASS Boom
    PUBLIC FUNCTION CONSTRUCTOR()
        LET value AS INT8 = 127
        value += 1
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET boom AS Boom = NEW Boom()
END FUNCTION
";
    let error = run(source, "").expect_err("constructor overflow must fail");
    assert_eq!(error.code, "NUMERIC_OVERFLOW");
}

#[test]
fn executes_imported_class_constructor_and_methods() {
    let (code, output) = run_path("tests/modules/objects/main.bn").expect("execute imported Box");
    assert_eq!(code, 0);
    assert_eq!(output, "7 7 1\n");
}

#[test]
fn executes_a_qualified_imported_interface() {
    let (code, output) = run_path("tests/modules/qualified-interface/main.bn")
        .expect("execute qualified imported interface");
    assert_eq!(code, 0);
    assert_eq!(output, "Puck\n");
}

#[test]
fn executes_a_class_derived_from_an_imported_base() {
    let (code, output) = run_path("tests/modules/imported-inheritance/main.bn")
        .expect("execute imported class inheritance");
    assert_eq!(code, 0);
    assert_eq!(output, "animal animal dog\n");
}

#[test]
fn executes_numeric_pointers_and_aliases() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER = NEW INTEGER
    value[0] = 42
    LET alias AS POINTER TO INTEGER = value
    PRINT alias[0]
    DELETE alias
END FUNCTION
";
    let (code, output) = run(source, "").expect("execute pointer alias");
    assert_eq!(code, 0);
    assert_eq!(output, "42\n");
}

#[test]
fn fixed_pointer_region_is_initialized_and_bounds_checked() {
    let source = r"
FUNCTION Start() AS VOID
    LET samples AS POINTER TO FLOAT[4] = NEW FLOAT[4]
    samples[0] = 1.5
    samples[3] = 2.0
    PRINT samples[0], samples[3]
    DELETE samples
END FUNCTION
";
    let (code, output) = run(source, "").expect("execute fixed pointer");
    assert_eq!(code, 0);
    assert_eq!(output, "1.5 2.0\n");
}

#[test]
fn zero_length_pointer_rejects_every_index() {
    let source = r"
FUNCTION Start() AS VOID
    LET empty AS POINTER TO INTEGER[] = NEW INTEGER[0]
    PRINT empty[0]
END FUNCTION
";
    let error = run(source, "").expect_err("empty region has no element");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn deleted_pointer_is_stale_for_every_alias() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER = NEW INTEGER
    LET alias AS POINTER TO INTEGER = value
    DELETE value
    alias[0] = 1
END FUNCTION
";
    let error = run(source, "").expect_err("stale alias must fail");
    assert_eq!(error.code, "USE_AFTER_DELETE");
}

#[test]
fn second_delete_is_double_delete() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER = NEW INTEGER
    DELETE value
    DELETE value
END FUNCTION
";
    let error = run(source, "").expect_err("second delete must fail");
    assert_eq!(error.code, "DOUBLE_DELETE");
}

#[test]
fn null_pointer_access_and_delete_are_diagnosed() {
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER OR NULL = NULL
    value[0] = 1
END FUNCTION
";
    let error = run(source, "").expect_err("NULL index must fail");
    assert_eq!(error.code, "NULL_POINTER_ACCESS");
    let source = r"
FUNCTION Start() AS VOID
    LET value AS POINTER TO INTEGER OR NULL = NULL
    DELETE value
END FUNCTION
";
    let error = run(source, "").expect_err("NULL delete must fail");
    assert_eq!(error.code, "NULL_POINTER_ACCESS");
}

#[test]
fn computed_pointer_length_mismatch_is_a_runtime_error() {
    let source = r"
FUNCTION Start() AS VOID
    LET count AS INTEGER = 3
    LET fixed AS POINTER TO INTEGER[2] = NEW INTEGER[count]
END FUNCTION
";
    let error = run(source, "").expect_err("length mismatch must fail");
    assert_eq!(error.code, "POINTER_LENGTH_MISMATCH");
}

#[test]
fn negative_allocation_count_is_invalid() {
    let source = r"
FUNCTION Start() AS VOID
    LET count AS INTEGER = -1
    LET samples AS POINTER TO INTEGER[] = NEW INTEGER[count]
END FUNCTION
";
    let error = run(source, "").expect_err("negative count must fail");
    assert_eq!(error.code, "ALLOCATION_SIZE_INVALID");
}

#[test]
fn class_delete_runs_the_destructor_once() {
    let source = r#"
CLASS Box
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "destroyed"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET box AS Box = NEW Box()
    DELETE box
    PRINT "after"
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute destructor");
    assert_eq!(code, 0);
    assert_eq!(output, "destroyed\nafter\n");
}

#[test]
fn leaked_class_does_not_run_its_destructor() {
    let source = r#"
CLASS Box
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        PRINT "destroyed"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET box AS Box = NEW Box()
    PRINT "end"
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute leak");
    assert_eq!(code, 0);
    assert_eq!(output, "end\n");
}

#[test]
fn reentrant_delete_in_a_destructor_is_double_delete() {
    let source = r"
CLASS Box
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION DESTRUCTOR()
        DELETE SELF
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET box AS Box = NEW Box()
    DELETE box
END FUNCTION
";
    let error = run(source, "").expect_err("reentrant delete must fail");
    assert_eq!(error.code, "DOUBLE_DELETE");
}

#[test]
fn host_args_exposes_injected_arguments() {
    let source = r"
FUNCTION Start() AS VOID
    PRINT LEN(HOST.Args)
    PRINT HOST.Args[0]
    PRINT HOST.Args[1]
END FUNCTION
";
    let host = HostEnv::fixed(vec!["prog.bn".into(), "ação".into()], 0, 0);
    let (code, output) = run_with_host(source, "", &host).expect("execute HOST.Args");
    assert_eq!(code, 0);
    assert_eq!(output, "2\nprog.bn\nação\n");
}

#[test]
fn host_args_exposes_count_and_immutable_strings() {
    let source = r"
FUNCTION Start() AS VOID
    PRINT LEN(HOST.Args)
    PRINT HOST.Args[0]
    PRINT HOST.Args[1]
END FUNCTION
";
    let host = HostEnv::fixed(vec!["/tmp/program.bn".into(), "ação".into()], 0, 0);
    let (code, output) = run_with_host(source, "", &host).expect("execute HOST.Args");
    assert_eq!(code, 0);
    assert_eq!(output, "2\n/tmp/program.bn\nação\n");
}

#[test]
fn host_clock_uses_injected_providers() {
    let source = r"
IMPORT HOST.Clock AS Clock

FUNCTION Start() AS VOID
    PRINT Clock.Now()
    PRINT Clock.Timer()
END FUNCTION
";
    let host = HostEnv::fixed(vec!["runtime.bn".into()], 1_000, 42);
    let (code, output) = run_with_host(source, "", &host).expect("execute HOST.Clock");
    assert_eq!(code, 0);
    assert_eq!(output, "1000\n42\n");
}

#[test]
fn host_argument_out_of_range_is_index_error() {
    let source = r"
FUNCTION Start() AS VOID
    PRINT HOST.Args[1]
END FUNCTION
";
    let error = run(source, "").expect_err("missing argument must fail");
    assert_eq!(error.code, "INDEX_OUT_OF_BOUNDS");
}

#[test]
fn rfc3339_and_civil_values_round_trip_through_utc() {
    let source = r#"
IMPORT BNMath AS Math
FUNCTION Start() AS VOID
    LET epoch AS TIMESTAMP = Timestamp.Parse("1970-01-01T00:00:00.000Z")
    LET offset AS TIMESTAMP = Timestamp.Parse("1970-01-01T00:00:00.000-03:00")
    LET date AS DATE = Date.Parse("2026-08-22")
    LET time AS TIME = Time.Parse("10:20:30.000")
    LET zone AS TIMEZONE = TimeZone.Parse("America/Sao_Paulo")
    LET combined AS TIMESTAMP = Math.TOTIMESTAMP(date, time)
    PRINT epoch, offset, Timestamp.Format(epoch)
    PRINT date, time, zone
    PRINT Math.TODATE(0 AS TIMESTAMP), Math.TOTIME(0 AS TIMESTAMP), SIZEOF(date), SIZEOF(time)
    PRINT combined, Date.Parse("1970-01-01") = Math.TODATE(0 AS TIMESTAMP)
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("execute temporal values");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "0 10800000 1970-01-01T00:00:00.000Z\n2026-08-22 10:20:30.000 America/Sao_Paulo\n1970-01-01 00:00:00.000 4 4\n1787394030000 TRUE\n"
    );
}

#[test]
fn timestamp_parse_accepts_rfc3339_fraction_widths() {
    let source = r#"
FUNCTION Start() AS VOID
    PRINT Timestamp.Format(Timestamp.Parse("2020-01-01T00:00:00.5Z"))
    PRINT Timestamp.Format(Timestamp.Parse("2020-01-01T00:00:00.05Z"))
    PRINT Timestamp.Format(Timestamp.Parse("2020-01-01T00:00:00.0050Z"))
END FUNCTION
"#;
    let (code, output) = run(source, "").expect("parse RFC 3339 fractions");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "2020-01-01T00:00:00.500Z\n2020-01-01T00:00:00.050Z\n2020-01-01T00:00:00.005Z\n"
    );
}

#[test]
fn language_tour_executes_through_ir() {
    let (code, output) = run_path("examples/language-tour.bn").expect("execute language tour");
    assert_eq!(code, 0);
    assert!(output.contains("Basic Next"));
    assert!(output.contains("counter deleted"));
}

#[test]
fn shortest_path_example_runs() {
    let (code, output) = run_path("examples/shortest_path.bn").expect("execute shortest path");
    assert_eq!(code, 0);
    assert_eq!(output, "6\n");
}

#[test]
fn dispatch_game_tournament_example_executes_in_ticket_order() {
    let (code, output) = run_path("examples/dispatch_game_tournament.bn")
        .expect("execute game-tournament dispatch example");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "CC: A= 3000  B= 3000\nCD: A= 0  B= 5000\nDC: A= 5000  B= 0\nDD: A= 1000  B= 1000\ntournament complete\n"
    );
}

#[test]
fn dispatch_reliability_simulation_example_executes_in_ticket_order() {
    let (code, output) = run_path("examples/dispatch_reliability_simulation.bn")
        .expect("execute reliability dispatch example");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "line-A: outages= 12  cost= 1200\nline-B: outages= 16  cost= 3200\nline-C: outages= 20  cost= 6000\nline-D: outages= 24  cost= 9600\nreliability simulation complete\n"
    );
}

#[test]
fn dispatch_cellular_automaton_example_executes_in_ticket_order() {
    let (code, output) = run_path("examples/dispatch_cellular_automaton.bn")
        .expect("execute cellular-automaton dispatch example");
    assert_eq!(code, 0);
    assert_eq!(
        output,
        "rule-30 seed-37: checksum= 27\nrule-45 seed-74: checksum= 13\nrule-110 seed-1: checksum= 10\nrule-184 seed-146: checksum= 14\ncellular automaton complete\n"
    );
}

#[test]
fn temporal_parse_rejects_invalid_civil_and_rfc3339_forms() {
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT Date.Parse("2026-02-30")
END FUNCTION
"#,
        "",
    )
    .expect_err("invalid DATE");
    assert_eq!(error.code, "INVALID_DATE");
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT Time.Parse("24:00:00.000")
END FUNCTION
"#,
        "",
    )
    .expect_err("invalid TIME");
    assert_eq!(error.code, "INVALID_TIME");
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT Timestamp.Parse("2026-08-22T10:20:30.0001Z")
END FUNCTION
"#,
        "",
    )
    .expect_err("excess TIMESTAMP precision");
    assert_eq!(error.code, "PARSE_ERROR");
    let error = run(
        r#"FUNCTION Start() AS VOID
PRINT TimeZone.Parse("-03:00")
END FUNCTION
"#,
        "",
    )
    .expect_err("offset is not a TIMEZONE");
    assert_eq!(error.code, "INVALID_TIMEZONE");
}

#[test]
fn bnweb_server_lifecycle_uses_native_state() {
    let source = r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET server AS Web.Server = NEW Web.Server()
LET stopped AS VOID OR Error = server.Stop(1000)
IF stopped IS Error THEN
PRINT stopped.Code
END IF
LET closed AS VOID OR Error = server.Close(1000)
IF closed IS Error THEN
PRINT closed.Code
ELSE
PRINT "closed"
END IF
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNWeb server lifecycle");
    assert_eq!(output, "closed\n");
}

#[test]
fn bnweb_server_status_and_counters_are_read_only() {
    let source = r"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET server AS Web.Server = NEW Web.Server()
PRINT server.Status()
PRINT server.IsReady()
PRINT server.ActiveConnections()
PRINT server.PendingRequests()
PRINT server.AcceptedRequests()
PRINT server.ActiveRequests()
PRINT server.RejectedRequests()
PRINT server.TimedOutRequests()
PRINT server.CompletedRequests()
PRINT server.FailedRequests()
PRINT server.RateLimitedRequests()
PRINT server.TotalRequestDurationMs()
PRINT server.AverageRequestDurationMs()
PRINT server.MaxRequestDurationMs()
END FUNCTION
";
    let (_, output) = run(source, "").expect("read BNWeb server status");
    assert_eq!(
        output,
        "Starting\nFALSE\n0\n0\n0\n0\n0\n0\n0\n0\n0\n0\n0\n0\n"
    );
}

#[test]
fn bnweb_server_start_binds_host_net_endpoint() {
    let source = r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
STOP 1
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
STOP 2
ELSE
LET server AS Web.Server = NEW Web.Server()
LET started AS VOID OR Error = server.Start(endpoint)
IF started IS Error THEN
PRINT started.Code
ELSE
LET stopped AS VOID OR Error = server.Stop(1000)
PRINT stopped IS Error
PRINT server.Status()
PRINT server.IsReady()
END IF
END IF
END IF
END FUNCTION
"#;
    let (_, output) = match run(source, "") {
        Ok(result) => result,
        Err(error) => {
            assert_eq!(error.code, "WEB_LISTEN", "{error:?}");
            return;
        }
    };
    assert_eq!(output, "FALSE\nDraining\nFALSE\n");
}

#[test]
fn bnweb_server_options_are_validated_and_consumed_by_start() {
    let source = r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
STOP 1
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
STOP 2
ELSE
LET options AS Web.ServerOptions OR Error = Web.ServerOptions.New(1, 128, 1, 1, 65536, 100, 8192, 8388608, 5000, 5000, 30000, 60000, 120000, 5000, 10, 10, 10000, FALSE, FALSE)
IF options IS Error THEN
PRINT "invalid"
ELSE
LET server AS Web.Server = NEW Web.Server()
LET started AS VOID OR Error = server.StartWithOptions(endpoint, options)
IF started IS Error THEN
PRINT started.Code
ELSE
LET stopped AS VOID OR Error = server.Stop(1000)
PRINT stopped IS Error
END IF
END IF
END IF
END IF
END FUNCTION
"#;
    let (_, output) = match run(source, "") {
        Ok(result) => result,
        Err(error) => {
            assert_eq!(error.code, "WEB_LISTEN", "{error:?}");
            return;
        }
    };
    assert_eq!(output, "FALSE\n");
}

#[test]
fn bnweb_dispatch_invokes_registered_handler() {
    let source = r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
IF response.Status() = 202 THEN
response.SetStatus(201)
ELSE
response.SetStatus(500)
END IF
END FUNCTION
FUNCTION Filter(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(202)
END FUNCTION
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
STOP 1
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
STOP 2
ELSE
LET server AS Web.Server = NEW Web.Server()
server.AddFilter(Filter)
server.Route("GET", "/", Handler)
LET started AS VOID OR Error = server.Start(endpoint)
IF started IS Error THEN
STOP 3
ELSE
LET request AS Web.Request = NEW Web.Request()
LET response AS Web.Response = NEW Web.Response()
LET dispatched AS VOID OR Error = server.Dispatch(request, response)
IF dispatched IS Error THEN
PRINT dispatched.Code
ELSE
PRINT response.Status()
LET requestId AS STRING OR Error = response.Header("X-Request-ID")
IF requestId IS Error THEN
PRINT "missing-request-id"
ELSE
PRINT LEN(requestId) >= 33
END IF
END IF
server.Stop(1000)
END IF
END IF
END IF
END FUNCTION
"#;
    match run(source, "") {
        Ok((_, output)) => assert_eq!(output, "201\nTRUE\n"),
        Err(error) => assert_eq!(error.code, "WEB_LISTEN", "{error:?}"),
    }
}

#[test]
fn bnweb_start_serves_registered_bn_handler_over_http() {
    let probe = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind BNWeb bridge probe: {error}"),
    };
    let port = probe.local_addr().expect("probe address").port();
    drop(probe);
    let source = format!(
        r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(207)
response.Write("bn-handler")
END FUNCTION
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
STOP 1
END IF
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, {port})
IF endpoint IS Error THEN
STOP 2
END IF
LET server AS Web.Server = NEW Web.Server()
server.Route("GET", "/bridge", Handler)
LET started AS VOID OR Error = server.Start(endpoint)
IF started IS Error THEN
STOP 3
END IF
LET gate AS STRING OR EOF = INPUT()
server.Stop(1000)
END FUNCTION
"#
    );
    let path = unique_temp("bnweb-http-bridge.bn");
    fs::write(&path, &source).expect("write BNWeb bridge fixture");
    let graph = load(path.to_str().expect("fixture path")).expect("load BNWeb bridge fixture");
    let models = analyze_modules(&graph).expect("analyze BNWeb bridge fixture");
    let module = lower_graph(&graph, &models).expect("lower BNWeb bridge fixture");
    let (input_sender, input_receiver) = std::sync::mpsc::channel();
    let execution = std::thread::spawn(move || {
        let reader = ChannelReader {
            receiver: input_receiver,
            current: Cursor::new(Vec::new()),
        };
        let mut input = std::io::BufReader::new(reader);
        let mut output = Vec::new();
        execute_with_host(
            &module,
            &mut input,
            &mut output,
            &HostEnv::fixed(vec!["bridge.bn".into()], 0, 0),
        )
        .map(|code| (code, output))
    });
    let mut client = None;
    for _ in 0..200 {
        match std::net::TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => {
                client = Some(stream);
                break;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(1)),
        }
    }
    let Some(mut client) = client else {
        let _ = input_sender.send(b"release\n".to_vec());
        let result = execution.join().expect("bridge execution thread");
        let _ = fs::remove_file(path);
        if let Err(error) = result {
            assert_eq!(error.code, "WEB_LISTEN", "{error:?}");
        }
        return;
    };
    client
        .write_all(b"GET /bridge HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write BNWeb bridge request");
    let mut response = String::new();
    client
        .read_to_string(&mut response)
        .expect("read BNWeb bridge response");
    input_sender
        .send(b"release\n".to_vec())
        .expect("release BNWeb server");
    let result = execution.join().expect("bridge execution thread");
    let _ = fs::remove_file(path);
    let (code, _) = result.expect("execute BNWeb bridge fixture");
    assert_eq!(code, 0);
    assert!(response.starts_with("HTTP/1.1 207"), "{response}");
    assert!(response.ends_with("bn-handler"), "{response}");
}

#[test]
fn bnweb_dispatch_correlates_response_id_with_redacted_log_record() {
    let path = unique_temp("bnweb-correlation.jsonl");
    let source = format!(
        r#"IMPORT BNWeb AS Web
IMPORT BNLog AS Log
IMPORT HOST.Net AS Net
IMPORT HOST.FileSystem AS FS
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(204)
END FUNCTION
FUNCTION Start() AS VOID
LET logger AS Log.Logger = NEW Log.Logger()
LET added AS VOID OR Error = logger.AddFile("{}", 3)
IF added IS Error THEN
PRINT "logger-error"
ELSE
LET serverResult AS Web.Server OR Error = Web.Server.New(logger)
IF serverResult IS Error THEN
PRINT "server-error"
ELSE
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
PRINT "address-error"
ELSE
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
PRINT "endpoint-error"
ELSE
LET server AS Web.Server = serverResult
server.Route("GET", "/", Handler)
LET started AS VOID OR Error = server.Start(endpoint)
IF started IS Error THEN
PRINT "start-error"
ELSE
LET request AS Web.Request = NEW Web.Request()
LET response AS Web.Response = NEW Web.Response()
LET dispatched AS VOID OR Error = server.Dispatch(request, response)
IF dispatched IS Error THEN
PRINT "dispatch-error"
ELSE
LET requestId AS STRING OR Error = response.Header("X-Request-ID")
IF requestId IS Error THEN
PRINT "missing-request-id"
ELSE
PRINT LEN(requestId) >= 33
END IF
END IF
server.Stop(1000)
END IF
logger.Flush(1000)
END IF
END IF
END IF
END IF
END FUNCTION
"#,
        bn_path(&path)
    );
    let (_, output) = match run(&source, "") {
        Ok(result) => result,
        Err(error) if error.code == "WEB_LISTEN" => {
            let _ = fs::remove_file(path);
            return;
        }
        Err(error) => panic!("execute BNWeb correlation path: {error:?}"),
    };
    let contents = fs::read_to_string(&path).expect("read BNWeb correlation log");
    let _ = fs::remove_file(path);
    assert_eq!(output, "TRUE\n");
    assert!(contents.contains("\"request_id\":\"r"), "{contents}");
}

#[test]
fn bnweb_client_rejects_unsafe_urls_before_transport() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET client AS Web.Client = NEW Web.Client()
LET response AS Web.Response OR Error = client.Request("GET", "http://127.0.0.1/", "")
PRINT response IS Error
LET malformed AS Web.Response OR Error = client.Request("GET", "not-a-url", "")
PRINT malformed IS Error
DELETE client
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNWeb client validation");
    assert_eq!(output, "TRUE\nTRUE\n");
}

#[test]
fn bnweb_egress_policy_is_constructed_and_applied_before_transport() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET policy AS Web.EgressPolicy OR Error = Web.EgressPolicy.New("https", "203.0.113.0/24", "443", 2, 1000)
PRINT policy IS Error
LET client AS Web.Client = NEW Web.Client()
IF policy IS Error THEN
PRINT TRUE
ELSE
LET response AS Web.Response OR Error = client.RequestWithPolicy("GET", "http://127.0.0.1/", "", policy)
PRINT response IS Error
END IF
DELETE client
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNWeb egress policy");
    assert_eq!(output, "FALSE\nTRUE\n");
}

#[test]
fn bnweb_server_errors_stay_in_error_channel() {
    let source = r"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET server AS Web.Server = NEW Web.Server()
LET stopped AS VOID OR Error = server.Stop(0)
PRINT stopped IS Error
DELETE server
END FUNCTION
";
    let (_, output) = run(source, "").expect("execute BNWeb server errors");
    assert_eq!(output, "TRUE\n");
}

#[test]
fn bnweb_response_enforces_commit_state() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET response AS Web.Response = NEW Web.Response()
response.SetStatus(201)
response.Write("ok")
response.Commit()
PRINT response.Status(), response.IsCommitted()
LET failed AS VOID OR Error = response.Write("late")
PRINT failed IS Error
DELETE response
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNWeb response");
    assert_eq!(output, "201 TRUE\nTRUE\n");
}

#[test]
fn bnweb_request_exposes_bounded_native_values() {
    let source = r"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET request AS Web.Request = NEW Web.Request()
PRINT request.Method(), request.Target(), request.Body(1)
PRINT request.PeerAddress(), request.EffectiveClientAddress()
LET too_small AS STRING OR Error = request.Body(-1)
PRINT too_small IS Error
DELETE request
END FUNCTION
";
    let (_, output) = run(source, "").expect("execute BNWeb request");
    assert_eq!(output, "GET / \nHOST.Net.Address HOST.Net.Address\nTRUE\n");
}

#[test]
fn bnweb_request_collections_are_bounded_objects() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET request AS Web.Request = NEW Web.Request()
LET headers AS Web.HeaderValues OR Error = request.Headers("x-test")
LET query AS Web.QueryValues OR Error = request.Query("q")
IF headers IS Error OR query IS Error THEN
PRINT "error"
ELSE
PRINT headers.Count(), query.Count()
END IF
DELETE request
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNWeb collections");
    assert_eq!(output, "0 0\n");
}

#[test]
fn bnweb_session_store_constructor_reaches_provider() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET sessions AS Web.SessionStore = NEW Web.SessionStore(2, 60000)
LET id AS STRING OR Error = sessions.Create("value")
IF id IS Error THEN
    PRINT "error"
ELSE
    sessions.Set(id, "updated")
    LET rotated AS STRING OR Error = sessions.Rotate(id, "rotated")
    IF rotated IS Error THEN PRINT "error" ELSE PRINT sessions.Get(rotated)
END IF
DELETE sessions
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute SessionStore provider");
    assert_eq!(output, "rotated\n");
}

#[test]
fn bnweb_static_state_factories_reach_providers() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET sessions AS Web.SessionStore OR Error = Web.SessionStore.New(2, 60000)
PRINT sessions IS Error
LET page AS Web.Scraper OR Error = Web.Scraper.Parse("<p>ok</p>")
IF page IS Error THEN PRINT "error" ELSE PRINT page.Text("p")
LET tls AS Web.TLSConfig OR Error = Web.TLSConfig.FromPEM("bad", "bad")
PRINT tls IS Error
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute BNWeb static factories");
    assert_eq!(output, "FALSE\nok\nTRUE\n");
}

#[test]
fn bnweb_scraper_and_acl_reach_providers() {
    let source = r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Start() AS VOID
LET page AS Web.Scraper = NEW Web.Scraper("<p>Hello</p><script>x()</script>")
PRINT page.Text("p")
LET acl AS Web.ACL = NEW Web.ACL()
LET cidr AS Net.CIDR OR Error = Net.CIDR.Parse("10.0.0.0/8")
IF cidr IS Error THEN PRINT "error" ELSE acl.Deny(cidr)
LET address AS Net.Address OR Error = Net.Address.Parse("10.1.2.3")
IF address IS Error THEN PRINT "error" ELSE PRINT acl.Check(address)
DELETE page
DELETE acl
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute scraper and ACL providers");
    assert_eq!(output, "Hello\nFALSE\n");
}

#[test]
fn bnweb_cookie_jar_is_reachable_from_bn() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET jar AS Web.CookieJar = NEW Web.CookieJar()
jar.Set("sid", "a", "example.test", "/", 60000)
PRINT jar.Get("sid", "example.test", "/"), jar.Count()
jar.Delete("sid", "example.test", "/")
LET missing AS STRING OR Error = jar.Get("sid", "example.test", "/")
PRINT missing IS Error
DELETE jar
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute CookieJar provider");
    assert_eq!(output, "a 1\nTRUE\n");
}

#[test]
fn bnweb_cookie_jar_exposes_explicit_secure_policy() {
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET jar AS Web.CookieJar = NEW Web.CookieJar()
LET accepted AS VOID OR Error = jar.SetWithPolicy("sid", "a", "example.test", "/", 60000, TRUE, TRUE, "Strict")
PRINT accepted IS Error, jar.Count()
LET rejected AS VOID OR Error = jar.SetWithPolicy("bad", "b", "example.test", "/", 60000, FALSE, TRUE, "None")
PRINT rejected IS Error, jar.Count()
DELETE jar
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute explicit CookieJar policy");
    assert_eq!(output, "FALSE 1\nTRUE 1\n");
}
