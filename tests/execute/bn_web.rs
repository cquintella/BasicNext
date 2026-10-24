// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::support::*;
use std::{
    fs,
    io::{Cursor, Read, Write},
    sync::atomic::{AtomicU64, Ordering},
};
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
LET options AS Web.ServerOptions OR Error = Web.ServerOptions.New(1, 128, 1, 1, 65536, 100, 8192, 8388608, 5000, 5000, 30000, 60000, 120000, 5000, 10, 10, 10000, FALSE)
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

/// The in-process `Server.Dispatch` keeps sessions as the network server
/// does (bucket 0.6.5 E.S0.e): a write creates the session and the `sid`
/// cookie; `Session()` without `UseSessions` is `UNAVAILABLE`. A header the
/// response lacks is an `Error`, as `Response.Header` declares.
#[test]
fn bnweb_dispatch_keeps_server_sessions() {
    let source = r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
FUNCTION Login(request AS Web.Request, response AS Web.Response) AS VOID
LET session AS Web.Session OR Error = request.Session()
IF session IS Error THEN
PRINT session.Code
ELSE
PRINT session.Get("user") IS Error
session.Set("user", "ana")
PRINT session.Get("user")
END IF
END FUNCTION
FUNCTION Start() AS VOID
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
STOP 1
END IF
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, 0)
IF endpoint IS Error THEN
STOP 2
END IF
LET plain AS Web.Server = NEW Web.Server()
plain.Route("GET", "/", Login)
plain.Start(endpoint)
plain.Dispatch(NEW Web.Request(), NEW Web.Response())
LET server AS Web.Server = NEW Web.Server()
PRINT server.UseSessions(0, 1000) IS Error
PRINT server.UseSessions(4, 60000)
server.Route("GET", "/", Login)
server.Start(endpoint)
LET response AS Web.Response = NEW Web.Response()
server.Dispatch(NEW Web.Request(), response)
LET cookie AS STRING OR Error = response.Header("Set-Cookie")
IF cookie IS STRING THEN
PRINT LEN(cookie)
END IF
PRINT response.Header("X-Absent") IS Error
plain.Stop(1000)
server.Stop(1000)
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("dispatch with sessions");
    let unavailable = bn_types::error_codes::web::UNAVAILABLE;
    assert_eq!(
        output,
        format!("{unavailable}\nTRUE\nNULL\nTRUE\nana\n69\nTRUE\n")
    );
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
            &fixed(vec!["bridge.bn".into()], 0, 0),
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

/// `examples/bnweb/users_api.bn` over real HTTP (bucket 0.6.5 E.S0.d,
/// E.S0.e): `POST /auth` checks the form and keeps the user in a server
/// session, rotating its id; `GET /users?id=` and `GET /users/:id` need it;
/// `POST /logout` ends it, and the old id is refused afterwards.
#[test]
fn bnweb_users_api_example_authenticates_and_serves_users() {
    let Some(served) = Served::start("examples/bnweb/users_api.bn", &[]) else {
        return;
    };
    let port = served.port;
    let form_post = |target: &str, form: &str, cookie: &str| {
        format!(
            "POST {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{cookie}\
             Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{form}",
            form.len()
        )
    };
    let get = |target: &str, cookie: &str| {
        format!("GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n{cookie}\r\n")
    };
    let rejected = http_exchange(port, &form_post("/auth", "user=ana&password=wrong", ""));
    let accepted = http_exchange(port, &form_post("/auth", "user=ana&password=bn-secret", ""));
    let sid = accepted
        .1
        .iter()
        .find_map(|cookie| cookie.strip_prefix("sid="))
        .and_then(|cookie| cookie.split(';').next())
        .unwrap_or_default()
        .to_owned();
    let session = format!("Cookie: sid={sid}\r\n");
    let anonymous = http_exchange(port, &get("/users?id=00102", ""));
    let by_query = http_exchange(port, &get("/users?id=00102", &session));
    let by_path = http_exchange(port, &get("/users/00103", &session));
    let missing = http_exchange(port, &get("/users/00999", &session));
    let forged = http_exchange(port, &get("/users/00102", "Cookie: sid=s0123\r\n"));
    let logout = http_exchange(port, &form_post("/logout", "", &session));
    let replayed = http_exchange(port, &get("/users/00102", &session));
    let (code, _) = served.stop();
    assert_eq!(code, 0);
    assert!(rejected.0.starts_with("HTTP/1.1 401"), "{rejected:?}");
    assert!(
        rejected.1.is_empty(),
        "no session for a failed login: {rejected:?}"
    );
    assert!(accepted.0.starts_with("HTTP/1.1 200"), "{accepted:?}");
    assert_eq!(accepted.2, "authenticated");
    assert_eq!(sid.len(), 33, "{accepted:?}");
    assert!(
        accepted.1[0].ends_with("; Path=/; HttpOnly; SameSite=Lax"),
        "{accepted:?}"
    );
    assert!(anonymous.0.starts_with("HTTP/1.1 401"), "{anonymous:?}");
    assert!(
        anonymous.1.is_empty(),
        "reading creates no session: {anonymous:?}"
    );
    assert_eq!(by_query.2, r#"{"id":"00102","name":"Ana"}"#);
    assert_eq!(by_path.2, r#"{"id":"00103","name":"Carla"}"#);
    assert!(missing.0.starts_with("HTTP/1.1 404"), "{missing:?}");
    assert!(forged.0.starts_with("HTTP/1.1 401"), "{forged:?}");
    assert_eq!(
        logout.1,
        ["sid=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax"],
        "{logout:?}"
    );
    assert!(replayed.0.starts_with("HTTP/1.1 401"), "{replayed:?}");
}

/// `Server.UseQueue` runs handlers in parallel (bucket 0.6.5 E.S0.e): eight
/// requests to a handler that waits 300 ms finish together, not one after
/// the other (2.4 s serially). `UseQueue` on a started server is refused.
#[test]
fn bnweb_handlers_run_in_parallel_on_a_dispatch_queue() {
    let Some((elapsed, output)) = slow_requests(&["queue"], 8) else {
        return;
    };
    assert!(
        elapsed < std::time::Duration::from_millis(1200),
        "8 × 300 ms handlers took {elapsed:?}"
    );
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[..2], ["NULL", "NULL"], "{output}");
    assert!(
        lines[2].starts_with("Error 1 in BNWeb.Server.UseQueue: server is already started"),
        "{output}"
    );
}

/// Without `UseQueue`, handlers run one at a time (`bnweb.md`, "Server
/// lifecycle"; known issue K43): four 300 ms handlers take at least 1.2 s.
#[test]
fn bnweb_handlers_without_a_queue_run_serially() {
    let Some((elapsed, output)) = slow_requests(&[], 4) else {
        return;
    };
    assert!(
        elapsed >= std::time::Duration::from_millis(1200),
        "4 × 300 ms handlers took {elapsed:?}, so some ran together"
    );
    assert_eq!(output, "NULL\n");
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
LET made AS Log.Logger OR Error = Log.Logger.New("web", "JSON", "{}")
IF made IS Error THEN
PRINT "logger-error"
ELSE
LET logger AS Log.Logger = made
LET level AS VOID OR Error = logger.SetLevel(Log.Level.HTTP)
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
#[allow(clippy::too_many_lines)]
fn bnweb_network_request_correlates_response_id_with_log_record() {
    let probe = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind BNWeb probe: {error}"),
    };
    let port = probe.local_addr().expect("probe address").port();
    drop(probe);
    let log_path = unique_temp("bnweb-network-log.jsonl");
    let source = format!(
        r#"IMPORT BNWeb AS Web
IMPORT BNLog AS Log
IMPORT HOST.Net AS Net
IMPORT HOST.FileSystem AS FS
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(200)
response.Write("ok")
END FUNCTION
FUNCTION Start() AS VOID
LET made AS Log.Logger OR Error = Log.Logger.New("netweb", "JSON", "{}")
IF made IS Error THEN
PRINT "logger-error"
STOP 1
END IF
LET logger AS Log.Logger = made
LET level AS VOID OR Error = logger.SetLevel(Log.Level.HTTP)
LET serverResult AS Web.Server OR Error = Web.Server.New(logger)
IF serverResult IS Error THEN
PRINT "server-error"
STOP 2
END IF
LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
IF address IS Error THEN
STOP 3
END IF
LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, {port})
IF endpoint IS Error THEN
STOP 4
END IF
LET server AS Web.Server = serverResult
server.Route("GET", "/test-access", Handler)
LET started AS VOID OR Error = server.Start(endpoint)
IF started IS Error THEN
STOP 5
END IF
LET gate AS STRING OR EOF = INPUT()
server.Stop(1000)
logger.Flush(1000)
END FUNCTION
"#,
        bn_path(&log_path)
    );
    let path = unique_temp("bnweb-network-access.bn");
    fs::write(&path, &source).expect("write BNWeb network access fixture");
    let graph = load(path.to_str().expect("fixture path")).expect("load fixture");
    let models = analyze_modules(&graph).expect("analyze fixture");
    let module = lower_graph(&graph, &models).expect("lower fixture");
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
            &fixed(vec!["network-access.bn".into()], 0, 0),
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
        let _ = execution.join();
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(log_path);
        return;
    };
    client
        .write_all(b"GET /test-access HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .expect("write request");
    let mut response = String::new();
    client.read_to_string(&mut response).expect("read response");
    input_sender
        .send(b"release\n".to_vec())
        .expect("stop server");
    let result = execution.join().expect("join server execution");
    let _ = fs::remove_file(path);
    if let Err(error) = result {
        let _ = fs::remove_file(&log_path);
        assert_eq!(error.code, "WEB_LISTEN", "{error:?}");
        return;
    }
    let contents = fs::read_to_string(&log_path).expect("read access log");
    let _ = fs::remove_file(log_path);
    assert!(
        response.contains("HTTP/1.1 200"),
        "response was: {response}"
    );
    assert!(
        response.contains("x-request-id:"),
        "response was: {response}"
    );
    assert!(
        contents.contains("\"http.method\":\"GET\""),
        "log was: {contents}"
    );
    assert!(
        contents.contains("\"http.path\":\"/test-access\""),
        "log was: {contents}"
    );
    assert!(
        contents.contains("\"http.status\":\"200\""),
        "log was: {contents}"
    );
    assert!(
        contents.contains("\"request_id\":\"r"),
        "log was: {contents}"
    );
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
RELEASE client
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
RELEASE client
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
RELEASE server
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
RELEASE response
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
RELEASE request
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
RELEASE request
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
RELEASE sessions
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
RELEASE page
RELEASE acl
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
RELEASE jar
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
RELEASE jar
END FUNCTION
"#;
    let (_, output) = run(source, "").expect("execute explicit CookieJar policy");
    assert_eq!(output, "FALSE 1\nTRUE 1\n");
}

#[test]
#[allow(clippy::too_many_lines)]
fn bnweb_server_static_requires_filesystem_and_serves_files() {
    let no_fs_source = r#"IMPORT BNWeb AS Web
FUNCTION Start() AS VOID
LET server AS Web.Server = NEW Web.Server()
LET res AS VOID OR Error = server.Static("/assets", ".")
IF res IS Error THEN
PRINT res.Code
END IF
END FUNCTION
"#;
    let (_, out) = run(no_fs_source, "").expect("execute no-fs source");
    assert_eq!(out, "9\n");

    let probe = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind static probe: {error}"),
    };
    let port = probe.local_addr().expect("probe address").port();
    drop(probe);

    let temp_dir = std::env::temp_dir().join(format!("bnweb-rt-static-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    fs::write(temp_dir.join("index.html"), "<html>hello bnweb</html>").unwrap();
    fs::write(temp_dir.join("info.txt"), "static content").unwrap();

    let temp_str = bn_path(&temp_dir);

    let source = format!(
        r#"IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
IMPORT HOST.FileSystem AS FS
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(200)
response.Write("override-handler")
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
server.Route("GET", "/assets/custom", Handler)
LET static_res AS VOID OR Error = server.Static("/assets", "{temp_str}")
IF static_res IS Error THEN
STOP 3
END IF
LET started AS VOID OR Error = server.Start(endpoint)
IF started IS Error THEN
STOP 4
END IF
LET gate AS STRING OR EOF = INPUT()
server.Stop(1000)
END FUNCTION
"#
    );
    let path = unique_temp("bnweb-static-test.bn");
    fs::write(&path, &source).expect("write static fixture");
    let graph = load(path.to_str().expect("fixture path")).expect("load fixture");
    let models = analyze_modules(&graph).expect("analyze fixture");
    let module = lower_graph(&graph, &models).expect("lower fixture");
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
            &fixed(vec!["static.bn".into()], 0, 0),
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
    let Some(client) = client else {
        let _ = input_sender.send(b"stop\n".to_vec());
        let _ = execution.join();
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    };
    drop(client);

    let send_req = |req: &str| -> String {
        let mut conn = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
        conn.write_all(req.as_bytes()).expect("write");
        let mut res = String::new();
        conn.read_to_string(&mut res).expect("read");
        res
    };

    let res =
        send_req("GET /assets/info.txt HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(res.starts_with("HTTP/1.1 200 OK"), "{res}");
    assert!(
        res.contains("content-type: text/plain; charset=utf-8"),
        "{res}"
    );
    assert!(res.ends_with("\r\n\r\nstatic content"), "{res}");

    let res_dir = send_req("GET /assets/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(res_dir.starts_with("HTTP/1.1 200 OK"), "{res_dir}");
    assert!(
        res_dir.contains("content-type: text/html; charset=utf-8"),
        "{res_dir}"
    );
    assert!(
        res_dir.ends_with("\r\n\r\n<html>hello bnweb</html>"),
        "{res_dir}"
    );

    let res_custom =
        send_req("GET /assets/custom HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    assert!(res_custom.starts_with("HTTP/1.1 200 OK"), "{res_custom}");
    assert!(
        res_custom.ends_with("\r\n\r\noverride-handler"),
        "{res_custom}"
    );

    input_sender.send(b"stop\n".to_vec()).expect("stop server");
    let result = execution.join().expect("join server thread");
    let _ = fs::remove_file(path);
    let _ = fs::remove_dir_all(&temp_dir);
    if let Err(error) = result {
        assert_eq!(error.code, "WEB_LISTEN", "{error:?}");
    }
}

#[test]
fn web_callback_uses_a_fresh_executor_and_projects_response() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "basicnext-web-callback-{}-{}.bn",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let source = r#"IMPORT BNWeb AS Web
FUNCTION Handler(request AS Web.Request, response AS Web.Response) AS VOID
response.SetStatus(207)
response.Write("isolated")
END FUNCTION
FUNCTION Start() AS VOID
END FUNCTION
"#;
    fs::write(&path, source).expect("write callback fixture");
    let graph = load(path.to_str().expect("fixture path")).expect("load fixture");
    let models = analyze_modules(&graph).expect("analyze fixture");
    let module = lower_graph(&graph, &models).expect("lower fixture");
    let request = bn_lib_web::web::Request::new(
        "GET",
        "/callback",
        Vec::new(),
        "",
        "127.0.0.1".parse().expect("peer address"),
    )
    .expect("construct callback request");
    let response = bn_lib_web::execute_callback(
        &module,
        &fixed(vec!["callback.bn".into()], 0, 0),
        "Handler",
        request,
        bn_lib_web::web::Response::new(),
    )
    .expect("execute callback");
    let _ = fs::remove_file(path);
    assert_eq!(response.status, 207);
    assert_eq!(response.body, "isolated");
}
