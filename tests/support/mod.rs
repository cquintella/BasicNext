#![allow(dead_code)]

use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use serde_json::json;
use wait_timeout::ChildExt;

static UNIQUE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub fn new(label: &str) -> io::Result<Self> {
        let root = env::temp_dir();
        loop {
            let id = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!("basicnext-{label}-{}-{id}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path.join(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[derive(Debug)]
pub struct ProcessOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
    pub failure_artifact: Option<PathBuf>,
}

impl ProcessOutput {
    pub fn accept_expected_status(&mut self) {
        if let Some(path) = self.failure_artifact.take() {
            let _ = fs::remove_file(path);
        }
    }
}

pub fn run(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
) -> io::Result<ProcessOutput> {
    let command_line: Vec<String> = std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|item| item.to_string_lossy().into_owned())
        .collect();

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }

    let mut child = command.spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .expect("stdout is piped before spawning the child");
    let mut stderr = child
        .stderr
        .take()
        .expect("stderr is piped before spawning the child");
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });

    if let Some(bytes) = input {
        let mut stdin = child
            .stdin
            .take()
            .expect("stdin is piped when input bytes are provided");
        stdin.write_all(bytes)?;
    }

    let (status, timed_out) = if let Some(status) = child.wait_timeout(timeout)? {
        (status, false)
    } else {
        child.kill()?;
        (child.wait()?, true)
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout reader thread panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr reader thread panicked"))??;

    let failure_artifact = if timed_out || !status.success() {
        Some(retain_failure(
            &command_line,
            status,
            timed_out,
            timeout,
            &stdout,
            &stderr,
        )?)
    } else {
        None
    };

    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
        timed_out,
        failure_artifact,
    })
}

fn retain_failure(
    command: &[String],
    status: ExitStatus,
    timed_out: bool,
    timeout: Duration,
    stdout: &[u8],
    stderr: &[u8],
) -> io::Result<PathBuf> {
    let directory =
        env::var_os("BN_FAILURE_ARTIFACT_DIR").map_or_else(env::temp_dir, PathBuf::from);
    fs::create_dir_all(&directory)?;
    let id = UNIQUE_ID.fetch_add(1, Ordering::Relaxed);
    let path = directory.join(format!("bn-diff-{}-{id}.json", std::process::id()));
    let report = json!({
        "command": command,
        "status": if timed_out { "timeout" } else { "failed" },
        "returncode": status.code(),
        "timeout_ms": timeout.as_millis(),
        "stdout": String::from_utf8_lossy(stdout),
        "stderr": String::from_utf8_lossy(stderr),
    });
    fs::write(&path, serde_json::to_vec_pretty(&report)?)?;
    Ok(path)
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn executable_path(name: &str) -> PathBuf {
    let override_name = match name {
        "bni" => Some("BN_INTERPRETER"),
        "bnc" => Some("BN_COMPILER"),
        _ => None,
    };
    if let Some(path) = override_name.and_then(env::var_os) {
        return PathBuf::from(path);
    }

    let mut path = workspace_root().join("target");
    path.push(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    });
    path.push(format!("{name}{}", env::consts::EXE_SUFFIX));
    path
}

pub fn bni() -> Command {
    Command::new(executable_path("bni"))
}

pub fn bnc() -> Command {
    Command::new(executable_path("bnc"))
}

pub fn compile_native(source: impl AsRef<Path>, directory: &TestDir) -> PathBuf {
    let artifact = directory.join(format!("program{}", env::consts::EXE_SUFFIX));
    let output = run(
        bnc().args([
            source.as_ref().as_os_str(),
            "-o".as_ref(),
            artifact.as_os_str(),
        ]),
        None,
        Duration::from_secs(30),
    )
    .expect("run bnc");
    assert!(
        output.status.success(),
        "bnc failed for {}:\n{}",
        source.as_ref().display(),
        String::from_utf8_lossy(&output.stderr)
    );
    artifact
}
