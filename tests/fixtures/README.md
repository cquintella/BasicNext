# HOST.Exec fixture helper

`host_exec_helper.rs` is compiled as a real executable by the acceptance
harness (for example, `rustc --edition=2021 host_exec_helper.rs -o helper`).
Tests invoke the resulting path directly with `std::process::Command` / BN
`HOST.Exec.Run`; no shell, script interpreter, or mocked process provider is
involved.

## Modes

| Mode | Behavior |
| --- | --- |
| `stdout TEXT` | Write TEXT to stdout |
| `stderr TEXT` | Write TEXT to stderr |
| `status N` | Exit with status N |
| `argv …` | Echo remaining argv as `i=value` lines |
| `echo-stdin` | Read stdin to EOF and echo to stdout |
| `invalid-utf8` | Write non-UTF-8 bytes to stdout |
| `invalid-utf8-stderr` | Write non-UTF-8 bytes to stderr |
| `block` | Sleep 120 seconds (timeout fixture) |
| `touch PATH` | Create PATH with marker contents (side-effect) |
| `both SIZE` | Concurrently write SIZE bytes to stdout and stderr |
| `stdout-bytes N` | Write N bytes (`X`) to stdout |
| `cwd` | Print process current working directory |
| `env KEY` | Print value of environment variable KEY |
| `signal` | Raise SIGTERM to self (Unix); else exit 99 |
