# Basic Next for VS Code

Basic Next language support for Visual Studio Code.

## Install

From the repository root, package the extension and install the generated
VSIX file:

```sh
cd plugins/vscode
npx --yes @vscode/vsce package --allow-missing-repository
code --install-extension basicnext-0.2.0.vsix
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
- Save the file to run `bn check`; source-spanned errors appear in the
  Problems panel.
- Use **Basic Next: Run** from the Command Palette, the editor run menu, or
  `Cmd+F5` (`Ctrl+F5` on Windows/Linux) to run `bn run` in an integrated
  terminal.
- Use **Basic Next: Build and Run** or `Cmd+Shift+F5` (`Ctrl+Shift+F5`) to
  build a temporary native artifact and execute it. It works for the current
  supported `bn build` subset.
- The Run and Debug view exposes **Run Basic Next** as a launch-only adapter.
  It opens an integrated terminal and executes `bn run`; VS Code does not
  receive the child-process lifetime from that terminal. End the launch
  session with Stop/Disconnect after the program exits.

Breakpoints, process tracking, and step debugging are not implemented yet.

The bundled TextMate grammar is synchronized with
`docs/library/basicnext.tmLanguage.json`.
