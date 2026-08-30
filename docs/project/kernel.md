# `bn-kernel` host

`bn-kernel` keeps the Rust crate dependency-free. The installable Python
package uses `pyzmq` for the Jupyter v5 wire transport. Each request writes
one complete Basic Next program to a fresh temporary `.bn` file and invokes
`bn run --no-filesystem --jupyter-stdin`; no declarations or process state
survive between cells.

The package API is `bn_kernel.execute_cell`. Without a connection file, the
`plugins/jupyter/bin/bn-kernel` launcher also supports JSON Lines
(`{"code": "...", "stdin": "..."}`). With a connection file (`-f`), it
serves the Jupyter wire.

## Wire protocol

The kernel answers:

- `kernel_info_request` → `kernel_info_reply` with protocol 5.3,
  `implementation` `bn`, and `language_info` (`name` `basicnext`,
  `file_extension` `.bn`, `mimetype` `text/x-basicnext`). JupyterLab and
  `jupyter_client.KernelClient.wait_for_ready()` require this handshake.
- `execute_request` → `execute_reply`, with stdout on IOPub `stream`.
- `shutdown_request` → `shutdown_reply`. The kernel process then exits.
- `interrupt_request` → `interrupt_reply` and SIGTERM on the `bn` child.
- Heartbeat is a dedicated `REP` thread, so execute and `INPUT()` do not
  starve Jupyter's ping.
- `INPUT()` → Jupyter `input_request` / `input_reply` on the stdin
  channel. The kernel does **not** read `execute_request.content.stdin`.
  `--jupyter-stdin` is the private marker contract between the kernel and
  `bn`; it is not a user-facing `bn run` flag.
- Child stdout and stderr are read concurrently so a large `PRINT` cannot
  fill a pipe and deadlock the kernel.

Unknown request types are ignored; the session stays up.

`PrintAt` / `NumCols` / `NumRows` fail **at the call** because kernel
stdout is not a TTY. `HOST.FileSystem` is denied by `--no-filesystem`
before `Start` when the import is present. `PRINT`, `INPUT()`, `Cls`, and
`Beep` still run.

Not in this kernel: accumulated declarations across cells, implicit
`Start`, HTML grid for `PrintAt`, an in-process Rust ZMQ kernel.
