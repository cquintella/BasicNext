# Appendix I: External module conventions

Every `BN*` facility is an external module backed by a host/provider
interface. External modules:

- are never implicitly available in the language core;
- require an explicit `IMPORT` and alias;
- must expose bounded, deterministic errors when a provider is unavailable;
- must keep their normative API and fixtures separate from core grammar;
- may be implemented by Rust providers without changing BN syntax.

`HOST` is the sole built-in interface object in the language specification. The
planned `BNThreads`, `BNCrypto`, and other future modules follow these same
rules.

## Module search path (0.5.1)

`IMPORT` resolves a module by walking an **ordered list of directories; the
first hit wins**. The effective list is built as `defaults → config → CLI`:

- **Defaults:** the directory of the entry `.bn` file, plus the discovered
  standard-library `modules/bn`.
- **Config:** a `module-path = ["dir", ...]` array in the selected `config.toml`.
- **CLI:** repeatable `--module-path <dir>` flags (added last, highest priority).

The same resolution applies to `bn check`, `bn run`, `bn build`, and `bn eval`.

The standard library ships as `.bn` source. When `bn` is installed, it finds the
stdlib by walking upward from its own executable, accepting (first match wins):

- `<prefix>/share/bn/modules/bn` — the FHS install location used by the install
  scripts (arch-independent, alongside the diagnostics catalog);
- `<prefix>/lib/bn/modules/bn`;
- a portable `modules/bn` beside the executable or the working directory.

So a system install places the modules at, for example,
`/usr/local/share/bn/modules/bn`, and no configuration is required afterwards.
