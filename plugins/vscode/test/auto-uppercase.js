// Author: Carlos Quintella
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

const assert = require("assert");
const Module = require("module");
const path = require("path");

const extension = path.resolve(__dirname, "..");
const originalLoad = Module._load;
Module._load = (request, parent, isMain) => {
  if (request === "vscode") {
    return {
      DiagnosticSeverity: { Error: 0, Warning: 1, Information: 2, Hint: 3 },
      Range: class Range {},
      Position: class Position {},
      TextEdit: { replace: () => ({}) },
      Diagnostic: class Diagnostic {},
      languages: {
        createDiagnosticCollection: () => ({ set() {} }),
        registerDefinitionProvider: () => ({ dispose() {} }),
        registerReferenceProvider: () => ({ dispose() {} }),
        registerHoverProvider: () => ({ dispose() {} }),
        registerDocumentSymbolProvider: () => ({ dispose() {} }),
        registerCompletionItemProvider: () => ({ dispose() {} }),
        registerDocumentFormattingEditProvider: () => ({ dispose() {} }),
      },
      CompletionItem: class CompletionItem {},
      StatusBarAlignment: { Left: 1, Right: 2 },
      ConfigurationTarget: { Global: 1, Workspace: 2 },
      workspace: {
        getConfiguration: () => ({
          get: (_k, d) => d,
          inspect: () => ({}),
          update: async () => {},
        }),
        onDidOpenTextDocument: () => ({ dispose() {} }),
        onDidChangeTextDocument: () => ({ dispose() {} }),
        onDidCloseTextDocument: () => ({ dispose() {} }),
        onDidSaveTextDocument: () => ({ dispose() {} }),
        onWillSaveTextDocument: () => ({ dispose() {} }),
        onDidChangeConfiguration: () => ({ dispose() {} }),
        onDidChangeWorkspaceFolders: () => ({ dispose() {} }),
        textDocuments: [],
        workspaceFolders: [],
      },
      window: {
        activeTextEditor: undefined,
        visibleTextEditors: [],
        createTerminal: () => ({ show() {}, sendText() {} }),
        createStatusBarItem: () => ({ show() {}, dispose() {}, text: "", tooltip: "", command: "" }),
        showQuickPick: async () => undefined,
      },
      commands: { registerCommand: () => ({ dispose() {} }), executeCommand: async () => {} },
      Uri: { file: (p) => ({ toString: () => p }), parse: (u) => ({ toString: () => u }) },
    };
  }
  if (request === "child_process") {
    return {
      spawn: () => ({ stdin: { write() {} }, stdout: { on() {} }, on() {}, kill() {} }),
      execFile: (_a, _b, _c, cb) => cb && cb(new Error("ENOENT")),
    };
  }
  return originalLoad(request, parent, isMain);
};

const {
  RESERVED_WORDS,
  inStringOrLineComment,
  reservedWordUppercaseEdit,
  formatBasicNextSource,
  mergeBlockColorRules,
  BLOCK_COLOR_DEFAULTS,
  filterDiagnosticsBySeverity,
} = require(path.join(extension, "extension.js"));

assert.ok(RESERVED_WORDS.has("FUNCTION"));
assert.ok(RESERVED_WORDS.has("WHILE"));
assert.ok(!RESERVED_WORDS.has("function"));

assert.strictEqual(inStringOrLineComment('PRINT "function"', 8), true);
assert.strictEqual(inStringOrLineComment("// function", 3), true);
assert.strictEqual(inStringOrLineComment("LET x = function", 8), false);

{
  const doc = "function ";
  const edit = reservedWordUppercaseEdit(doc, { text: " ", rangeOffset: 8 });
  assert.ok(edit, "expected uppercase edit after space");
  assert.strictEqual(edit.upper, "FUNCTION");
  assert.strictEqual(doc.slice(edit.start, edit.end), "function");
}

{
  const doc = 'PRINT "function "';
  const edit = reservedWordUppercaseEdit(doc, { text: " ", rangeOffset: 15 });
  assert.strictEqual(edit, null, "must not uppercase inside string");
}

{
  const doc = "WHILE";
  const edit = reservedWordUppercaseEdit(doc, { text: "WHILE", rangeOffset: 0 });
  assert.strictEqual(edit, null, "already uppercase");
}

{
  const doc = "while";
  const edit = reservedWordUppercaseEdit(doc, { text: "while", rangeOffset: 0 });
  assert.ok(edit);
  assert.strictEqual(edit.upper, "WHILE");
}

{
  const doc = "step ";
  const edit = reservedWordUppercaseEdit(doc, { text: " ", rangeOffset: 4 }, {
    exclusions: ["STEP"],
  });
  assert.strictEqual(edit, null, "excluded STEP must not uppercase");
}

{
  const doc = "while";
  const edit = reservedWordUppercaseEdit(doc, { text: "while", rangeOffset: 0 }, {
    allowPaste: false,
  });
  assert.strictEqual(edit, null, "paste/completion disabled");
}

{
  const doc = "while ";
  const edit = reservedWordUppercaseEdit(doc, { text: " ", rangeOffset: 5 }, {
    allowPaste: false,
  });
  assert.ok(edit, "boundary typing still works when paste disabled");
  assert.strictEqual(edit.upper, "WHILE");
}

{
  const src = "function Foo()\n  end   function\n";
  const out = formatBasicNextSource(src);
  assert.strictEqual(out, "FUNCTION Foo()\n  END FUNCTION\n");
}

{
  const src = 'PRINT "function" // while\nwhile true\nend while\n';
  const out = formatBasicNextSource(src);
  assert.match(out, /PRINT "function"/);
  assert.match(out, /\/\/ while/);
  assert.match(out, /WHILE TRUE/);
  assert.match(out, /END WHILE/);
}

{
  const merged = mergeBlockColorRules(
    {
      textMateRules: [
        { scope: "comment", settings: { foreground: "#888888" } },
        { scope: "storage.type.function.bn", settings: { foreground: "#000000" } },
      ],
    },
    { function: "#AABBCC" },
  );
  const scopes = merged.textMateRules.map((r) => r.scope);
  assert.ok(scopes.includes("comment"));
  assert.ok(scopes.includes("storage.type.function.bn"));
  assert.strictEqual(
    merged.textMateRules.filter((r) => r.scope === "storage.type.function.bn").length,
    1,
  );
  assert.strictEqual(
    merged.textMateRules.find((r) => r.scope === "storage.type.function.bn").settings.foreground,
    "#AABBCC",
  );
  assert.strictEqual(BLOCK_COLOR_DEFAULTS.function, "#C586C0");
}

{
  const vscode = require("vscode");
  const diags = [
    { severity: vscode.DiagnosticSeverity.Error },
    { severity: vscode.DiagnosticSeverity.Warning },
    { severity: vscode.DiagnosticSeverity.Hint },
  ];
  assert.strictEqual(filterDiagnosticsBySeverity(diags, "error").length, 1);
  assert.strictEqual(filterDiagnosticsBySeverity(diags, "warning").length, 2);
  assert.strictEqual(filterDiagnosticsBySeverity(diags, "hint").length, 3);
}

console.log("Basic Next auto-uppercase checks passed");
