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


/** Reserved words from docs/0.5.0/0.5.0.ebnf (uppercase spellings). */
const RESERVED_WORDS = new Set([
  "AND", "AS", "ASYNC", "AWAIT", "BOOLEAN", "BYTE", "CLASS", "CONST",
  "CONSTRUCTOR", "CONTINUE", "DATE", "DESTRUCTOR", "DIV", "EACH", "ELSE",
  "END", "EOF", "EXIT", "EXPORT", "EXTENDS", "FALSE", "FLOAT", "FLOAT32",
  "FLOAT64", "FOR", "FUNCTION", "HOST", "IF", "IMPLEMENTS", "IMPORT", "IN",
  "INPUT", "INT8", "INT16", "INT32", "INT64", "INTEGER", "INTERFACE", "IS",
  "LEN", "LET", "NA", "NEW", "NOT", "NULL", "OR", "PARALLEL", "POINTER",
  "PRINT", "PRIVATE", "PUBLIC", "RELEASE", "REPEAT", "RETURN", "SELF", "SHL",
  "SHR", "SIZEOF", "STATIC", "STEP", "STOP", "STRING", "STRUCT", "SUPER",
  "SYSTEM", "THEN", "TIME", "TIMESTAMP", "TIMEZONE", "TO", "TRUE", "UINT16",
  "UINT32", "UINT64", "UNTIL", "VOID", "WEAK", "WHILE", "XOR",
]);

const WORD_CHAR = /[A-Za-z0-9_]/
const BOUNDARY_INSERTED = /[\s\(\)\[\]\{\},;:\.+\-*\/=<>!&|^%~]/

/** True if column is inside // comment or "…" string on the line (simple scan). */
function inStringOrLineComment(lineText, column) {
  let inString = false;
  let i = 0;
  while (i < column && i < lineText.length) {
    const ch = lineText[i];
    if (!inString && ch === "/" && lineText[i + 1] === "/") return true;
    if (ch === "\\" && inString) {
      i += 2;
      continue;
    }
    if (ch === "\"") inString = !inString;
    i += 1;
  }
  return inString;
}

/**
 * If the edit just finished a reserved word (typed non-word after it, or the
 * word itself), return { start, end, upper } offsets in the document; else null.
 */
function reservedWordUppercaseEdit(documentText, change) {
  if (!change || typeof change.text !== "string") return null;
  const startOffset = (() => {
    // Prefer rangeOffset when present (VS Code TextDocumentContentChangeEvent)
    if (typeof change.rangeOffset === "number") return change.rangeOffset;
    return null;
  })();
  if (startOffset === null) return null;

  const inserted = change.text;
  if (inserted.length === 0) return null;

  // Case A: user typed a boundary char after a word — uppercase the word before.
  if (inserted.length === 1 && BOUNDARY_INSERTED.test(inserted) && !WORD_CHAR.test(inserted)) {
    const before = startOffset; // caret was here before insert; word ends here
    let end = before;
    let start = end;
    while (start > 0 && WORD_CHAR.test(documentText[start - 1])) start -= 1;
    if (start === end) return null;
    const word = documentText.slice(start, end);
    const upper = word.toUpperCase();
    if (!RESERVED_WORDS.has(upper) || word === upper) return null;
    // Ensure not inside string/comment: find line
    const lineStart = documentText.lastIndexOf("\n", start - 1) + 1;
    const lineEnd = documentText.indexOf("\n", start);
    const lineText = documentText.slice(lineStart, lineEnd < 0 ? documentText.length : lineEnd);
    const col = start - lineStart;
    if (inStringOrLineComment(lineText, col)) return null;
    return { start, end, upper };
  }

  // Case B: pasted or completed a whole word that is reserved (no trailing boundary yet)
  // Only when the insert is a single identifier token.
  if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(inserted)) {
    const upper = inserted.toUpperCase();
    if (!RESERVED_WORDS.has(upper) || inserted === upper) return null;
    const end = startOffset + inserted.length;
    // Require word boundary after (EOF or non-word) and before
    const beforeOk = startOffset === 0 || !WORD_CHAR.test(documentText[startOffset - 1]);
    const afterOk = end >= documentText.length || !WORD_CHAR.test(documentText[end]);
    // After our change, document already has inserted text at startOffset
    // But documentText passed should be AFTER the change (VS Code event order).
    if (!beforeOk || !afterOk) return null;
    const lineStart = documentText.lastIndexOf("\n", startOffset - 1) + 1;
    const lineEnd = documentText.indexOf("\n", startOffset);
    const lineText = documentText.slice(lineStart, lineEnd < 0 ? documentText.length : lineEnd);
    if (inStringOrLineComment(lineText, startOffset - lineStart)) return null;
    return { start: startOffset, end, upper };
  }

  return null;
}

function enableAutoUppercaseKeywords() {
  return vscode.workspace.getConfiguration("basicnext").get("autoUppercaseKeywords", true) !== false;
}

function registerAutoUppercaseKeywords(context) {
  let applying = false;
  const sub = vscode.workspace.onDidChangeTextDocument(async (event) => {
    if (applying) return;
    if (!enableAutoUppercaseKeywords()) return;
    const document = event.document;
    if (document.languageId !== "basicnext") return;
    if (!event.contentChanges || event.contentChanges.length !== 1) return;
    const change = event.contentChanges[0];
    const edit = reservedWordUppercaseEdit(document.getText(), change);
    if (!edit) return;
    const editor = vscode.window.visibleTextEditors.find((e) => e.document === document)
      || (vscode.window.activeTextEditor?.document === document ? vscode.window.activeTextEditor : undefined);
    if (!editor) return;
    applying = true;
    try {
      await editor.edit((builder) => {
        const start = document.positionAt(edit.start);
        const end = document.positionAt(edit.end);
        builder.replace(new vscode.Range(start, end), edit.upper);
      }, { undoStopBefore: false, undoStopAfter: false });
    } finally {
      applying = false;
    }
  });
  context.subscriptions.push(sub);
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
  registerAutoUppercaseKeywords(context);
  startLanguageServer(context, collection);
}

function deactivate() {}

module.exports = { activate, deactivate, parseDiagnostics, shellQuote, startLanguageServer, lspCompletionItems, RESERVED_WORDS, inStringOrLineComment, reservedWordUppercaseEdit };
