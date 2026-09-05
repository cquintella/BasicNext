// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

const vscode = require("vscode");
const cp = require("child_process");
const os = require("os");
const path = require("path");

function shellQuote(value) {
  return process.platform === "win32"
    ? `"${value.replace(/"/g, '""')}"`
    : `'${value.replace(/'/g, "'\\''")}'`;
}

function activeDocument() {
  const document = vscode.window.activeTextEditor?.document;
  return document?.languageId === "basicnext" && !document.isUntitled ? document : undefined;
}

function terminal() {
  const terminal = vscode.window.createTerminal("Basic Next");
  terminal.show(true);
  return terminal;
}

function parseDiagnostics(output) {
  const diagnostics = [];
  const pattern = /^(error|warning)(?:\[[^\]]+\])?:\s*([^\n]*)\n\s*--> .*:(\d+):(\d+)$/gm;
  for (const match of output.matchAll(pattern)) {
    const severity = match[1] === "warning"
    ? vscode.DiagnosticSeverity.Warning
    : vscode.DiagnosticSeverity.Error;
    const line = Number(match[3]) - 1;
    const column = Number(match[4]) - 1;
    diagnostics.push(new vscode.Diagnostic(
      new vscode.Range(line, column, line, column + 1),
      match[2],
      severity,
    ));
  }
  return diagnostics;
}

function lspCompletionItems(result) {
  const items = Array.isArray(result) ? result : result?.items || [];
  return items.map((item) => {
    const completion = new vscode.CompletionItem(item.label, item.kind);
    if (item.detail) completion.detail = item.detail;
    return completion;
  });
}

function startLanguageServer(context, collection) {
  if (
    typeof cp.spawn !== "function" ||
    !vscode.languages.registerDefinitionProvider ||
    !vscode.languages.registerReferenceProvider ||
    !vscode.languages.registerHoverProvider ||
    !vscode.languages.registerDocumentSymbolProvider
  ) return undefined;
  const executable = vscode.workspace.getConfiguration("basicnext").get("executable", "bn");
  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const child = cp.spawn(executable, ["lsp"], { cwd, stdio: ["pipe", "pipe", "pipe"] });
  let buffer = Buffer.alloc(0);
  let nextId = 1;
  const pending = new Map();
  const send = (method, params) => {
    const id = nextId++;
    const payload = JSON.stringify({ jsonrpc: "2.0", id, method, params });
    child.stdin.write(`Content-Length: ${Buffer.byteLength(payload, "utf8")}\r\n\r\n${payload}`);
    return new Promise((resolve) => pending.set(id, resolve));
  };
  const notify = (method, params) => {
    const payload = JSON.stringify({ jsonrpc: "2.0", method, params });
    child.stdin.write(`Content-Length: ${Buffer.byteLength(payload, "utf8")}\r\n\r\n${payload}`);
  };
  const consume = () => {
    while (true) {
      const separator = buffer.indexOf("\r\n\r\n");
      if (separator < 0) return;
      const header = buffer.subarray(0, separator).toString("ascii");
      const length = Number(header.match(/Content-Length:\s*(\d+)/i)?.[1]);
      if (!Number.isSafeInteger(length) || length < 0 || buffer.length < separator + 4 + length) return;
      const message = JSON.parse(buffer.subarray(separator + 4, separator + 4 + length).toString("utf8"));
      buffer = buffer.subarray(separator + 4 + length);
      if (message.id !== undefined && pending.has(message.id)) {
        pending.get(message.id)(message.result ?? null);
        pending.delete(message.id);
      } else if (message.method === "textDocument/publishDiagnostics") {
        const diagnostics = (message.params.diagnostics || []).map((item) => new vscode.Diagnostic(
          new vscode.Range(item.range.start.line, item.range.start.character, item.range.end.line, item.range.end.character),
          item.message,
          item.severity === 2 ? vscode.DiagnosticSeverity.Warning : vscode.DiagnosticSeverity.Error,
        ));
        collection.set(vscode.Uri.parse(message.params.uri), diagnostics);
      }
    }
  };
  child.stdout.on("data", (chunk) => { buffer = Buffer.concat([buffer, chunk]); consume(); });
  child.on("error", () => {});
  const initialize = send("initialize", { processId: process.pid, rootUri: cwd ? vscode.Uri.file(cwd).toString() : null, capabilities: {} });
  initialize.then(() => notify("initialized", {}));
  const sync = (document, method = "textDocument/didOpen") => {
    if (document.languageId !== "basicnext" || document.isUntitled) return;
    const textDocument = { uri: document.uri.toString(), languageId: "basicnext", version: document.version, text: document.getText() };
    notify(method, method === "textDocument/didChange" ? { textDocument: { uri: textDocument.uri, version: textDocument.version }, contentChanges: [{ text: textDocument.text }] } : { textDocument });
  };
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(sync),
    vscode.workspace.onDidChangeTextDocument((event) => sync(event.document, "textDocument/didChange")),
    vscode.workspace.onDidCloseTextDocument((document) => notify("textDocument/didClose", { textDocument: { uri: document.uri.toString() } })),
    vscode.languages.registerDefinitionProvider("basicnext", { provideDefinition: (document, position) => send("textDocument/definition", { textDocument: { uri: document.uri.toString() }, position }).then((items) => items || []) }),
    vscode.languages.registerReferenceProvider("basicnext", { provideReferences: (document, position, context) => send("textDocument/references", { textDocument: { uri: document.uri.toString() }, position, context: { includeDeclaration: Boolean(context?.includeDeclaration) } }).then((items) => items || []) }),
    vscode.languages.registerHoverProvider("basicnext", { provideHover: (document, position) => send("textDocument/hover", { textDocument: { uri: document.uri.toString() }, position }) }),
    vscode.languages.registerDocumentSymbolProvider("basicnext", { provideDocumentSymbols: (document) => send("textDocument/documentSymbol", { textDocument: { uri: document.uri.toString() } }).then((items) => items || []) }),
    vscode.languages.registerCompletionItemProvider("basicnext", { provideCompletionItems: (document, position) => send("textDocument/completion", { textDocument: { uri: document.uri.toString() }, position }).then(lspCompletionItems) }, "."),
    { dispose: () => { notify("shutdown", null); notify("exit", null); child.kill(); } },
  );
  for (const document of vscode.workspace.textDocuments || []) sync(document);
  return child;
}

function activate(context) {
  const collection = vscode.languages.createDiagnosticCollection("basicnext");
  const run = async () => {
    const document = activeDocument();
    if (!document) return;
    if (document.isDirty && !await document.save()) return;
    const executable = vscode.workspace.getConfiguration("basicnext").get("executable", "bn");
    terminal().sendText(`${shellQuote(executable)} run ${shellQuote(document.fileName)}`);
  };
  const buildAndRun = async () => {
    const document = activeDocument();
    if (!document) return;
    if (document.isDirty && !await document.save()) return;
    const executable = vscode.workspace.getConfiguration("basicnext").get("executable", "bn");
    const extension = process.platform === "win32" ? ".exe" : "";
    const artifact = path.join(os.tmpdir(), `basicnext-${path.basename(document.fileName, ".bn")}${extension}`);
    terminal().sendText(`${shellQuote(executable)} build ${shellQuote(document.fileName)} -o ${shellQuote(artifact)} && ${shellQuote(artifact)}`);
  };
  context.subscriptions.push(
    collection,
    vscode.commands.registerCommand("basicnext.run", run),
    vscode.commands.registerCommand("basicnext.buildAndRun", buildAndRun),
  );
  startLanguageServer(context, collection);
}

function deactivate() {}

module.exports = { activate, deactivate, parseDiagnostics, shellQuote, startLanguageServer, lspCompletionItems };
