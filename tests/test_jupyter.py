import hashlib
import hmac
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest

import zmq

from bn_kernel.jupyter import DELIMITER, JupyterKernel


ROOT = Path(__file__).resolve().parents[1]
BN = str(ROOT / "target" / "debug" / "bn")


def free_port(context):
    probe = context.socket(zmq.REP)
    port = probe.bind_to_random_port("tcp://127.0.0.1")
    probe.close(0)
    return port


def message(msg_type, content, *, key=b"", session="test"):
    header = {
        "msg_id": msg_type,
        "username": "test",
        "session": session,
        "date": "",
        "msg_type": msg_type,
        "version": "5.3",
    }
    parent = {"session": session}
    frames = [
        json.dumps(value, separators=(",", ":")).encode()
        for value in (header, parent, {}, content)
    ]
    signature = hmac.new(key, b"".join(frames), hashlib.sha256).hexdigest().encode()
    return [DELIMITER, signature, *frames]


def receive(socket, timeout=5000):
    poller = zmq.Poller()
    poller.register(socket, zmq.POLLIN)
    events = dict(poller.poll(timeout))
    if socket not in events:
        raise AssertionError("timed out waiting for Jupyter message")
    frames = socket.recv_multipart()
    delimiter = frames.index(DELIMITER)
    payload = frames[delimiter + 2 :]
    return json.loads(payload[0].decode()), json.loads(payload[3].decode())


class JupyterWireTests(unittest.TestCase):
    def test_program_execution_and_shutdown_use_the_wire_contract(self):
        context = zmq.Context()
        with tempfile.TemporaryDirectory(prefix="basicnext-jupyter-test-") as directory:
            ports = {channel: free_port(context) for channel in ("shell", "control", "stdin", "hb", "iopub")}
            connection = Path(directory) / "connection.json"
            connection.write_text(
                json.dumps(
                    {
                        "transport": "tcp",
                        "ip": "127.0.0.1",
                        "key": "",
                        **{f"{channel}_port": port for channel, port in ports.items()},
                    }
                ),
                encoding="utf-8",
            )
            kernel = JupyterKernel(connection, bn=BN)
            thread = threading.Thread(target=kernel.serve)
            thread.start()
            shell = context.socket(zmq.DEALER)
            shell.setsockopt(zmq.IDENTITY, b"shell-client")
            shell.connect(f"tcp://127.0.0.1:{ports['shell']}")
            control = context.socket(zmq.DEALER)
            control.setsockopt(zmq.IDENTITY, b"control-client")
            control.connect(f"tcp://127.0.0.1:{ports['control']}")
            iopub = context.socket(zmq.SUB)
            iopub.setsockopt(zmq.SUBSCRIBE, b"")
            iopub.connect(f"tcp://127.0.0.1:{ports['iopub']}")
            time.sleep(0.05)
            try:
                shell.send_multipart(message("kernel_info_request", {}))
                header, content = receive(shell)
                self.assertEqual(header["msg_type"], "kernel_info_reply")
                self.assertEqual(content["implementation"], "bn")

                code = 'FUNCTION Start() AS VOID\nPRINT "program"\nEND FUNCTION\n'
                shell.send_multipart(message("execute_request", {"code": code}))
                stream_header, stream = receive(iopub)
                while stream_header["msg_type"] != "stream":
                    stream_header, stream = receive(iopub)
                self.assertEqual(stream_header["msg_type"], "stream")
                self.assertEqual(stream["text"], "program\n")
                reply_header, reply = receive(shell)
                self.assertEqual(reply_header["msg_type"], "execute_reply")
                self.assertEqual(reply["status"], "ok")

                control.send_multipart(message("shutdown_request", {"restart": False}))
                shutdown_header, shutdown = receive(control)
                self.assertEqual(shutdown_header["msg_type"], "shutdown_reply")
                self.assertFalse(shutdown["restart"])
            finally:
                shell.close(0)
                control.close(0)
                iopub.close(0)
                kernel._alive = False
                thread.join(timeout=2)
                for socket in kernel._sockets.values():
                    socket.close(0)
                kernel._context.term()
                context.term()
        self.assertFalse(thread.is_alive())


if __name__ == "__main__":
    unittest.main()
