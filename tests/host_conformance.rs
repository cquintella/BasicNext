// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! HOST conformance: every operation the frontend type-checks has a fixture
//! under `tests/host/`, each fixture has an exact `bni` result, and the `bnc`
//! build of every fixture matches it byte for byte (every HOST operation
//! compiles natively since 2026-10-11). Adding a HOST member without a case
//! fails.

mod support;

use std::{collections::BTreeSet, fs, time::Duration};

use support::{ProcessOutput, TestDir, bnc, bni, executable_path, run, workspace_root};

const TIMEOUT: Duration = Duration::from_secs(30);

struct Case {
    fixture: &'static str,
    /// Program arguments; `"{bni}"` is replaced by the `bni` path.
    args: &'static [&'static str],
    covers: &'static [&'static str],
    stdout: &'static str,
    exit: i32,
    /// A diagnostic code expected on stderr (runtime failures).
    stderr: Option<&'static str>,
}

const CASES: &[Case] = &[
    Case {
        fixture: "args.bn",
        args: &["Ana", "Bia"],
        covers: &["HOST.Args"],
        stdout: "2\nAna\nBia\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "clock.bn",
        args: &[],
        covers: &["HOST.Clock.Now", "HOST.Clock.Timer"],
        stdout: "TRUE TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "numprocs.bn",
        args: &[],
        covers: &["HOST.NumProcs"],
        stdout: "TRUE FALSE\nTRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "random.bn",
        args: &[],
        covers: &["HOST.Random.Random", "HOST.Random.Seed"],
        stdout: "TRUE TRUE TRUE TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "console_stream.bn",
        args: &[],
        covers: &["HOST.Console.Cls", "HOST.Console.Beep"],
        stdout: "\u{1b}[2J\u{1b}[H\u{7}done\n",
        exit: 0,
        stderr: None,
    },
    // Standard output is a pipe here, so the TTY-only methods must fail.
    Case {
        fixture: "console_printat.bn",
        args: &[],
        covers: &["HOST.Console.PrintAt"],
        stdout: "",
        exit: 1,
        stderr: Some("HOST_CAPABILITY_UNAVAILABLE"),
    },
    Case {
        fixture: "console_numcols.bn",
        args: &[],
        covers: &["HOST.Console.NumCols"],
        stdout: "",
        exit: 1,
        stderr: Some("HOST_CAPABILITY_UNAVAILABLE"),
    },
    Case {
        fixture: "console_numrows.bn",
        args: &[],
        covers: &["HOST.Console.NumRows"],
        stdout: "",
        exit: 1,
        stderr: Some("HOST_CAPABILITY_UNAVAILABLE"),
    },
    Case {
        fixture: "net_codes.bn",
        args: &[],
        covers: &[
            "HOST.Net.INVALID_ARGUMENT",
            "HOST.Net.TIMEOUT",
            "HOST.Net.UNREACHABLE",
            "HOST.Net.CONNECTION_REFUSED",
            "HOST.Net.CONNECTION_CLOSED",
            "HOST.Net.ADDRESS_IN_USE",
            "HOST.Net.PERMISSION_DENIED",
            "HOST.Net.NOT_FOUND",
            "HOST.Net.UNAVAILABLE",
            "HOST.Net.LIMIT",
            "HOST.Net.CLOSED",
            "HOST.Net.IO_FAILED",
            "HOST.Net.POLICY_DENIED",
        ],
        stdout: "1 2 3 4\n5 6 7 8\n9 10 11 12 13\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "fs_codes.bn",
        args: &[],
        covers: &[
            "HOST.FileSystem.INVALID_ARGUMENT",
            "HOST.FileSystem.NOT_FOUND",
            "HOST.FileSystem.PERMISSION_DENIED",
            "HOST.FileSystem.IS_DIRECTORY",
            "HOST.FileSystem.CLOSED",
            "HOST.FileSystem.WRONG_FAMILY",
            "HOST.FileSystem.INVALID_UTF8",
            "HOST.FileSystem.IO_FAILED",
            "HOST.FileSystem.POLICY_DENIED",
        ],
        stdout: "1 2 3 4 5\n6 7 8 9\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "fs_text.bn",
        args: &[],
        covers: &[
            "HOST.FileSystem.Open",
            "HOST.FileSystem.File",
            "HOST.FileSystem.READ",
            "HOST.FileSystem.WRITE",
            "HOST.FileSystem.APPEND",
            "HOST.FileSystem.Exists",
            "HOST.FileSystem.DeleteFile",
            "FS.File.Write",
            "FS.File.WriteLine",
            "FS.File.ReadLine",
            "FS.File.ReadAll",
            "FS.File.Close",
        ],
        stdout: "TRUE 0 1 2\nline: one\nrest: 10\nclosed error: FALSE\nexists: TRUE\n\
                 removed error: FALSE exists: FALSE\nmissing is error: TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "fs_bytes.bn",
        args: &[],
        covers: &["FS.File.WriteBytes", "FS.File.ReadBytes"],
        stdout: "FALSE 3 1 128 255 TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "exec.bn",
        args: &["{bni}"],
        covers: &[
            "HOST.Exec.Run",
            "HOST.Exec.Result.ReturnCode",
            "HOST.Exec.Result.Stdout",
            "HOST.Exec.Result.Stderr",
        ],
        stdout: "TRUE 0 TRUE 0\nTRUE TRUE\nTRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "exec_codes.bn",
        args: &[],
        covers: &[
            "HOST.Exec.INVALID_ARGUMENT",
            "HOST.Exec.PROGRAM_NOT_FOUND",
            "HOST.Exec.PERMISSION_DENIED",
            "HOST.Exec.SPAWN_FAILED",
            "HOST.Exec.WAIT_FAILED",
            "HOST.Exec.CAPTURE_FAILED",
            "HOST.Exec.INVALID_UTF8",
            "HOST.Exec.CAPTURE_LIMIT",
            "HOST.Exec.TIMEOUT",
            "HOST.Exec.TERMINATION_FAILED",
            "HOST.Exec.POLICY_DENIED",
        ],
        stdout: "1 2 3 4 5 6 7 8 9 10 11\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "env_errors.bn",
        args: &[],
        covers: &[
            "HOST.Env.Get",
            "HOST.Env.Has",
            "HOST.Env.NOT_SET",
            "HOST.Env.INVALID_NAME",
            "HOST.Env.INVALID_UTF8",
            "HOST.Env.POLICY_DENIED",
        ],
        stdout: "1 2 3 4\nTRUE HOST.Env.Get\nTRUE HOST.Env.Get\nTRUE HOST.Env.Get\nTRUE HOST.Env.Get\nTRUE HOST.Env.Has\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_address.bn",
        args: &[],
        covers: &[
            "HOST.Net.Address",
            "HOST.Net.Address.Parse",
            "HOST.Net.Address.ToString",
            "HOST.Net.Address.IsLoopback",
            "HOST.Net.Address.IsPrivate",
            "HOST.Net.Address.IsLinkLocal",
            "HOST.Net.Address.IsMulticast",
            "HOST.Net.Address.IsIPv4",
            "HOST.Net.Address.IsIPv6",
            "HOST.Net.CIDR",
            "HOST.Net.CIDR.Parse",
            "HOST.Net.CIDR.Network",
            "HOST.Net.CIDR.PrefixLength",
            "HOST.Net.CIDR.Contains",
            "HOST.Net.Endpoint",
            "HOST.Net.Endpoint.Create",
            "HOST.Net.Endpoint.Address",
            "HOST.Net.Endpoint.Port",
        ],
        stdout: "TRUE TRUE\n127.0.0.1 TRUE TRUE TRUE TRUE\nTRUE 192.168.10.0 24 TRUE FALSE\n\
                 TRUE 127.0.0.1 8080\nTRUE FALSE FALSE TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_resolve.bn",
        args: &[],
        covers: &[
            "HOST.Net.Resolve",
            "HOST.Net.Addresses",
            "HOST.Net.Addresses.Count",
            "HOST.Net.Addresses.Get",
            "HOST.Net.Reverse",
        ],
        stdout: "TRUE TRUE TRUE\nreverse answered: TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_results.bn",
        args: &[],
        covers: &[
            "HOST.Net.Resolve",
            "HOST.Net.Addresses.Get",
            "HOST.Net.Endpoint.Create",
            "HOST.Net.Address.Parse",
        ],
        stdout: "TRUE TRUE TRUE\n1 TRUE TRUE TRUE\n\
                 Error 1 in HOST.Net.Addresses.Get: cannot get address 1 \
                 (cause: the index must be within 0..0; got 1)\n\
                 TRUE HOST.Net.Address.Parse\n::1 8080\nTRUE HOST.Net.Address.Parse\nTRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_tcp.bn",
        args: &[],
        covers: &[
            "HOST.Net.TCPListen",
            "HOST.Net.TCPListener",
            "HOST.Net.TCPListener.Accept",
            "HOST.Net.TCPListener.LocalEndpoint",
            "HOST.Net.TCPListener.Close",
            "HOST.Net.TCPConnect",
            "HOST.Net.TCPStream",
            "HOST.Net.TCPStream.SetTimeouts",
            "HOST.Net.TCPStream.LocalEndpoint",
            "HOST.Net.TCPStream.RemoteEndpoint",
            "HOST.Net.TCPStream.Write",
            "HOST.Net.TCPStream.Read",
            "HOST.Net.TCPStream.ShutdownRead",
            "HOST.Net.TCPStream.ShutdownWrite",
            "HOST.Net.TCPStream.Close",
        ],
        stdout: "TRUE TRUE TRUE\nFALSE TRUE TRUE\n2 2 72 105 TRUE FALSE FALSE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_cidr_tcp_errors.bn",
        args: &[],
        covers: &[
            "HOST.Net.CIDR.Parse",
            "HOST.Net.CIDR.Network",
            "HOST.Net.CIDR.PrefixLength",
            "HOST.Net.TCPStream.SetTimeouts",
            "HOST.Net.TCPStream.ShutdownRead",
            "HOST.Net.TCPStream.ShutdownWrite",
            "HOST.Net.TCPStream.Close",
        ],
        stdout: "1 cannot parse \"10.0.0.1/8\" as a CIDR prefix\n\
                 1 cannot parse \"10.0.0.0/08\" as a CIDR prefix\n\
                 1 cannot parse \"nope\" as a CIDR prefix\n\
                 1 cannot parse \"::/129\" as a CIDR prefix\n\
                 2001:db8:: 32\n0.0.0.0 0\n\
                 1 HOST.Net.TCPStream.SetTimeouts cannot set the timeouts of the TCP stream\n\
                 1 HOST.Net.TCPStream.SetTimeouts cannot set the timeouts of the TCP stream\n\
                 ok\nok\nok\n\
                 11 HOST.Net.TCPStream.SetTimeouts cannot set the timeouts of the TCP stream\n\
                 11 HOST.Net.TCPStream.ShutdownRead cannot shut down reading on the TCP stream\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_udp.bn",
        args: &[],
        covers: &[
            "HOST.Net.UDPBind",
            "HOST.Net.UDPSocket.LocalEndpoint",
            "HOST.Net.UDPSocket.SendTo",
            "HOST.Net.UDPSocket.Receive",
            "HOST.Net.UDPSocket.Close",
            "HOST.Net.UDPPacket.Size",
            "HOST.Net.UDPPacket.WasTruncated",
            "HOST.Net.UDPPacket.CopyTo",
            "HOST.Net.UDPPacket.Source",
        ],
        stdout: "TRUE TRUE 4\n2 TRUE 2 1 2\nfrom loopback: TRUE\n",
        exit: 0,
        stderr: None,
    },
    Case {
        fixture: "net_ping.bn",
        args: &[],
        covers: &[
            "HOST.Net.Ping",
            "HOST.Net.PingReply.Address",
            "HOST.Net.PingReply.RoundTripMicroseconds",
            "HOST.Net.Neighbor",
        ],
        stdout: "ping: TRUE TRUE\nneighbor answered: TRUE\n",
        exit: 0,
        stderr: None,
    },
];

fn fixture_path(case: &Case) -> std::path::PathBuf {
    workspace_root().join("tests/host").join(case.fixture)
}

fn program_args(case: &Case) -> Vec<String> {
    let bni_path = executable_path("bni").display().to_string();
    case.args
        .iter()
        .map(|arg| arg.replace("{bni}", &bni_path))
        .collect()
}

fn interpret(case: &Case, directory: &TestDir) -> ProcessOutput {
    let mut command = bni();
    command
        .current_dir(directory.path())
        .arg("run")
        .arg(fixture_path(case))
        .arg("--")
        .args(program_args(case));
    let mut output = run(&mut command, None, TIMEOUT).expect("run bni");
    output.accept_expected_status();
    output
}

fn assert_expected(case: &Case, output: &ProcessOutput, backend: &str) {
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        case.stdout,
        "{backend} stdout for {}",
        case.fixture
    );
    assert_eq!(
        output.status.code(),
        Some(case.exit),
        "{backend} exit for {}: {}",
        case.fixture,
        String::from_utf8_lossy(&output.stderr)
    );
    if let Some(code) = case.stderr {
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{backend} stderr for {} lacks {code}",
            case.fixture
        );
    }
}

/// The last segment of an operation (`Parse` in `HOST.Net.Address.Parse`) and
/// the capability name for the two non-member capabilities.
fn spelling(operation: &str) -> &str {
    match operation {
        "HOST.Args" => "HOST.Args",
        "HOST.NumProcs" => "NumProcs",
        _ => operation.rsplit('.').next().unwrap_or(operation),
    }
}

#[test]
fn every_host_operation_has_a_conformance_case() {
    let covered: BTreeSet<&str> = CASES.iter().flat_map(|case| case.covers).copied().collect();
    let missing: Vec<String> = bn_frontend::host_operation_names()
        .into_iter()
        .filter(|operation| !covered.contains(operation.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "HOST operations without a conformance case: {missing:?}"
    );
    let known: BTreeSet<String> = bn_frontend::host_operation_names().into_iter().collect();
    for operation in &covered {
        assert!(
            known.contains(*operation),
            "{operation} is covered but is not a HOST operation"
        );
    }
}

#[test]
fn every_covered_operation_appears_in_its_fixture() {
    for case in CASES {
        let source = fs::read_to_string(fixture_path(case)).expect("read fixture");
        for operation in case.covers {
            assert!(
                source.contains(spelling(operation)),
                "{} claims {operation} but never spells {}",
                case.fixture,
                spelling(operation)
            );
        }
    }
}

#[test]
fn host_fixtures_produce_their_exact_interpreter_results() {
    for case in CASES {
        let directory = TestDir::new("host-bni").expect("create directory");
        assert_expected(case, &interpret(case, &directory), "bni");
    }
}

#[test]
fn host_fixtures_match_natively() {
    for case in CASES {
        let directory = TestDir::new("host-bnc").expect("create directory");
        let artifact = directory.join(format!("program{}", std::env::consts::EXE_SUFFIX));
        let mut compile = bnc();
        compile.arg(fixture_path(case)).arg("-o").arg(&artifact);
        let built = run(&mut compile, None, TIMEOUT).expect("run bnc");
        assert!(
            built.status.success(),
            "bnc rejected {}: {}",
            case.fixture,
            String::from_utf8_lossy(&built.stderr)
        );
        let mut command = std::process::Command::new(&artifact);
        command
            .current_dir(directory.path())
            .args(program_args(case));
        let mut output = run(&mut command, None, TIMEOUT).expect("run native program");
        output.accept_expected_status();
        assert_expected(case, &output, "bnc");
    }
}
