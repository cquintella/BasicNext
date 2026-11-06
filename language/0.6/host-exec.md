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
codes 1–11 listed in the [0.5 language specification](0.6.md#hostexec-051)
Policy is checked before
spawning and re-checked by `bn_rt` at the spawn boundary on the compiled
path. Both the interpreter and the native (LLVM) backend implement the
capability with the same observables (fixtures E01–E14).

The process tree that a timeout ends is the child and every process it started
that is still in the child's process group (POSIX) or job object (Windows);
the child leads that group, or is assigned to that job, at the start of the
call. A process that left the group or job on purpose (`setsid`, a job
breakaway) is outside the tree. When the call ends without a timeout, a
process still running in the tree is left alone. A signal that ends the program
with its default action (`SIGINT`, `SIGTERM`, `SIGHUP` on POSIX) also ends the
tree of every call still running; a handler the program's host installed for
those signals is kept.

Result fields are immutable: `ReturnCode AS INT64`, `Stdout AS STRING`, and
`Stderr AS STRING`.
