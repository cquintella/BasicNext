# Author: Carlos Quintella
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import annotations

import hashlib
import hmac
import json
from pathlib import Path
import subprocess
import tempfile
import threading
import uuid

DELIMITER = b"<IDS|MSG>"
INPUT_REQUEST_MARKER = "\x1eBN_INPUT_REQUEST\n"
KERNEL_INFO = {
    "status": "ok",
    "protocol_version": "5.3",
    "implementation": "bn",
    "implementation_version": "0.4.2",
    "language_info": {
        "name": "basicnext",
    "version": "0.4.2",
        "mimetype": "text/x-basicnext",
        "file_extension": ".bn",
    },
    "banner": "Basic Next",
    "help_links": [],
}


def valid_signature(key: bytes, signature: bytes, frames: list[bytes]) -> bool:
    expected = hmac.new(key, b"".join(frames), hashlib.sha256).hexdigest().encode()
    return hmac.compare_digest(signature, expected)


class JupyterKernel:
    """Minimal Jupyter v5 kernel using Python's optional pyzmq package."""

    def __init__(self, connection_file: str | Path, *, bn: str = "bn") -> None:
        import zmq  # type: ignore[import-not-found]

        config = json.loads(Path(connection_file).read_text(encoding="utf-8"))
        self._zmq = zmq
        self._context = zmq.Context()
        self._config = config
        self._key = config.get("key", "").encode()
        self._bn = bn
        self._execution_count = 0
        self._alive = True
        self._child = None
        self._heartbeat_thread = None
        self._sockets = {}
        for channel in ("shell", "control", "iopub", "stdin", "hb"):
            kind = (
                zmq.ROUTER
                if channel in ("shell", "control", "stdin")
                else zmq.PUB
                if channel == "iopub"
                else zmq.REP
            )
            socket = self._context.socket(kind)
            socket.bind(f"{config.get('transport', 'tcp')}://{config['ip']}:{config[channel + '_port']}")
            self._sockets[channel] = socket

    def _message(self, identities: list[bytes], frames: list[bytes]) -> tuple[list[bytes], dict, dict]:
        header, parent, metadata, content = (json.loads(frame.decode()) for frame in frames[:4])
        return identities, header, content

    def _send(self, channel: str, identities: list[bytes], msg_type: str, content: dict, parent: dict) -> None:
        header = {
            "msg_id": uuid.uuid4().hex,
            "username": "bn",
            "session": parent.get("session", ""),
            "date": "",
            "msg_type": msg_type,
            "version": "5.3",
        }
        frames = [json.dumps(value, separators=(",", ":")).encode() for value in (header, parent, {}, content)]
        signature = hmac.new(self._key, b"".join(frames), hashlib.sha256).hexdigest().encode()
        self._sockets[channel].send_multipart(identities + [DELIMITER, signature, *frames])

    def _recv(self, socket):
        frames = socket.recv_multipart()
        delimiter = frames.index(DELIMITER)
        payload = frames[delimiter + 2 :]
        if not valid_signature(self._key, frames[delimiter + 1], payload):
            return None
        return self._message(frames[:delimiter], payload)

    def _heartbeat_loop(self) -> None:
        socket = self._sockets["hb"]
        poller = self._zmq.Poller()
        poller.register(socket, self._zmq.POLLIN)
        while self._alive:
            if poller.poll(100):
                socket.send(socket.recv())

    def _close_sockets(self) -> None:
        for socket in self._sockets.values():
            socket.close(0)

    def _stop_child(self) -> None:
        child = self._child
        if child is None:
            return
        child.terminate()
        try:
            child.wait(timeout=2)
        except subprocess.TimeoutExpired:
            child.kill()

    def _input_reply(self, identities: list[bytes], parent: dict, process: subprocess.Popen) -> str | None:
        self._send("stdin", identities, "input_request", {"prompt": "", "password": False}, parent)
        poller = self._zmq.Poller()
        poller.register(self._sockets["stdin"], self._zmq.POLLIN)
        while True:
            if process.poll() is not None:
                return None
            if not poller.poll(100):
                continue
            try:
                received = self._recv(self._sockets["stdin"])
            except (ValueError, json.JSONDecodeError):
                continue
            if received is None:
                continue
            _, header, content = received
            if header.get("msg_type") == "input_reply":
                return None if content.get("status") == "abort" else str(content.get("value", ""))

    def _execute(self, source: str, identities: list[bytes], parent: dict):
        with tempfile.TemporaryDirectory(prefix="basicnext-kernel-") as directory:
            path = Path(directory) / "cell.bn"
            path.write_text(source, encoding="utf-8")
            process = subprocess.Popen(
                [self._bn, "run", "--no-filesystem", "--jupyter-stdin", str(path)],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self._child = process
            output = []
            errors = []

            def read_stdout() -> None:
                output.append(process.stdout.read())

            def read_stderr() -> None:
                while True:
                    line = process.stderr.readline()
                    if not line:
                        break
                    if line == INPUT_REQUEST_MARKER:
                        reply = self._input_reply(identities, parent, process)
                        if process.stdin is None or process.poll() is not None:
                            continue
                        if reply is None:
                            process.stdin.close()
                        else:
                            process.stdin.write(reply + "\n")
                            process.stdin.flush()
                    else:
                        errors.append(line)

            stdout_thread = threading.Thread(target=read_stdout)
            stderr_thread = threading.Thread(target=read_stderr)
            stdout_thread.start()
            stderr_thread.start()
            try:
                while stdout_thread.is_alive() or stderr_thread.is_alive():
                    if self._dispatch("control", timeout_ms=50) == "shutdown":
                        self._stop_child()
                        break
                    stdout_thread.join(0.05)
                    stderr_thread.join(0.05)
            finally:
                stdout_thread.join()
                stderr_thread.join()
                returncode = process.wait()
                for stream in (process.stdin, process.stdout, process.stderr):
                    if stream is not None:
                        stream.close()
                self._child = None
        return output[0] if output else "", "".join(errors) or None, returncode

    def _dispatch(self, channel: str, *, timeout_ms: int | None = None) -> str | None:
        socket = self._sockets[channel]
        poller = self._zmq.Poller()
        poller.register(socket, self._zmq.POLLIN)
        if timeout_ms is None:
            events = poller.poll()
        else:
            events = poller.poll(timeout_ms)
        if not events:
            return None
        try:
            received = self._recv(socket)
        except (ValueError, json.JSONDecodeError):
            return None
        if received is None:
            return None
        identities, header, content = received
        msg_type = header.get("msg_type")
        if msg_type == "kernel_info_request":
            self._send(channel, identities, "kernel_info_reply", KERNEL_INFO, header)
            return "kernel_info"
        if msg_type == "interrupt_request":
            self._stop_child()
            self._send(channel, identities, "interrupt_reply", {"status": "ok"}, header)
            return "interrupt"
        if msg_type == "shutdown_request":
            self._alive = False
            self._stop_child()
            self._send(channel, identities, "shutdown_reply", {"restart": False}, header)
            return "shutdown"
        if msg_type == "execute_request" and channel == "shell":
            self._send("iopub", [], "status", {"execution_state": "busy"}, header)
            output, error, returncode = self._execute(content.get("code", ""), identities, header)
            if output:
                self._send("iopub", [], "stream", {"name": "stdout", "text": output}, header)
            self._execution_count += 1
            reply = {
                "status": "ok" if returncode == 0 else "error",
                "execution_count": self._execution_count,
                "user_expressions": {},
                "payload": {},
            }
            if error:
                reply.update({"ename": "BasicNextError", "evalue": error, "traceback": [error]})
            self._send("shell", identities, "execute_reply", reply, header)
            self._send("iopub", [], "status", {"execution_state": "idle"}, header)
            return "execute"
        return None

    def serve(self) -> None:
        self._heartbeat_thread = threading.Thread(target=self._heartbeat_loop, daemon=True)
        self._heartbeat_thread.start()
        poller = self._zmq.Poller()
        poller.register(self._sockets["shell"], self._zmq.POLLIN)
        poller.register(self._sockets["control"], self._zmq.POLLIN)
        while self._alive:
            events = dict(poller.poll(100))
            if self._sockets["control"] in events and self._dispatch("control", timeout_ms=0) == "shutdown":
                break
            if self._sockets["shell"] in events and self._dispatch("shell", timeout_ms=0) == "shutdown":
                break
        heartbeat = self._heartbeat_thread
        if heartbeat is not None and heartbeat is not threading.current_thread():
            heartbeat.join(timeout=1)
        self._close_sockets()
