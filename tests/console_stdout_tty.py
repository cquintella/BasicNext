"""Stdout is a PTY (TTY); stdin is a pipe. NumCols/NumRows must still work."""

import fcntl
import os
import struct
import subprocess
import sys
import termios


def main() -> int:
    binary = os.environ["BN"]
    program = os.environ["BN_PROGRAM"]
    master, slave = os.openpty()
    fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
    process = subprocess.Popen(
        [binary, "run", program],
        stdin=subprocess.PIPE,
        stdout=slave,
        stderr=subprocess.PIPE,
    )
    os.close(slave)
    assert process.stdin is not None
    process.stdin.close()
    chunks = bytearray()
    while True:
        try:
            data = os.read(master, 4096)
        except OSError:
            break
        if not data:
            break
        chunks.extend(data)
        if b"\n" in chunks:
            break
    os.close(master)
    stderr = process.stderr.read() if process.stderr is not None else b""
    status = process.wait()
    sys.stdout.buffer.write(bytes(chunks))
    sys.stderr.buffer.write(stderr)
    return status


if __name__ == "__main__":
    raise SystemExit(main())
