// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Server.Static` against a hostile tree, end to end on `bni` (a compiled
//! server does not listen yet, so `bnc` is covered by the runtime reader's
//! own tests): a FIFO in the mount is refused at once and never holds a
//! server thread, and a mount outside the `--read-root` serves nothing.

#![cfg(unix)]

use super::support::{TestDir, bni};
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::process::Stdio;
use std::time::Duration;

const CONNECT_ATTEMPTS: u32 = 400;
const CONNECT_PAUSE: Duration = Duration::from_millis(25);
/// A response slower than this is a blocked server thread.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
/// More FIFO requests than the server has worker threads.
const FIFO_REQUESTS: usize = 32;
const SECRET: &str = "outside-the-read-root";
const PUBLIC: &str = "inside-the-read-root";

const PROGRAM: &str = "IMPORT BNWeb AS Web
IMPORT HOST.Net AS Net
IMPORT HOST.FileSystem AS FS

FUNCTION Start() AS VOID
    LET address AS Net.Address OR Error = Net.Address.Parse(\"127.0.0.1\")
    IF address IS Error THEN
        STOP 1
    END IF
    LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, __PORT__)
    IF endpoint IS Error THEN
        STOP 2
    END IF
    LET server AS Web.Server = NEW Web.Server()
    LET inside AS VOID OR Error = server.Static(\"/in\", HOST.Args[1])
    LET outside AS VOID OR Error = server.Static(\"/out\", HOST.Args[2])
    IF inside IS Error OR outside IS Error THEN
        STOP 3
    END IF
    LET started AS VOID OR Error = server.Start(endpoint)
    IF started IS Error THEN
        STOP 4
    END IF
    LET gate AS STRING OR EOF = INPUT()
    IF gate IS EOF THEN
        PRINT \"stopping\"
    END IF
    server.Stop(1000)
END FUNCTION
";

fn free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind");
    listener.local_addr().expect("address").port()
}

fn get(port: u16, path: &str) -> String {
    let mut connection = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    connection
        .set_read_timeout(Some(RESPONSE_TIMEOUT))
        .expect("read timeout");
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    connection.write_all(request.as_bytes()).expect("write");
    let mut response = String::new();
    connection
        .read_to_string(&mut response)
        .unwrap_or_else(|error| panic!("{path}: no response in time ({error})"));
    response
}

#[test]
fn a_static_mount_refuses_a_fifo_and_paths_outside_the_read_root() {
    let directory = TestDir::new("web-static-policy").expect("directory");
    let base = directory.path().canonicalize().expect("canonical base");
    let inside = base.join("inside");
    let outside = base.join("outside");
    fs::create_dir_all(&inside).expect("inside");
    fs::create_dir_all(&outside).expect("outside");
    fs::write(inside.join("page.txt"), PUBLIC).expect("public file");
    fs::write(outside.join("page.txt"), SECRET).expect("secret file");
    let fifo = std::process::Command::new("mkfifo")
        .arg(inside.join("pipe"))
        .status()
        .expect("run mkfifo");
    assert!(fifo.success(), "mkfifo");
    let port = free_port();
    let source = base.join("static.bn");
    fs::write(&source, PROGRAM.replace("__PORT__", &port.to_string())).expect("source");
    let mut child = bni()
        .current_dir(&base)
        .args(["run", "--sandbox", "--read-root"])
        .arg(&inside)
        .arg(&source)
        .arg("--")
        .arg(&inside)
        .arg(&outside)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start bni");
    let ready = (0..CONNECT_ATTEMPTS).any(|_| {
        let connected = TcpStream::connect(("127.0.0.1", port)).is_ok();
        if !connected {
            std::thread::sleep(CONNECT_PAUSE);
        }
        connected
    });
    let outcome = std::panic::catch_unwind(|| {
        assert!(ready, "the server accepted a connection");
        let fifo_responses: Vec<String> =
            (0..FIFO_REQUESTS).map(|_| get(port, "/in/pipe")).collect();
        let public = get(port, "/in/page.txt");
        let secret = get(port, "/out/page.txt");
        (fifo_responses, public, secret)
    });
    drop(child.stdin.take());
    let _ = child.wait();
    let (fifo_responses, public, secret) =
        outcome.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    for response in &fifo_responses {
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }
    assert!(public.starts_with("HTTP/1.1 200"), "{public}");
    assert!(public.ends_with(PUBLIC), "{public}");
    assert!(secret.starts_with("HTTP/1.1 403"), "{secret}");
    assert!(!secret.contains(SECRET), "{secret}");
}
