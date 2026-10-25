// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    fs,
    io::{BufRead, Cursor, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

pub(crate) use bn_frontend::lowering::lower_graph;
pub(crate) use bn_frontend::module_graph::load;
pub(crate) use bn_frontend::semantic::analyze_modules;
pub(crate) use bn_interp::{HostEnv, execute_with_host, execute_with_host_debug};
pub(crate) use bn_interpret_driver::environment::HostEnvDefaults;

pub(crate) struct ChannelReader {
    pub(crate) receiver: std::sync::mpsc::Receiver<Vec<u8>>,
    pub(crate) current: Cursor<Vec<u8>>,
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

/// A fixed-clock host with the providers `bn` ships.
pub(crate) fn fixed(arguments: Vec<String>, timestamp_ms: i64, monotonic_ns: i64) -> HostEnv {
    HostEnv::fixed(arguments, timestamp_ms, monotonic_ns).with_default_providers()
}

pub(crate) fn run(source_text: &str, input: &str) -> Result<(u8, String), bn_diag::Diagnostic> {
    run_with_host(source_text, input, &fixed(vec!["runtime.bn".into()], 0, 0))
}

pub(crate) fn run_with_host(
    source_text: &str,
    input: &str,
    host: &HostEnv,
) -> Result<(u8, String), bn_diag::Diagnostic> {
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

pub(crate) fn run_path(path: &str) -> Result<(u8, String), bn_diag::Diagnostic> {
    run_loaded(path, "", &fixed(vec![path.into()], 0, 0))
}

pub(crate) fn unique_temp(suffix: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "basicnext-{}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        suffix
    ))
}

pub(crate) fn bn_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

pub(crate) fn run_loaded(
    path: &str,
    input: &str,
    host: &HostEnv,
) -> Result<(u8, String), bn_diag::Diagnostic> {
    let graph = load(path).map_err(|error| *error.diagnostic)?;
    let models = analyze_modules(&graph).map_err(|error| *error.diagnostic)?;
    let module = lower_graph(&graph, &models)?;
    let mut input = Cursor::new(input.as_bytes());
    let mut output = Vec::new();
    let code = execute_with_host(&module, &mut input, &mut output, host)?;
    Ok((code, String::from_utf8(output).expect("UTF-8 output")))
}

pub(crate) fn run_loaded_debug(
    path: &str,
    host: &HostEnv,
    events: &mut Vec<(String, usize)>,
) -> Result<(), bn_diag::Diagnostic> {
    let graph = load(path).map_err(|error| *error.diagnostic)?;
    let models = analyze_modules(&graph).map_err(|error| *error.diagnostic)?;
    let module = lower_graph(&graph, &models)?;
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    let mut hook = |function: &str, span: bn_source::Span| {
        events.push((function.to_owned(), span.start.line));
    };
    execute_with_host_debug(&module, &mut input, &mut output, host, &mut hook)?;
    Ok(())
}

pub(crate) struct InjectedDataProvider;
pub(crate) static NEXT_PROVIDER: AtomicU64 = AtomicU64::new(0);

impl bn_interpret_driver::environment::CsvProvider for InjectedDataProvider {
    fn read_csv(
        &self,
        _text: &str,
        _separator: char,
    ) -> Result<Vec<Vec<String>>, bn_interpret_driver::environment::DataFailure> {
        Ok(vec![vec![String::from("provided")]])
    }
}

/// One HTTP/1.1 exchange with `Connection: close`: the status line, the
/// `set-cookie` values and the body.
pub(crate) fn http_exchange(port: u16, request: &str) -> (String, Vec<String>, String) {
    let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
    client.write_all(request.as_bytes()).expect("write request");
    let mut raw = String::new();
    client.read_to_string(&mut raw).expect("read response");
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw.as_str(), ""));
    let mut lines = head.lines();
    let status = lines.next().unwrap_or_default().to_owned();
    let cookies = lines
        .filter_map(|line| line.split_once(": "))
        .filter(|(name, _)| name.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, value)| value.to_owned())
        .collect();
    (status, cookies, body.to_owned())
}

/// A BNWeb program listening on a free loopback port, stopped by a line on
/// its input.
pub(crate) struct Served {
    pub(crate) port: u16,
    pub(crate) input: std::sync::mpsc::Sender<Vec<u8>>,
    pub(crate) execution: std::thread::JoinHandle<Result<(u8, Vec<u8>), bn_diag::Diagnostic>>,
}

impl Served {
    /// Runs `path` with the port, then `arguments`, as its arguments; `None` when this
    /// host forbids binding (sandbox).
    pub(crate) fn start(path: &str, arguments: &[&str]) -> Option<Self> {
        let probe = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return None,
            Err(error) => panic!("bind probe: {error}"),
        };
        let port = probe.local_addr().expect("probe address").port();
        drop(probe);
        let graph = load(path).expect("load the program");
        let models = analyze_modules(&graph).expect("analyze the program");
        let module = lower_graph(&graph, &models).expect("lower the program");
        let (input, input_receiver) = std::sync::mpsc::channel();
        let mut host_arguments = vec![path.to_owned(), port.to_string()];
        host_arguments.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        let execution = std::thread::spawn(move || {
            let reader = ChannelReader {
                receiver: input_receiver,
                current: Cursor::new(Vec::new()),
            };
            let mut reader = std::io::BufReader::new(reader);
            let mut output = Vec::new();
            execute_with_host(
                &module,
                &mut reader,
                &mut output,
                &fixed(host_arguments, 0, 0),
            )
            .map(|code| (code, output))
        });
        let ready = (0..500).any(|_| {
            let connected = std::net::TcpStream::connect(("127.0.0.1", port)).is_ok();
            if !connected {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            connected
        });
        let served = Self {
            port,
            input,
            execution,
        };
        assert!(
            ready,
            "the program did not listen on {port}: {:?}",
            served.stop()
        );
        Some(served)
    }

    /// Stops the program and returns its exit code and output.
    pub(crate) fn stop(self) -> (u8, String) {
        let _ = self.input.send(b"\n".to_vec());
        let (code, output) = self
            .execution
            .join()
            .expect("program thread")
            .expect("run the program");
        (code, String::from_utf8_lossy(&output).into_owned())
    }
}

/// A server whose `/slow` handler waits 300 ms; with a `queue` argument it
/// runs its handlers on a `Dispatch.Queue`. It prints the results of
/// `UseQueue` before and after `Start` (the second is refused).
pub(crate) const SLOW_SERVER: &str = r#"IMPORT BNWeb AS Web
IMPORT BNDispatch AS Dispatch
IMPORT BNMath AS M
IMPORT HOST.Net AS Net

FUNCTION Slow(request AS Web.Request, response AS Web.Response) AS VOID
    LET group AS Dispatch.Group OR Error = Dispatch.Group.New()
    IF group IS Dispatch.Group THEN
        group.Enter()
        LET waited AS VOID OR Error = group.Wait(300)
        IF waited IS Error THEN
            response.Write("waited")
        END IF
    END IF
END FUNCTION

FUNCTION Start() AS INTEGER
    LET address AS Net.Address OR Error = Net.Address.Parse("127.0.0.1")
    LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(8)
    IF address IS Error OR queue IS Error THEN
        RETURN 1
    END IF
    LET endpoint AS Net.Endpoint OR Error = Net.Endpoint.Create(address, M.VAL(HOST.Args[1]) AS UINT16)
    IF endpoint IS Error THEN
        RETURN 1
    END IF
    LET server AS Web.Server = NEW Web.Server()
    LET parallel AS BOOLEAN = LEN(HOST.Args) > 2
    IF parallel THEN
        PRINT server.UseQueue(queue)
    END IF
    server.Route("GET", "/slow", Slow)
    PRINT server.Start(endpoint)
    IF parallel THEN
        PRINT server.UseQueue(queue)
    END IF
    LET line AS STRING OR EOF = INPUT()
    IF line IS STRING THEN
        server.Stop(5000)
    END IF
    RETURN 0
END FUNCTION
"#;

/// Sends `count` concurrent requests to `/slow`; the time they took and the
/// program's output, or `None` when this host forbids binding.
pub(crate) fn slow_requests(
    arguments: &[&str],
    count: usize,
) -> Option<(std::time::Duration, String)> {
    let path = unique_temp("bnweb-slow.bn");
    fs::write(&path, SLOW_SERVER).expect("write the program");
    let served = Served::start(path.to_str().expect("utf-8 path"), arguments);
    let _ = fs::remove_file(&path);
    let served = served?;
    let port = served.port;
    let started = std::time::Instant::now();
    let clients: Vec<_> = (0..count)
        .map(|_| {
            std::thread::spawn(move || {
                http_exchange(
                    port,
                    "GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
                )
            })
        })
        .collect();
    for client in clients {
        let answer = client.join().expect("client thread");
        assert!(answer.0.starts_with("HTTP/1.1 200"), "{answer:?}");
        assert_eq!(answer.2, "waited");
    }
    let elapsed = started.elapsed();
    let (code, output) = served.stop();
    assert_eq!(code, 0, "{output}");
    Some((elapsed, output))
}

/// Serialize HOST.Exec acceptance tests: parallel rustc/spawn storms were
/// falsely tripping the 60s Exec timeout under load.
pub(crate) fn host_exec_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn build_host_exec_helper() -> PathBuf {
    static HELPER: OnceLock<PathBuf> = OnceLock::new();
    HELPER
        .get_or_init(|| {
            let helper = unique_temp("host-exec-helper-shared");
            let status = Command::new("rustc")
                .args(["--edition=2021", "tests/fixtures/host_exec_helper.rs", "-o"])
                .arg(&helper)
                .status()
                .expect("spawn rustc for host_exec_helper");
            assert!(status.success(), "rustc must build host_exec_helper");
            helper
        })
        .clone()
}
