// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! End-to-end integration tests for the interactive session server in `bni`:
//! `bni eval --session --format json` (Cluster D.1).

#![allow(clippy::map_unwrap_or)]

use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
};

use serde_json::{Value as JsonValue, json};

const WORKSPACE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn bni_cmd() -> Command {
    let _ = std::env::set_current_dir(WORKSPACE);
    let path = std::env::var_os("BN_INTERPRETER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_BIN_EXE_bni")));
    let mut command = Command::new(path);
    command.current_dir(WORKSPACE);
    command
}

struct SessionClient {
    child: std::process::Child,
    reader: BufReader<std::process::ChildStdout>,
    writer: std::process::ChildStdin,
}

impl SessionClient {
    fn spawn() -> Self {
        let mut child = bni_cmd()
            .args(["eval", "--session", "--format", "json"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn bni session");

        let reader = BufReader::new(child.stdout.take().expect("child stdout"));
        let writer = child.stdin.take().expect("child stdin");
        Self {
            child,
            reader,
            writer,
        }
    }

    fn request(&mut self, req: &JsonValue) -> JsonValue {
        let req_line = format!("{req}\n");
        self.writer
            .write_all(req_line.as_bytes())
            .expect("write request line to session");
        self.writer.flush().expect("flush session stdin");

        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .expect("read response line from session");
        assert!(
            !line.trim().is_empty(),
            "expected response from session server"
        );
        serde_json::from_str(&line).expect("valid JSON response envelope")
    }

    fn close(mut self) {
        let _ = self.request(&json!({"action": "quit"}));
        let status = self.child.wait().expect("wait for child exit");
        assert!(
            status.success(),
            "session server exited with code {:?}",
            status.code()
        );
    }
}

#[test]
fn session_classify_actions() {
    let mut client = SessionClient::spawn();

    // 1. Complete statement
    let resp = client.request(&json!({
        "action": "classify",
        "source": "LET x AS INTEGER = 10"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["status"], "complete");
    assert!(resp["tokens"].as_array().expect("tokens array").len() >= 3);

    // 2. Incomplete statement
    let resp = client.request(&json!({
        "action": "classify",
        "source": "IF x > 0 THEN"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["status"], "incomplete");
    let open = resp["open_constructs"].as_array().expect("open constructs");
    assert!(
        open.iter()
            .any(|c| c.as_str().is_some_and(|s| s.contains("IF")))
    );

    // 3. Invalid syntax
    let resp = client.request(&json!({
        "action": "classify",
        "source": "LET = = = 123"
    }));
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["status"], "invalid");
    assert!(
        !resp["diagnostics"]
            .as_array()
            .expect("diagnostics")
            .is_empty()
    );

    client.close();
}

#[test]
fn session_variables_persistence_and_expressions() {
    let mut client = SessionClient::spawn();

    // Turn 1: declare x
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET x AS INTEGER = 42"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["exit_code"], 0);

    // Turn 2: declare y using x
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET y AS INTEGER = x + 8"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 3: evaluate expression x + y
    let resp = client.request(&json!({
        "action": "submit",
        "source": "x + y"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["result_value"], "92");
    assert_eq!(resp["result_type"], "INT32");

    // Turn 4: inspect bindings
    let resp = client.request(&json!({
        "action": "bindings"
    }));
    assert_eq!(resp["ok"], true);
    let vars = resp["variables"].as_array().expect("variables array");
    assert!(
        vars.iter()
            .any(|v| v["name"] == "x" && v["type"] == "INT32")
    );
    assert!(
        vars.iter()
            .any(|v| v["name"] == "y" && v["type"] == "INT32")
    );

    client.close();
}

#[test]
fn session_atomicity_on_errors() {
    let mut client = SessionClient::spawn();

    // Turn 1: valid variable
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET a AS INTEGER = 10"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 2: static semantic error (unknown type)
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET bad AS NonExistentType = 123"
    }));
    assert_eq!(resp["ok"], false);
    assert_eq!(resp["exit_code"], 1);

    // Verify session is alive and 'a' is preserved
    let resp = client.request(&json!({
        "action": "submit",
        "source": "a * 2"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["result_value"], "20");

    // Turn 3: runtime error (divide by zero)
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET bad_rt AS INTEGER = a / 0"
    }));
    assert_eq!(resp["ok"], false);

    // Verify 'bad_rt' was not committed and 'a' is still intact
    let resp = client.request(&json!({
        "action": "bindings"
    }));
    let vars = resp["variables"].as_array().expect("variables array");
    assert!(vars.iter().any(|v| v["name"] == "a"));
    assert!(!vars.iter().any(|v| v["name"] == "bad_rt"));

    client.close();
}

#[test]
fn session_function_and_class_declarations() {
    let mut client = SessionClient::spawn();

    // Turn 1: declare function
    let resp = client.request(&json!({
        "action": "submit",
        "source": "FUNCTION AddFive(n AS INTEGER) AS INTEGER\nRETURN n + 5\nEND FUNCTION"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 2: call function
    let resp = client.request(&json!({
        "action": "submit",
        "source": "AddFive(20)"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["result_value"], "25");

    // Turn 3: declare class
    let resp = client.request(&json!({
        "action": "submit",
        "source": "CLASS Item\nPUBLIC value AS INTEGER\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 4: instantiate class and use member
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET it AS Item = NEW Item()\nit.value = 99\nPRINT it.value"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["stdout"], "99\n");

    client.close();
}

#[test]
fn session_introspection_actions() {
    let mut client = SessionClient::spawn();

    // 1. type
    let resp = client.request(&json!({
        "action": "type",
        "source": "42 * 2"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["type"], "INT32");

    // 2. ir
    let resp = client.request(&json!({
        "action": "ir",
        "source": "LET val AS INTEGER = 7"
    }));
    assert_eq!(resp["ok"], true);
    assert!(!resp["ir"].as_str().expect("ir string").is_empty());

    // 3. cfg
    let resp = client.request(&json!({
        "action": "cfg",
        "source": "LET val AS INTEGER = 7"
    }));
    assert_eq!(resp["ok"], true);
    assert!(!resp["blocks"].as_array().expect("blocks array").is_empty());

    // 4. arc
    let resp = client.request(&json!({
        "action": "arc"
    }));
    assert_eq!(resp["ok"], true);
    assert!(resp["total_live_objects"].is_number());

    // 5. bench
    let resp = client.request(&json!({
        "action": "bench",
        "source": "1 + 1",
        "iterations": 20
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["iterations"], 20);
    assert!(resp["mean_ns"].is_number());

    // 6. complete
    let resp = client.request(&json!({
        "action": "complete",
        "prefix": "FU"
    }));
    assert_eq!(resp["ok"], true);
    let candidates = resp["candidates"].as_array().expect("candidates");
    assert!(candidates.iter().any(|c| c["name"] == "FUNCTION"));

    // 7. doc
    let resp = client.request(&json!({
        "action": "doc",
        "symbol": "HOST.Fs"
    }));
    assert_eq!(resp["ok"], true);
    let methods = resp["methods"].as_array().expect("methods");
    assert!(methods.iter().any(|m| m.as_str() == Some("Exists")));

    // 8. policy
    let resp = client.request(&json!({
        "action": "policy"
    }));
    assert_eq!(resp["ok"], true);
    assert!(resp["policy"]["sandbox"].is_boolean());

    // 9. export
    client.request(&json!({
        "action": "submit",
        "source": "LET final_var AS INTEGER = 77"
    }));
    let resp = client.request(&json!({
        "action": "export"
    }));
    assert_eq!(resp["ok"], true);
    let exported = resp["source"].as_str().expect("exported program");
    assert!(exported.contains("FUNCTION Start() AS VOID"));
    assert!(exported.contains("final_var"));

    let temp_file = std::env::temp_dir().join(format!("session_export_{}.bn", std::process::id()));
    std::fs::write(&temp_file, exported).expect("write exported program");
    let check_status = bni_cmd()
        .args(["check", temp_file.to_str().unwrap()])
        .status()
        .expect("run bni check on exported program");
    let _ = std::fs::remove_file(&temp_file);
    assert!(
        check_status.success(),
        "exported program must pass bni check"
    );

    // 10. reset
    let resp = client.request(&json!({
        "action": "reset"
    }));
    assert_eq!(resp["ok"], true);
    let resp = client.request(&json!({
        "action": "bindings"
    }));
    assert!(resp["variables"].as_array().expect("vars").is_empty());

    client.close();
}

#[test]
fn session_arc_temporal_inversion_between_turns() {
    let mut client = SessionClient::spawn();

    // Turn 1: declare class
    let resp = client.request(&json!({
        "action": "submit",
        "source": "CLASS Node\nPUBLIC value AS INTEGER\nPUBLIC FUNCTION CONSTRUCTOR()\nEND FUNCTION\nEND CLASS"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 2: allocate class, assign to variable, and RELEASE it
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET n AS Node = NEW Node()\nn.value = 123\nRELEASE n"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 3: attempt to access released variable in the next turn
    let resp = client.request(&json!({
        "action": "submit",
        "source": "PRINT n.value"
    }));
    assert_eq!(resp["ok"], false);

    // Turn 4: verify session is intact and ready for subsequent turns
    let resp = client.request(&json!({
        "action": "submit",
        "source": "LET fresh AS INTEGER = 777"
    }));
    assert_eq!(resp["ok"], true);

    // Turn 5: evaluate expression using fresh variable
    let resp = client.request(&json!({
        "action": "submit",
        "source": "fresh * 2"
    }));
    assert_eq!(resp["ok"], true);
    assert_eq!(resp["result_value"], "1554");

    client.close();
}
