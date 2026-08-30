// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

let buffer = Buffer.alloc(0);
let sequence = 1;

function send(message) {
  const body = Buffer.from(JSON.stringify({ seq: sequence++, ...message }));
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function response(request, body = {}) {
  send({ type: "response", request_seq: request.seq, command: request.command, success: true, body });
}

function event(name, body = {}) {
  send({ type: "event", event: name, body });
}

function launch(request) {
  const { program, cwd, runtimeExecutable = "bn" } = request.arguments;
  response(request);
  send({ type: "request", command: "runInTerminal", arguments: {
    kind: "integrated",
    title: "Basic Next",
    cwd,
    args: [runtimeExecutable, "run", program],
  } });
}

function handle(message) {
  if (message.type !== "request") return;
  switch (message.command) {
    case "initialize":
      response(message, { supportsConfigurationDoneRequest: false });
      event("initialized");
      break;
    case "launch": launch(message); break;
    case "disconnect":
    case "terminate":
      response(message);
      event("terminated");
      break;
    default: response(message);
  }
}

process.stdin.on("data", (data) => {
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
