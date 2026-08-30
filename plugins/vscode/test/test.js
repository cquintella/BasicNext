// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

const assert = require("assert");
const fs = require("fs");
const Module = require("module");
const path = require("path");

const root = path.resolve(__dirname, "../../..");
const extension = path.resolve(__dirname, "..");
const packageJson = JSON.parse(fs.readFileSync(path.join(extension, "package.json"), "utf8"));
assert.strictEqual(packageJson.contributes.grammars[0].path, "./syntaxes/basicnext.tmLanguage.json");
assert.strictEqual(packageJson.contributes.debuggers[0].type, "basicnext");
assert.deepStrictEqual(packageJson.contributes.debuggers[0].languages, ["basicnext"]);
assert.deepStrictEqual(
  JSON.parse(fs.readFileSync(path.join(root, "docs/library/basicnext.tmLanguage.json"), "utf8")),
  JSON.parse(fs.readFileSync(path.join(extension, "syntaxes/basicnext.tmLanguage.json"), "utf8")),
);
const originalLoad = Module._load;
const calls = [];
let saved;
const pending = [];
const commands = {};
const terminalLines = [];
let activeEditor;
let saveCount = 0;
Module._load = (request, parent, isMain) => {
  if (request === "vscode") {
    return {
      DiagnosticSeverity: { Error: 0, Warning: 1 },
      Range: class Range { constructor(...values) { this.values = values; } },
      Diagnostic: class Diagnostic { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
      languages: { createDiagnosticCollection: () => ({ set: (...values) => calls.push(values) }) },
      workspace: { getConfiguration: () => ({ get: () => "bn" }), getWorkspaceFolder: () => undefined, onDidSaveTextDocument: (listener) => { saved = listener; return { dispose() {} }; } },
      window: {
        get activeTextEditor() { return activeEditor; },
        createTerminal: () => ({ show() {}, sendText: (line) => terminalLines.push(line) }),
      },
      commands: { registerCommand: (name, command) => { commands[name] = command; return { dispose() {} }; } },
    };
  }
  if (request === "child_process") return { execFile: (...args) => pending.push(args.at(-1)) };
  return originalLoad(request, parent, isMain);
};
const { activate, parseDiagnostics } = require(path.join(extension, "extension.js"));
Module._load = originalLoad;
const diagnostics = parseDiagnostics("error[E100]: first\n --> sample.bn:2:5\nerror[E200]: second\n --> sample.bn:4:1\n");
assert.strictEqual(diagnostics.length, 2);
assert.deepStrictEqual(diagnostics.map((diagnostic) => [diagnostic.message, diagnostic.range.values]), [["first", [1, 4, 1, 5]], ["second", [3, 0, 3, 1]]]);
const document = { languageId: "basicnext", isUntitled: false, fileName: "sample.bn", uri: "sample", version: 1 };
activate({ subscriptions: { push() {} } });
saved(document);
document.version = 2;
saved(document);
pending[0](null, "", "error[E100]: stale\n --> sample.bn:1:1\n");
pending[1](null, "", "error[E200]: current\n --> sample.bn:2:1\n");
assert.strictEqual(calls.length, 1);
assert.strictEqual(calls[0][1][0].message, "current");
document.version = 3;
saved(document);
pending[2](Object.assign(new Error("not found"), { code: "ENOENT" }), "", "");
assert.match(calls[1][1][0].message, /cannot execute Basic Next checker 'bn'/);

(async () => {
  activeEditor = { document: {
    languageId: "basicnext",
    isUntitled: false,
    isDirty: true,
    fileName: "/tmp/example.bn",
    save: async () => { saveCount += 1; return true; },
  } };
  await commands["basicnext.run"]();
  await commands["basicnext.buildAndRun"]();
  assert.strictEqual(saveCount, 2);
  assert.match(terminalLines[0], /'bn' run '\/tmp\/example\.bn'/);
  assert.match(terminalLines[1], /'bn' build '\/tmp\/example\.bn' -o .* && /);
  console.log("Basic Next VS Code extension checks passed");
})().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
