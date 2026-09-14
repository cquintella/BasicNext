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
    ? `"${String(value).replace(/"/g, '""')}"`
    : `'${String(value).replace(/'/g, "'\\''")}'`;
}

function shellQuoteArgs(args) {
  return (Array.isArray(args) ? args : []).map(shellQuote).join(" ");
}

function cfg() {
  return vscode.workspace.getConfiguration("basicnext");
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

const SEVERITY_RANK = { error: 0, warning: 1, hint: 2 };

function diagnosticSeverityRank(severity) {
  if (severity === vscode.DiagnosticSeverity.Error) return 0;
  if (severity === vscode.DiagnosticSeverity.Warning) return 1;
  return 2;
}

function filterDiagnosticsBySeverity(diagnostics, minimum) {
  const minRank = SEVERITY_RANK[minimum] ?? SEVERITY_RANK.warning;
  return (diagnostics || []).filter((d) => diagnosticSeverityRank(d.severity) <= minRank);
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

const END_BLOCK_KEYWORDS = new Set([
  "FUNCTION", "WHILE", "IF", "FOR", "CLASS", "STRUCT", "INTERFACE",
  "CONSTRUCTOR", "DESTRUCTOR",
]);

const WORD_CHAR = /[A-Za-z0-9_]/
const BOUNDARY_INSERTED = /[\s\(\)\[\]\{\},;:\.+\-*\/=<>!&|^%~]/

const BLOCK_COLOR_DEFAULTS = {
  function: "#C586C0",
  while: "#D7BA7D",
  for: "#CE9178",
  repeat: "#DCDCAA",
  conditional: "#569CD6",
  class: "#4EC9B0",
  flow: "#F44747",
  await: "#B267E6",
};

const BLOCK_COLOR_SCOPES = {
  function: { scope: "storage.type.function.bn", fontStyle: "bold" },
  while: { scope: "keyword.control.loop.while.bn", fontStyle: "bold" },
  for: { scope: "keyword.control.loop.for.bn", fontStyle: "bold" },
  repeat: { scope: "keyword.control.loop.repeat.bn", fontStyle: "bold" },
  conditional: { scope: "keyword.control.conditional.bn", fontStyle: "bold" },
  class: { scope: "storage.type.class.bn", fontStyle: "bold" },
  flow: { scope: "keyword.control.flow.bn", fontStyle: undefined },
  await: { scope: "keyword.control.await.bn", fontStyle: "bold" },
};

const OWNED_BLOCK_SCOPES = new Set(Object.values(BLOCK_COLOR_SCOPES).map((v) => v.scope));

const HEX_COLOR = /^#([0-9A-Fa-f]{6}|[0-9A-Fa-f]{3})$/;

function normalizeHexColor(value, fallback) {
  if (typeof value === "string" && HEX_COLOR.test(value.trim())) return value.trim();
  return fallback;
}

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

function exclusionSet(exclusions) {
  return new Set((Array.isArray(exclusions) ? exclusions : []).map((s) => String(s).toUpperCase()));
}

/**
 * If the edit just finished a reserved word (typed non-word after it, or the
 * word itself), return { start, end, upper } offsets in the document; else null.
 * options: { exclusions?: string[], allowPaste?: boolean }
 */
function reservedWordUppercaseEdit(documentText, change, options = {}) {
  if (!change || typeof change.text !== "string") return null;
  const startOffset = (() => {
    if (typeof change.rangeOffset === "number") return change.rangeOffset;
    return null;
  })();
  if (startOffset === null) return null;

  const inserted = change.text;
  if (inserted.length === 0) return null;
  const exclusions = exclusionSet(options.exclusions);
  const allowPaste = options.allowPaste !== false;

  // Case A: user typed a boundary char after a word — uppercase the word before.
  if (inserted.length === 1 && BOUNDARY_INSERTED.test(inserted) && !WORD_CHAR.test(inserted)) {
    const before = startOffset;
    let end = before;
    let start = end;
    while (start > 0 && WORD_CHAR.test(documentText[start - 1])) start -= 1;
    if (start === end) return null;
    const word = documentText.slice(start, end);
    const upper = word.toUpperCase();
    if (!RESERVED_WORDS.has(upper) || word === upper) return null;
    if (exclusions.has(upper)) return null;
    const lineStart = documentText.lastIndexOf("\n", start - 1) + 1;
    const lineEnd = documentText.indexOf("\n", start);
    const lineText = documentText.slice(lineStart, lineEnd < 0 ? documentText.length : lineEnd);
    const col = start - lineStart;
    if (inStringOrLineComment(lineText, col)) return null;
    return { start, end, upper };
  }

  // Case B: pasted or completed a whole word that is reserved (no trailing boundary yet)
  if (allowPaste && /^[A-Za-z_][A-Za-z0-9_]*$/.test(inserted)) {
    const upper = inserted.toUpperCase();
    if (!RESERVED_WORDS.has(upper) || inserted === upper) return null;
    if (exclusions.has(upper)) return null;
    const end = startOffset + inserted.length;
    const beforeOk = startOffset === 0 || !WORD_CHAR.test(documentText[startOffset - 1]);
    const afterOk = end >= documentText.length || !WORD_CHAR.test(documentText[end]);
    if (!beforeOk || !afterOk) return null;
    const lineStart = documentText.lastIndexOf("\n", startOffset - 1) + 1;
    const lineEnd = documentText.indexOf("\n", startOffset);
    const lineText = documentText.slice(lineStart, lineEnd < 0 ? documentText.length : lineEnd);
    if (inStringOrLineComment(lineText, startOffset - lineStart)) return null;
    return { start: startOffset, end, upper };
  }

  return null;
}

/**
 * Format Basic Next source: uppercase reserved words outside strings/comments
 * and normalize `END   KEYWORD` → `END KEYWORD` for block closers.
 * Returns the full formatted text (pure).
 */
function formatBasicNextSource(text) {
  if (typeof text !== "string" || text.length === 0) return text || "";
  let out = "";
  let i = 0;
  let inString = false;
  let inLineComment = false;
  let inBlockComment = false;

  const flushWord = (start, end) => {
    const word = text.slice(start, end);
    const upper = word.toUpperCase();
    if (RESERVED_WORDS.has(upper) && word !== upper) return upper;
    return word;
  };

  while (i < text.length) {
    const ch = text[i];
    const next = text[i + 1];

    if (inLineComment) {
      out += ch;
      if (ch === "\n") inLineComment = false;
      i += 1;
      continue;
    }
    if (inBlockComment) {
      out += ch;
      if (ch === "*" && next === "/") {
        out += next;
        i += 2;
        inBlockComment = false;
        continue;
      }
      i += 1;
      continue;
    }
    if (inString) {
      out += ch;
      if (ch === "\\") {
        if (next !== undefined) {
          out += next;
          i += 2;
          continue;
        }
      } else if (ch === "\"") {
        inString = false;
      }
      i += 1;
      continue;
    }

    if (ch === "/" && next === "/") {
      inLineComment = true;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === "/" && next === "*") {
      inBlockComment = true;
      out += ch;
      i += 1;
      continue;
    }
    if (ch === "\"") {
      inString = true;
      out += ch;
      i += 1;
      continue;
    }

    // Normalize END   KEYWORD
    if ((i === 0 || !WORD_CHAR.test(text[i - 1])) && /^END\b/i.test(text.slice(i, i + 3))) {
      const endTok = text.slice(i, i + 3);
      let j = i + 3;
      let spaces = "";
      while (j < text.length && (text[j] === " " || text[j] === "\t")) {
        spaces += text[j];
        j += 1;
      }
      const wordMatch = text.slice(j).match(/^[A-Za-z_][A-Za-z0-9_]*/);
      if (spaces.length > 0 && wordMatch) {
        const kw = wordMatch[0];
        const upperKw = kw.toUpperCase();
        if (END_BLOCK_KEYWORDS.has(upperKw)) {
          out += "END " + upperKw;
          i = j + kw.length;
          continue;
        }
      }
      // bare END or non-block — still uppercase END
      out += endTok.toUpperCase() === "END" ? "END" : flushWord(i, i + 3);
      i += 3;
      continue;
    }

    if (/[A-Za-z_]/.test(ch)) {
      let j = i + 1;
      while (j < text.length && WORD_CHAR.test(text[j])) j += 1;
      out += flushWord(i, j);
      i = j;
      continue;
    }

    out += ch;
    i += 1;
  }
  return out;
}

function formatEditsForDocument(document) {
  const original = document.getText();
  const formatted = formatBasicNextSource(original);
  if (formatted === original) return [];
  const last = document.positionAt(original.length);
  return [vscode.TextEdit.replace(new vscode.Range(new vscode.Position(0, 0), last), formatted)];
}

function enableAutoUppercaseKeywords() {
  return cfg().get("autoUppercaseKeywords", true) !== false;
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
    const edit = reservedWordUppercaseEdit(document.getText(), change, {
      exclusions: cfg().get("autoUppercaseExclusions", []),
      allowPaste: cfg().get("autoUppercaseOnPaste", true) !== false,
    });
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

function scopeList(scope) {
  if (Array.isArray(scope)) return scope;
  if (typeof scope === "string") return [scope];
  return [];
}

function ruleOwnsBnScope(rule) {
  return scopeList(rule?.scope).some((s) => OWNED_BLOCK_SCOPES.has(s));
}

/**
 * Merge owned block-family TextMate rules into an existing tokenColorCustomizations
 * object. Replaces only owned `.bn` scopes; preserves all other rules.
 */
function mergeBlockColorRules(existingCustomizations, colors) {
  const base = existingCustomizations && typeof existingCustomizations === "object"
    ? { ...existingCustomizations }
    : {};
  const previous = Array.isArray(base.textMateRules) ? base.textMateRules : [];
  const preserved = previous.filter((rule) => !ruleOwnsBnScope(rule));
  const ownedRules = Object.keys(BLOCK_COLOR_SCOPES).map((key) => {
    const meta = BLOCK_COLOR_SCOPES[key];
    const foreground = normalizeHexColor(colors?.[key], BLOCK_COLOR_DEFAULTS[key]);
    const settings = { foreground };
    if (meta.fontStyle) settings.fontStyle = meta.fontStyle;
    return { scope: meta.scope, settings };
  });
  base.textMateRules = preserved.concat(ownedRules);
  return base;
}

function readBlockColorsFromConfig() {
  const colors = {};
  for (const key of Object.keys(BLOCK_COLOR_DEFAULTS)) {
    colors[key] = normalizeHexColor(
      cfg().get(`blockColors.${key}`, BLOCK_COLOR_DEFAULTS[key]),
      BLOCK_COLOR_DEFAULTS[key],
    );
  }
  return colors;
}

async function applyBlockColorCustomizations() {
  const editorCfg = vscode.workspace.getConfiguration("editor");
  const inspect = editorCfg.inspect("tokenColorCustomizations");
  const hasWorkspace = Boolean(vscode.workspace.workspaceFolders?.length);
  const target = hasWorkspace
    ? vscode.ConfigurationTarget.Workspace
    : vscode.ConfigurationTarget.Global;
  const current = (hasWorkspace
    ? inspect?.workspaceValue
    : inspect?.globalValue) || {};
  const merged = mergeBlockColorRules(current, readBlockColorsFromConfig());
  await editorCfg.update("tokenColorCustomizations", merged, target);
}

function startLanguageServer(context, collection) {
  if (
    typeof cp.spawn !== "function" ||
    !vscode.languages.registerDefinitionProvider ||
    !vscode.languages.registerReferenceProvider ||
    !vscode.languages.registerHoverProvider ||
    !vscode.languages.registerDocumentSymbolProvider
  ) return undefined;
  const executable = cfg().get("executable", "bn");
  const lspArgs = cfg().get("lspArgs", []) || [];
  const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const child = cp.spawn(executable, ["lsp", ...lspArgs], { cwd, stdio: ["pipe", "pipe", "pipe"] });
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

function runBnCheck(document, collection) {
  if (!document || document.languageId !== "basicnext" || document.isUntitled) return;
  const executable = cfg().get("executable", "bn");
  const checkArgs = cfg().get("checkArgs", []) || [];
  const minimum = cfg().get("diagnosticsMinimumSeverity", "warning");
  const cwd = vscode.workspace.getWorkspaceFolder?.(document.uri)?.uri.fsPath
    || vscode.workspace.workspaceFolders?.[0]?.uri.fsPath
    || path.dirname(document.fileName);
  const args = ["check", ...checkArgs, document.fileName];
  cp.execFile(executable, args, { cwd, timeout: 30000, maxBuffer: 2 * 1024 * 1024 }, (error, stdout, stderr) => {
    const output = `${stdout || ""}${stderr || ""}`;
    const parsed = parseDiagnostics(output);
    const filtered = filterDiagnosticsBySeverity(parsed, minimum);
    collection.set(document.uri, filtered);
    if (error && parsed.length === 0 && /ENOENT|not found|spawn/i.test(String(error))) {
      // executable missing — leave collection empty; status bar shows hint
    }
  });
}

function registerDiagnostics(context, collection) {
  const onSave = vscode.workspace.onDidSaveTextDocument((document) => {
    if (cfg().get("checkOnSave", true) === false) return;
    runBnCheck(document, collection);
  });
  let timer;
  const onType = vscode.workspace.onDidChangeTextDocument((event) => {
    if (cfg().get("checkOnType", false) !== true) return;
    const document = event.document;
    if (document.languageId !== "basicnext" || document.isUntitled) return;
    const ms = Number(cfg().get("checkDebounceMs", 500)) || 0;
    clearTimeout(timer);
    timer = setTimeout(() => runBnCheck(document, collection), ms);
  });
  context.subscriptions.push(onSave, onType, { dispose: () => clearTimeout(timer) });
}

function registerFormatter(context) {
  const provider = {
    provideDocumentFormattingEdits(document) {
      if (document.languageId !== "basicnext") return [];
      return formatEditsForDocument(document);
    },
  };
  context.subscriptions.push(
    vscode.languages.registerDocumentFormattingEditProvider("basicnext", provider),
  );
  if (typeof vscode.workspace.onWillSaveTextDocument === "function") {
    context.subscriptions.push(vscode.workspace.onWillSaveTextDocument((event) => {
      if (cfg().get("formatOnSave", false) !== true) return;
      const document = event.document;
      if (document.languageId !== "basicnext") return;
      const edits = formatEditsForDocument(document);
      if (edits.length) event.waitUntil(Promise.resolve(edits));
    }));
  }
}

function probeBnVersion(executable) {
  return new Promise((resolve) => {
    const tryArgs = [["--version"], ["-V"], ["version"]];
    const attempt = (index) => {
      if (index >= tryArgs.length) {
        resolve({ ok: false, version: undefined });
        return;
      }
      cp.execFile(executable, tryArgs[index], { timeout: 4000 }, (error, stdout, stderr) => {
        if (error) {
          attempt(index + 1);
          return;
        }
        const text = `${stdout || ""}${stderr || ""}`.trim().split(/\r?\n/)[0] || "";
        resolve({ ok: true, version: text });
      });
    };
    attempt(0);
  });
}

function registerStatusBar(context) {
  const item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  item.command = "basicnext.statusBarAction";
  item.name = "Basic Next";
  item.show();
  context.subscriptions.push(item);

  const refresh = async () => {
    const executable = cfg().get("executable", "bn");
    const result = await probeBnVersion(executable);
    if (result.ok) {
      const short = (result.version || "").replace(/^bn\s+/i, "").slice(0, 40);
      item.text = short ? `Basic Next $(check) ${short}` : "Basic Next";
      item.tooltip = `bn: ${result.version || executable}`;
      item.backgroundColor = undefined;
    } else {
      item.text = "Basic Next $(warning) bn missing";
      item.tooltip = `Could not run '${executable} --version'. Set basicnext.executable or install bn on PATH.`;
    }
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("basicnext.statusBarAction", async () => {
      const pick = await vscode.window.showQuickPick(
        [
          { label: "Check current file", action: "check" },
          { label: "Run", action: "run" },
          { label: "Build and Run", action: "build" },
          { label: "Refresh bn version", action: "refresh" },
        ],
        { title: "Basic Next" },
      );
      if (!pick) return;
      if (pick.action === "check") await vscode.commands.executeCommand("basicnext.check");
      else if (pick.action === "run") await vscode.commands.executeCommand("basicnext.run");
      else if (pick.action === "build") await vscode.commands.executeCommand("basicnext.buildAndRun");
      else if (pick.action === "refresh") await refresh();
    }),
  );

  refresh();
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("basicnext.executable")) refresh();
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => refresh()),
  );
  return { refresh, item };
}

function activate(context) {
  const collection = vscode.languages.createDiagnosticCollection("basicnext");
  const run = async () => {
    const document = activeDocument();
    if (!document) return;
    if (document.isDirty && !await document.save()) return;
    const executable = cfg().get("executable", "bn");
    const runArgs = cfg().get("runArgs", []) || [];
    const extra = shellQuoteArgs(runArgs);
    const cmd = extra
      ? `${shellQuote(executable)} run ${extra} ${shellQuote(document.fileName)}`
      : `${shellQuote(executable)} run ${shellQuote(document.fileName)}`;
    terminal().sendText(cmd);
  };
  const buildAndRun = async () => {
    const document = activeDocument();
    if (!document) return;
    if (document.isDirty && !await document.save()) return;
    const executable = cfg().get("executable", "bn");
    const buildArgs = cfg().get("buildArgs", []) || [];
    const extension = process.platform === "win32" ? ".exe" : "";
    const artifact = path.join(os.tmpdir(), `basicnext-${path.basename(document.fileName, ".bn")}${extension}`);
    const extra = shellQuoteArgs(buildArgs);
    const buildCmd = extra
      ? `${shellQuote(executable)} build ${extra} ${shellQuote(document.fileName)} -o ${shellQuote(artifact)}`
      : `${shellQuote(executable)} build ${shellQuote(document.fileName)} -o ${shellQuote(artifact)}`;
    terminal().sendText(`${buildCmd} && ${shellQuote(artifact)}`);
  };
  const check = async () => {
    const document = activeDocument();
    if (!document) return;
    if (document.isDirty && !await document.save()) return;
    runBnCheck(document, collection);
  };

  context.subscriptions.push(
    collection,
    vscode.commands.registerCommand("basicnext.run", run),
    vscode.commands.registerCommand("basicnext.buildAndRun", buildAndRun),
    vscode.commands.registerCommand("basicnext.check", check),
  );

  registerAutoUppercaseKeywords(context);
  registerFormatter(context);
  registerDiagnostics(context, collection);
  registerStatusBar(context);
  startLanguageServer(context, collection);

  applyBlockColorCustomizations().catch(() => {});
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("basicnext.blockColors")) {
        applyBlockColorCustomizations().catch(() => {});
      }
    }),
  );
}

function deactivate() {}

module.exports = {
  activate,
  deactivate,
  parseDiagnostics,
  shellQuote,
  startLanguageServer,
  lspCompletionItems,
  RESERVED_WORDS,
  inStringOrLineComment,
  reservedWordUppercaseEdit,
  formatBasicNextSource,
  filterDiagnosticsBySeverity,
  mergeBlockColorRules,
  BLOCK_COLOR_DEFAULTS,
  BLOCK_COLOR_SCOPES,
  OWNED_BLOCK_SCOPES,
};
