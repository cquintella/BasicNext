// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

const assert = require("assert");
const cp = require("child_process");
const path = require("path");

const root = path.resolve(__dirname, "../../..");
const adapter = cp.spawn(process.execPath, [path.join(root, "plugins/vscode/debugAdapter.js")]);
let buffer = Buffer.alloc(0);
let sequence = 1;
const messages = [];
let terminalResponded = false;

function send(command, arguments) {
  const body = Buffer.from(JSON.stringify({ seq: sequence++, type: "request", command, arguments }));
  adapter.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
  adapter.stdin.write(body);
}

function respond(message) {
  const body = Buffer.from(JSON.stringify({ seq: sequence++, type: "response", request_seq: message.seq, command: message.command, success: true, body: { processId: 1 } }));
  adapter.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
  adapter.stdin.write(body);
}

adapter.stdout.on("data", (data) => {
  buffer = Buffer.concat([buffer, data]);
  while (true) {
    const boundary = buffer.indexOf("\r\n\r\n");
    if (boundary < 0) return;
    const length = Number(buffer.subarray(0, boundary).toString().match(/Content-Length: (\d+)/i)[1]);
    const start = boundary + 4;
    if (buffer.length < start + length) return;
    messages.push(JSON.parse(buffer.subarray(start, start + length).toString()));
    buffer = buffer.subarray(start + length);
    const message = messages.at(-1);
    if (message.type === "request" && message.command === "runInTerminal") {
      assert.deepStrictEqual(message.arguments.args.slice(1), ["run", path.join(root, "examples/hello.bn")]);
      assert.strictEqual(message.arguments.kind, "integrated");
      const [executable, ...arguments] = message.arguments.args;
      const result = cp.spawnSync(executable, arguments, { cwd: message.arguments.cwd, encoding: "utf8" });
      assert.strictEqual(result.status, 0);
      assert.match(result.stdout, /Basic Next0/);
      respond(message);
      terminalResponded = true;
      setTimeout(() => {
        assert(!messages.some((item) => item.event === "terminated"));
        send("disconnect", {});
      }, 50);
    }
    if (messages.some((message) => message.event === "terminated")) {
      assert(terminalResponded);
      assert(messages.some((message) => message.command === "initialize" && message.success));
      assert(messages.some((message) => message.command === "runInTerminal"));
      adapter.kill();
      console.log("Basic Next debug adapter checks passed");
    }
  }
});

adapter.on("close", () => {
  if (!messages.some((message) => message.event === "terminated")) process.exitCode = 1;
});

send("initialize", {});
send("launch", {
  program: path.join(root, "examples/hello.bn"),
  cwd: root,
  runtimeExecutable: path.join(root, "target/debug/bn"),
});
