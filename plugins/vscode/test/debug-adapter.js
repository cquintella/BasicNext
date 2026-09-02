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
let continued = false;
let finished = false;

setTimeout(() => {
  if (!finished) {
    console.error(`debug adapter timeout; received ${messages.length} messages`);
    process.exitCode = 1;
    adapter.kill();
  }
}, 5000);

function send(command, args) {
  const body = Buffer.from(JSON.stringify({ seq: sequence++, type: "request", command, arguments: args }));
  adapter.stdin.write(`Content-Length: ${body.length}\r\n\r\n`);
  adapter.stdin.write(body);
}

function handle(message) {
  messages.push(message);
  if (message.type === "event" && message.event === "stopped" && !continued) {
    continued = true;
    send("continue", { threadId: 1 });
  }
  if (message.type === "event" && message.event === "terminated" && !finished) {
    finished = true;
    assert(!messages.some((item) => item.type === "request" && item.command === "runInTerminal"));
    assert(messages.some((item) => item.command === "initialize" && item.success));
    assert(messages.some((item) => item.command === "launch" && item.success));
    assert(messages.some((item) => item.command === "configurationDone" && item.success));
    adapter.kill();
  }
}

adapter.stdout.on("data", (data) => {
  buffer = Buffer.concat([buffer, data]);
  while (true) {
    const boundary = buffer.indexOf("\r\n\r\n");
    if (boundary < 0) return;
    const header = buffer.subarray(0, boundary).toString();
    const length = Number(header.match(/Content-Length: (\d+)/i)?.[1]);
    const start = boundary + 4;
    if (!Number.isFinite(length) || buffer.length < start + length) return;
    handle(JSON.parse(buffer.subarray(start, start + length).toString()));
    buffer = buffer.subarray(start + length);
  }
});

adapter.on("close", () => {
  if (!finished) process.exitCode = 1;
  else console.log("Basic Next debug adapter checks passed");
});

send("initialize", {});
send("configurationDone", {});
send("launch", {
  program: path.join(root, "examples/hello.bn"),
  cwd: root,
  runtimeExecutable: path.join(root, "target/debug/bn"),
});
