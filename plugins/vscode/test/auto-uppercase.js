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
      DiagnosticSeverity: { Error: 0, Warning: 1 },
      Range: class Range {},
      Diagnostic: class Diagnostic {},
      languages: {
        createDiagnosticCollection: () => ({ set() {} }),
        registerDefinitionProvider: () => ({ dispose() {} }),
        registerReferenceProvider: () => ({ dispose() {} }),
        registerHoverProvider: () => ({ dispose() {} }),
        registerDocumentSymbolProvider: () => ({ dispose() {} }),
        registerCompletionItemProvider: () => ({ dispose() {} }),
      },
      CompletionItem: class CompletionItem {},
      workspace: {
        getConfiguration: () => ({ get: (_k, d) => d }),
        onDidOpenTextDocument: () => ({ dispose() {} }),
        onDidChangeTextDocument: () => ({ dispose() {} }),
        onDidCloseTextDocument: () => ({ dispose() {} }),
        textDocuments: [],
      },
      window: { activeTextEditor: undefined, visibleTextEditors: [], createTerminal: () => ({ show() {}, sendText() {} }) },
      commands: { registerCommand: () => ({ dispose() {} }) },
      Uri: { file: (p) => ({ toString: () => p }), parse: (u) => ({ toString: () => u }) },
    };
  }
  if (request === "child_process") return { spawn: () => ({ stdin: { write() {} }, stdout: { on() {} }, on() {}, kill() {} }) };
  return originalLoad(request, parent, isMain);
};

const {
  RESERVED_WORDS,
  inStringOrLineComment,
  reservedWordUppercaseEdit,
} = require(path.join(extension, "extension.js"));

assert.ok(RESERVED_WORDS.has("FUNCTION"));
assert.ok(RESERVED_WORDS.has("WHILE"));
assert.ok(!RESERVED_WORDS.has("function"));

assert.strictEqual(inStringOrLineComment('PRINT "function"', 8), true);
assert.strictEqual(inStringOrLineComment("// function", 3), true);
assert.strictEqual(inStringOrLineComment("LET x = function", 8), false);

// After typing space following "function" — document already includes the space.
// change.rangeOffset is where the space was inserted; word ends at that offset in pre-insert…
// Our helper uses documentText AFTER change and rangeOffset as insert start.
// So document = "function ", change = { text: " ", rangeOffset: 8 }
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

console.log("Basic Next auto-uppercase checks passed");
