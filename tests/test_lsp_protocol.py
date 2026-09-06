import json
import pathlib
import subprocess
import unittest


ROOT = pathlib.Path(__file__).parents[1]
BN = ROOT / "target" / "debug" / "bn"


class LspProtocolTests(unittest.TestCase):
    def test_advertised_requests_and_full_sync_change(self):
        process = subprocess.Popen([BN, "lsp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE)

        def send(message):
            payload = json.dumps(message).encode()
            process.stdin.write(f"Content-Length: {len(payload)}\r\n\r\n".encode() + payload)
            process.stdin.flush()

        def receive():
            headers = b""
            while b"\r\n\r\n" not in headers:
                headers += process.stdout.read(1)
            length = int(next(line for line in headers.decode().split("\r\n") if line.lower().startswith("content-length")) .split(":")[1])
            return json.loads(process.stdout.read(length))

        def receive_id(request_id):
            while True:
                message = receive()
                if message.get("id") == request_id:
                    return message

        uri = "file:///tmp/basic-next-lsp.bn"
        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}})
        initialized = receive()
        self.assertIn("completionProvider", initialized["result"])
        send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
        text = "FUNCTION Start() AS VOID\n    PRINT \"ok\"\nEND FUNCTION\n"
        send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {"uri": uri, "languageId": "basicnext", "version": 1, "text": text}}})
        send({"jsonrpc": "2.0", "id": 2, "method": "textDocument/documentSymbol", "params": {"textDocument": {"uri": uri}}})
        self.assertIsInstance(receive_id(2)["result"], list)
        send({"jsonrpc": "2.0", "id": 3, "method": "textDocument/completion", "params": {"textDocument": {"uri": uri}, "position": {"line": 1, "character": 4}}})
        self.assertIn("result", receive_id(3))
        changed = text.replace("ok", "changed")
        send({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {"textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": changed}]}})
        send({"jsonrpc": "2.0", "id": 4, "method": "textDocument/hover", "params": {"textDocument": {"uri": uri}, "position": {"line": 0, "character": 9}}})
        self.assertIn("result", receive_id(4))
        send({"jsonrpc": "2.0", "id": 5, "method": "shutdown", "params": None})
        self.assertIsNone(receive_id(5)["result"])
        send({"jsonrpc": "2.0", "method": "exit", "params": None})
        process.stdin.close()
        try:
            process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=5)
        self.assertIn(process.returncode, (0, -15))
        process.stdout.close()


if __name__ == "__main__":
    unittest.main()
