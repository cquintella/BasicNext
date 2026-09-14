# Basic Next for VS Code

Basic Next **0.5.0** language support for Visual Studio Code (extension **0.5.2**).

Aligned with `docs/0.5.0/`: `RELEASE` / `WEAK` / `ASYNC` / `AWAIT` highlighted;
`DELETE` marked deprecated/illegal (purged from language DNA).

## Install

From the repository root, package the extension and install the generated
VSIX file:

```sh
cd plugins/vscode
npx --yes @vscode/vsce package --allow-missing-repository
code --install-extension basicnext-0.5.2.vsix
```

Restart VS Code completely after installing or updating the extension. The
debugger contribution is loaded when the VS Code application starts.

## Configure

| Setting | Default | Description |
| --- | --- | --- |
| `basicnext.executable` | `bn` | Path to the `bn` binary |
| `basicnext.autoUppercaseKeywords` | `true` | Uppercase reserved words as you type |
| `basicnext.autoUppercaseOnPaste` | `true` | Also uppercase whole-token inserts (paste/completion) |
| `basicnext.autoUppercaseExclusions` | `[]` | Uppercase spellings to skip (e.g. `["STEP"]`) |
| `basicnext.formatOnSave` | `false` | Uppercase reserved words + normalize `END   FUNCTION` → `END FUNCTION` on save |
| `basicnext.runArgs` | `[]` | Extra args after `bn run` |
| `basicnext.buildArgs` | `[]` | Extra args after `bn build` |
| `basicnext.checkArgs` | `[]` | Extra args after `bn check` |
| `basicnext.lspArgs` | `[]` | Extra args after `bn lsp` |
| `basicnext.checkOnSave` | `true` | Run `bn check` on save |
| `basicnext.checkOnType` | `false` | Debounced `bn check` while typing |
| `basicnext.checkDebounceMs` | `500` | Debounce for check-on-type |
| `basicnext.diagnosticsMinimumSeverity` | `warning` | `error` / `warning` / `hint` |
| `basicnext.snippets.enabled` | `true` | Documented; snippets are always contributed — disable via VS Code Snippets UI |
| `basicnext.blockColors.function` | `#C586C0` | FUNCTION / END FUNCTION |
| `basicnext.blockColors.while` | `#D7BA7D` | WHILE / END WHILE |
| `basicnext.blockColors.for` | `#CE9178` | FOR / END FOR |
| `basicnext.blockColors.repeat` | `#DCDCAA` | REPEAT / UNTIL |
| `basicnext.blockColors.conditional` | `#569CD6` | IF / ELSE / END IF |
| `basicnext.blockColors.class` | `#4EC9B0` | CLASS / END CLASS |
| `basicnext.blockColors.flow` | `#F44747` | RETURN / EXIT / CONTINUE / STOP |
| `basicnext.blockColors.await` | `#B267E6` | AWAIT / ASYNC |

Block family colors are applied on activate and when `basicnext.blockColors.*`
changes by merging TextMate rules into workspace/user
`editor.tokenColorCustomizations` (only owned `.bn` scopes are replaced).
`END` shares the same TextMate scope as its block keyword in the grammar.

Default editor settings for `[basicnext]`: tab size 4, insert spaces, detect
indentation.

## Theme

**Basic Next Grid** — dark cyan grid theme. Select it from Color Theme
(`Cmd+K Cmd+T` / `Ctrl+K Ctrl+T`).

## Snippets

Contributed for `FUNCTION`, `WHILE`, `IF`, `FOR`, `REPEAT`, `CLASS`, and
common `IMPORT HOST.*` forms. To disable, use the VS Code Snippets UI
(or leave `basicnext.snippets.enabled` as documentation for preference).

## Use

- Open a `.bn` file. VS Code selects the `Basic Next` language mode and
  applies syntax highlighting, folding markers, and indentation rules.
- Status bar shows **Basic Next** plus the `bn` version (or a missing-binary
  hint). Click for Check / Run / Build and Run.
- The extension starts `bn lsp` (plus `basicnext.lspArgs`) for open Basic Next
  workspaces and forwards full-document changes. Diagnostics, definition
  lookup, and completion are provided by the Rust frontend; set
  `basicnext.executable` if the binary is not on `PATH`.
- Save the file to run `bn check` when `basicnext.checkOnSave` is true;
  source-spanned errors appear in the Problems panel (filtered by
  `basicnext.diagnosticsMinimumSeverity`).
- Format Document / format-on-save (when enabled) uppercases reserved words
  outside strings/comments and normalizes `END` block spacing.
- Use **Basic Next: Run** from the Command Palette, the editor run menu, or
  `Cmd+F5` (`Ctrl+F5` on Windows/Linux) to run `bn run` (plus `runArgs`) in an
  integrated terminal.
- Use **Basic Next: Build and Run** or `Cmd+Shift+F5` (`Ctrl+Shift+F5`) to
  build a temporary native artifact and execute it.
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

Block keywords use distinct TextMate scopes and default colors per block kind.
`END` shares the **same** color as its block keyword (`END FUNCTION` with
`FUNCTION`, `END WHILE` with `WHILE`, `END IF` with `IF`, and likewise for
`FOR` / `CLASS` / `STRUCT` / `INTERFACE` / constructor / destructor).
