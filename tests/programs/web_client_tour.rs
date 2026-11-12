// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Every served member of the `BNWeb` client classes is called on both
//! backends against a local echo server, with the same output
//! (`tests/host/web_client_tour.bn`, golden `web_client_tour.stdout`).

use super::support::{TestDir, bni, compile_native, run, workspace_root};
use std::fmt::Write as _;
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, spawn};
use std::time::Duration;

const PROGRAM: &str = "tests/host/web_client_tour.bn";
const GOLDEN: &str = "tests/host/web_client_tour.stdout";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(120);
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);
const UPLOAD: &str = "tour-file-content\n";
const DECODED: &[u8] = b"decoded-tour-text";
const BINARY: &[u8] = &[0xff, 0xfe, 0x00, 0x41];
/// A request head larger than this is refused by the echo server.
const MAX_HEAD: usize = 16 * 1024;
/// A request body larger than this is refused by the echo server.
const MAX_BODY: usize = 64 * 1024;
/// Bodies at most this long are echoed back as text.
const ECHO_BODY: usize = 64;

/// CRC-32 (IEEE), as the gzip trailer requires.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// `data` as one gzip member holding one stored deflate block.
fn gzip_stored(data: &[u8]) -> Vec<u8> {
    let length = u16::try_from(data.len()).expect("a short payload");
    let mut out = vec![0x1f, 0x8b, 0x08, 0, 0, 0, 0, 0, 0, 0xff];
    out.push(0x01);
    out.extend_from_slice(&length.to_le_bytes());
    out.extend_from_slice(&(!length).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(data).to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(data.len())
            .expect("a short payload")
            .to_le_bytes(),
    );
    out
}

struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut data = Vec::new();
    let mut chunk = [0_u8; 4096];
    let head_end = loop {
        if let Some(at) = data.windows(4).position(|window| window == b"\r\n\r\n") {
            break at;
        }
        if data.len() > MAX_HEAD {
            return None;
        }
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..read]);
    };
    let head = String::from_utf8(data[..head_end].to_vec()).ok()?;
    let mut lines = head.split("\r\n");
    let mut start = lines.next()?.split(' ');
    let method = start.next()?.to_owned();
    let target = start.next()?.to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
        .collect::<Vec<_>>();
    let mut request = Request {
        method,
        target,
        headers,
        body: data[head_end + 4..].to_vec(),
    };
    let length = request
        .header("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return None;
    }
    while request.body.len() < length {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            return None;
        }
        request.body.extend_from_slice(&chunk[..read]);
    }
    request.body.truncate(length);
    Some(request)
}

/// The text `/verb` answers: what the server received.
fn echo(request: &Request) -> Vec<u8> {
    let kind = request
        .header("content-type")
        .map_or("-", |value| value.split(';').next().unwrap_or("-").trim());
    let mut text = format!(
        "{} {} tour={} type={kind}",
        request.method,
        request.target,
        request.header("x-tour").unwrap_or("-"),
    );
    // A multipart length depends on the boundary the client chooses.
    if kind == "multipart/form-data" {
        let body = String::from_utf8_lossy(&request.body);
        let _ = write!(
            text,
            " parts={} file={}",
            body.matches("Content-Disposition").count(),
            body.contains(UPLOAD.trim_end())
        );
    } else {
        let _ = write!(text, " length={}", request.body.len());
        if let Ok(body) = std::str::from_utf8(&request.body)
            && !body.is_empty()
            && body.len() <= ECHO_BODY
            && !body.contains(['\r', '\n'])
        {
            let _ = write!(text, " body={body}");
        }
    }
    text.into_bytes()
}

fn respond(stream: &mut TcpStream, request: &Request) {
    let path = request.target.split('?').next().unwrap_or("");
    let (body, encoding) = match path {
        "/reply" => (b"reply-body".to_vec(), None),
        "/gzip" => (gzip_stored(DECODED), Some("gzip")),
        "/bytes" => (BINARY.to_vec(), None),
        _ => (echo(request), None),
    };
    let mut head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-Tour-Reply: yes\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(encoding) = encoding {
        let _ = write!(head, "Content-Encoding: {encoding}\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    if request.method != "HEAD" {
        let _ = stream.write_all(&body);
    }
    let _ = stream.flush();
}

struct EchoServer {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl EchoServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local address").port();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let handle = spawn(move || {
            for stream in listener.incoming() {
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else {
                    continue;
                };
                let _ = stream.set_read_timeout(Some(SOCKET_TIMEOUT));
                let _ = stream.set_write_timeout(Some(SOCKET_TIMEOUT));
                if let Some(request) = read_request(&mut stream) {
                    respond(&mut stream, &request);
                }
            }
        });
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for EchoServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn tour(command: &mut Command, server: &EchoServer, directory: &TestDir) -> String {
    let upload = directory.path().join("tour.txt");
    fs::write(&upload, UPLOAD).expect("write the upload");
    let log = directory.path().join("tour.log");
    command
        .arg(format!("http://127.0.0.1:{}", server.port))
        .arg(&log)
        .arg(&upload)
        .arg(server.port.to_string());
    let output = run(command, None, PROCESS_TIMEOUT).expect("run the tour");
    assert!(
        output.status.success(),
        "tour failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn web_client_tour_runs_alike_on_both_backends() {
    let source = workspace_root().join(PROGRAM);
    let golden = fs::read_to_string(workspace_root().join(GOLDEN)).expect("read the golden");
    let server = EchoServer::start();

    let interpreter_folder = TestDir::new("web-client-tour-bni").expect("directory");
    let mut command = bni();
    command.arg("run").arg(&source).arg("--");
    let from_interpreter = tour(&mut command, &server, &interpreter_folder);
    assert_eq!(from_interpreter, golden, "bni");

    let native_folder = TestDir::new("web-client-tour-bnc").expect("directory");
    let artifact = compile_native(&source, &native_folder);
    let from_native = tour(&mut Command::new(&artifact), &server, &native_folder);
    assert_eq!(from_native, golden, "bnc");
}
