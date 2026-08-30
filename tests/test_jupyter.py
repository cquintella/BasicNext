import hashlib
import hmac
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import unittest
import uuid

from bn_kernel.jupyter import KERNEL_INFO, valid_signature


DELIMITER = b"<IDS|MSG>"
LARGE_PRINT = (
    "FUNCTION Start() AS VOID\n"
    "FOR i AS INTEGER = 1 TO 2500\n"
    'PRINT "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"\n'
    "END FOR\n"
    "END FUNCTION\n"
)
BUSY_LOOP = "FUNCTION Start() AS VOID\nWHILE TRUE\nEND WHILE\nEND FUNCTION\n"
INPUT_CELL = (
    "FUNCTION Start() AS VOID\n"
    "LET line AS STRING OR EOF = INPUT()\n"
    "PRINT line\n"
    "END FUNCTION\n"
)


def signed_message(key: str, header: dict, content: dict, parent: dict | None = None) -> list[bytes]:
    frames = [
        json.dumps(value, separators=(",", ":")).encode()
        for value in (header, parent or {}, {}, content)
    ]
    signature = hmac.new(key.encode(), b"".join(frames), hashlib.sha256).hexdigest().encode()
    return [DELIMITER, signature, *frames]


def header(msg_type: str, session: str | None = None) -> dict:
    return {
        "msg_id": uuid.uuid4().hex,
        "session": session or uuid.uuid4().hex,
        "username": "test",
        "version": "5.3",
        "msg_type": msg_type,
    }


class JupyterProtocolTests(unittest.TestCase):
    def test_signature_verification_rejects_tampering(self):
        key = b"key"
        frames = [b"header", b"parent", b"metadata", b"content"]
        signature = hmac.new(key, b"".join(frames), hashlib.sha256).hexdigest().encode()
        self.assertTrue(valid_signature(key, signature, frames))
        self.assertFalse(valid_signature(key, signature, [*frames[:-1], b"changed"]))


class JupyterWireTests(unittest.TestCase):
    def setUp(self):
        import zmq

        def free_port():
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                return sock.getsockname()[1]

        self.zmq = zmq
        self.ports = {name: free_port() for name in ("shell", "control", "iopub", "stdin", "hb")}
        self.key = "integration-key"
        self.directory = tempfile.TemporaryDirectory()
        connection = Path(self.directory.name) / "connection.json"
        connection.write_text(
            json.dumps(
                {
                    "transport": "tcp",
                    "ip": "127.0.0.1",
                    "key": self.key,
                    **{f"{name}_port": port for name, port in self.ports.items()},
                }
            )
        )
        env = {
            **os.environ,
            "PYTHONPATH": str(Path(__file__).parents[1] / "plugins" / "jupyter"),
        }
        self.process = subprocess.Popen(
            [sys.executable, "-m", "bn_kernel", "-f", str(connection), "--bn", "target/debug/bn"],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.context = zmq.Context()
        identity = b"test-client"
        self.shell = self.context.socket(zmq.DEALER)
        self.control = self.context.socket(zmq.DEALER)
        self.stdin = self.context.socket(zmq.DEALER)
        for socket_name, port_name in ((self.shell, "shell"), (self.control, "control"), (self.stdin, "stdin")):
            socket_name.setsockopt(zmq.IDENTITY, identity)
            socket_name.connect(f"tcp://127.0.0.1:{self.ports[port_name]}")
        self.iopub = self.context.socket(zmq.SUB)
        self.iopub.connect(f"tcp://127.0.0.1:{self.ports['iopub']}")
        self.iopub.setsockopt(zmq.SUBSCRIBE, b"")
        self.hb = self.context.socket(zmq.REQ)
        self.hb.connect(f"tcp://127.0.0.1:{self.ports['hb']}")
        time.sleep(0.4)
        self.poller = zmq.Poller()
        for sock in (self.shell, self.control, self.iopub, self.stdin):
            self.poller.register(sock, zmq.POLLIN)

    def tearDown(self):
        for sock in (self.shell, self.control, self.stdin, self.iopub, self.hb):
            sock.close(0)
        self.context.term()
        if self.process.poll() is None:
            self.process.terminate()
            self.process.wait(timeout=5)
        self.process.stdout.close()
        self.process.stderr.close()
        self.directory.cleanup()

    def send(self, socket, msg_type, content=None, session=None):
        request = header(msg_type, session)
        socket.send_multipart(signed_message(self.key, request, content or {}))
        return request

    def pump(self, deadline, *, stop=None):
        seen = []
        contents = []
        while time.time() < deadline:
            events = dict(self.poller.poll(100))
            for sock, name in (
                (self.stdin, "stdin"),
                (self.iopub, "iopub"),
                (self.shell, "shell"),
                (self.control, "control"),
            ):
                if sock not in events:
                    continue
                message = sock.recv_multipart()
                msg_type = json.loads(message[-4]).get("msg_type")
                content = json.loads(message[-1])
                seen.append(msg_type)
                contents.append((name, msg_type, content, message))
                if stop and stop(seen, contents):
                    return seen, contents
        return seen, contents

    def test_kernel_info_handshake(self):
        self.send(self.shell, "kernel_info_request")
        seen, contents = self.pump(time.time() + 5, stop=lambda seen, _: "kernel_info_reply" in seen)
        self.assertIn("kernel_info_reply", seen)
        info = next(content for _, msg_type, content, _ in contents if msg_type == "kernel_info_reply")
        self.assertEqual(info["status"], "ok")
        self.assertEqual(info["protocol_version"], KERNEL_INFO["protocol_version"])
        self.assertEqual(info["implementation"], "bn")
        self.assertEqual(info["language_info"]["name"], "basicnext")
        self.assertEqual(info["language_info"]["file_extension"], ".bn")
        self.assertTrue(info["language_info"]["mimetype"].startswith("text/"))

    def test_input_value_and_cancelled_input(self):
        def execute(reply):
            request = self.send(self.shell, "execute_request", {"code": INPUT_CELL})
            seen = []
            deadline = time.time() + 5
            while time.time() < deadline and "execute_reply" not in seen:
                events = dict(self.poller.poll(100))
                if self.stdin in events:
                    message = self.stdin.recv_multipart()
                    if json.loads(message[-4]).get("msg_type") == "input_request":
                        content = {"value": reply} if reply is not None else {"status": "abort"}
                        self.stdin.send_multipart(
                            signed_message(self.key, header("input_reply", request["session"]), content)
                        )
                        seen.append("input_request")
                if self.iopub in events:
                    message = self.iopub.recv_multipart()
                    if json.loads(message[-4]).get("msg_type") == "stream":
                        seen.append(json.loads(message[-1])["text"])
                if self.shell in events:
                    message = self.shell.recv_multipart()
                    seen.append(json.loads(message[-4]).get("msg_type"))
            return seen

        seen = execute("from-jupyter")
        self.assertIn("input_request", seen)
        self.assertIn("from-jupyter\n", seen)
        self.assertIn("execute_reply", seen)
        cancelled = execute(None)
        self.assertIn("input_request", cancelled)
        self.assertIn("EOF\n", cancelled)

    def test_large_print_and_heartbeat(self):
        self.send(self.shell, "execute_request", {"code": LARGE_PRINT})
        self.hb.send(b"ping")
        self.assertTrue(self.hb.poll(2000), "heartbeat must reply during execute")
        self.assertEqual(self.hb.recv(), b"ping")
        seen, contents = self.pump(
            time.time() + 20,
            stop=lambda seen, _: "execute_reply" in seen,
        )
        self.assertIn("execute_reply", seen)
        stream = "".join(content["text"] for _, msg_type, content, _ in contents if msg_type == "stream")
        self.assertGreater(len(stream), 64 * 1024)

    def test_interrupt_stops_child(self):
        self.send(self.shell, "execute_request", {"code": BUSY_LOOP})
        time.sleep(0.3)
        self.send(self.control, "interrupt_request")
        seen, _ = self.pump(
            time.time() + 5,
            stop=lambda seen, _: "interrupt_reply" in seen and "execute_reply" in seen,
        )
        self.assertIn("interrupt_reply", seen)
        self.assertIn("execute_reply", seen)

    def test_shutdown_request(self):
        self.send(self.control, "shutdown_request", {"restart": False})
        seen, _ = self.pump(time.time() + 5, stop=lambda seen, _: "shutdown_reply" in seen)
        self.assertIn("shutdown_reply", seen)
        self.assertEqual(self.process.wait(timeout=5), 0)


if __name__ == "__main__":
    unittest.main()
