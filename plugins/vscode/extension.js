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

function activate(context) {
  const collection = vscode.languages.createDiagnosticCollection("basicnext");
  const lint = (document) => {
    if (document.languageId !== "basicnext" || document.isUntitled) return;
    const version = document.version;
    const executable = vscode.workspace.getConfiguration("basicnext").get("executable", "bn");
    cp.execFile(executable, ["check", document.fileName], { cwd: vscode.workspace.getWorkspaceFolder(document.uri)?.uri.fsPath }, (error, stdout, stderr) => {
      if (document.version !== version) return;
      if (error?.code === "ENOENT") {
        collection.set(document.uri, [new vscode.Diagnostic(
          new vscode.Range(0, 0, 0, 1),
          `cannot execute Basic Next checker '${executable}'`,
          vscode.DiagnosticSeverity.Error,
        )]);
        return;
      }
      collection.set(document.uri, parseDiagnostics(stderr || ""));
    });
  };
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
    vscode.workspace.onDidSaveTextDocument(lint),
    vscode.commands.registerCommand("basicnext.run", run),
    vscode.commands.registerCommand("basicnext.buildAndRun", buildAndRun),
  );
  if (vscode.window.activeTextEditor) lint(vscode.window.activeTextEditor.document);
}

function deactivate() {}

module.exports = { activate, deactivate, parseDiagnostics, shellQuote };
