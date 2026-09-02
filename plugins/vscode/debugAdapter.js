// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

const cp = require("child_process");

let buffer = Buffer.alloc(0);
let child = null;
let initializeRequest = null;
const pendingMessages = [];
const MAX_FRAME_BYTES = 1024 * 1024;

function writeFrame(stream, message) {
  const body = Buffer.from(JSON.stringify(message));
  if (body.length > MAX_FRAME_BYTES) throw new Error("DAP frame exceeds 1 MiB");
  stream.write(`Content-Length: ${body.length}\r\n\r\n`);
  stream.write(body);
}

function sendToChild(message) {
  if (!message) return;
  if (child?.stdin?.writable) writeFrame(child.stdin, message);
  else pendingMessages.push(message);
}

function flushPendingMessages() {
  while (pendingMessages.length > 0 && child?.stdin?.writable) {
    writeFrame(child.stdin, pendingMessages.shift());
  }
}

function startChild(request) {
  const { program, cwd, runtimeExecutable = "bn" } = request.arguments ?? {};
  if (typeof program !== "string" || program.length === 0) {
    throw new Error("launch requires a program");
  }
  child = cp.spawn(runtimeExecutable, ["dap"], {
    cwd,
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stdout.on("data", (data) => process.stdout.write(data));
  child.stderr.on("data", (data) => process.stderr.write(data));
  child.on("error", (error) => {
    process.stderr.write(`bn dap failed: ${error.message}\n`);
  });
  child.on("close", (code, signal) => {
    if (code !== 0 || signal) process.exitCode = code ?? 1;
  });
  sendToChild(initializeRequest);
  sendToChild(request);
  flushPendingMessages();
}

function handle(message) {
  if (message.type !== "request") return;
  if (message.command === "initialize" && child === null) {
    initializeRequest = message;
    return;
  }
  if (message.command === "launch" && child === null) {
    try {
      startChild(message);
    } catch (error) {
      process.stderr.write(`${error.message}\n`);
      process.exitCode = 1;
    }
    return;
  }
  sendToChild(message);
}

process.stdin.on("data", (data) => {
  buffer = Buffer.concat([buffer, data]);
  while (true) {
    const boundary = buffer.indexOf("\r\n\r\n");
    if (boundary < 0) return;
    const header = buffer.subarray(0, boundary).toString();
    const length = Number(header.match(/Content-Length: (\d+)/i)?.[1]);
    const start = boundary + 4;
    if (buffer.length > start + MAX_FRAME_BYTES) {
      throw new Error("DAP input frame exceeds 1 MiB");
    }
    if (!Number.isFinite(length) || buffer.length < start + length) return;
    if (length < 0 || length > MAX_FRAME_BYTES) {
      throw new Error("invalid DAP content length");
    }
    try {
      handle(JSON.parse(buffer.subarray(start, start + length).toString()));
    } catch (error) {
      process.stderr.write(`invalid DAP message: ${error.message}\n`);
      process.exitCode = 1;
      return;
    }
    buffer = buffer.subarray(start + length);
  }
});
