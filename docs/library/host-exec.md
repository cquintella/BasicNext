# HOST.Exec (0.5.1)

`HOST.Exec.Run(program AS STRING, args AS STRING[])` starts an operating-system
executable (an explicit path or a name resolved through `PATH`) without a shell
and waits synchronously for completion. The BN process remains alive; this is
not POSIX `execve`, and it does not interpret `.bn` source. Standard input is EOF;
stdout and stderr are captured independently as UTF-8 strings. A non-zero child
exit status is a successful `HOST.Exec.Result`, not a language error.

The interpreter applies a 60-second wall-clock timeout and a 16 MiB capture
ceiling per stream. Invalid arguments, spawn/wait failures, policy denial,
invalid UTF-8, timeout, and capture overflow return `Error` with the stable
codes 1–11 defined by `todo/proposals/host-exec-0.5.1.md`. Policy is checked
before spawning. The LLVM backend does not advertise this capability until a
versioned native ABI and parity fixtures are accepted.

Result fields are immutable: `ReturnCode AS INT64`, `Stdout AS STRING`, and
`Stderr AS STRING`.
