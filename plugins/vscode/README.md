# Basic Next for VS Code

Basic Next language support for Visual Studio Code.

## Install

From the repository root, package the extension and install the generated
VSIX file:

```sh
cd plugins/vscode
npx --yes @vscode/vsce package --allow-missing-repository
code --install-extension basicnext-0.4.3.vsix
```

Restart VS Code completely after installing or updating the extension. The
debugger contribution is loaded when the VS Code application starts.

## Configure

The extension runs `bn` from `PATH` by default. If necessary, set
`basicnext.executable` in VS Code settings to the full path of the `bn`
executable.

## Use

- Open a `.bn` file. VS Code selects the `Basic Next` language mode and
  applies syntax highlighting.
- The extension starts `bn lsp` for open Basic Next workspaces and forwards
  full-document changes. Diagnostics, definition lookup, and completion are
  provided by the Rust frontend; set `basicnext.executable` if the binary is
  not on `PATH`. Completion covers reserved words, `HOST` capabilities and
  members (`HOST.Console.Cls`, import aliases such as `CON.NumCols`), local
  `FUNCTION`/`CLASS`/`LET` names, built-in namespaces (`Date.Parse`), and
  exported names of imported standard modules (`Math.ABS`). Type `.` or
  `Ctrl+Space` (`Cmd+Space` on macOS) to trigger it.
- Save the file to run `bn check`; source-spanned errors appear in the
  Problems panel.
- Use **Basic Next: Run** from the Command Palette, the editor run menu, or
  `Cmd+F5` (`Ctrl+F5` on Windows/Linux) to run `bn run` in an integrated
  terminal.
- Use **Basic Next: Build and Run** or `Cmd+Shift+F5` (`Ctrl+Shift+F5`) to
  build a temporary native artifact and execute it. It works for the current
  supported `bn build` subset.
- The Run and Debug view exposes **Run Basic Next** through the native `bn dap`
  service. The adapter forwards DAP over bounded local stdio; it does not open
  a terminal or execute `bn run` for a debug session.
- Breakpoints, pause, continue, stack/scopes/variables, and stepping are
  debugger operations. Stepping follows interpreter IR instructions carrying
  Basic Next source spans: multiple instructions may map to one source line,
  and loops may revisit a line. The debugger is not a REPL and does not
  evaluate arbitrary expressions.

The bundled TextMate grammar is synchronized with
`docs/library/basicnext.tmLanguage.json`.
