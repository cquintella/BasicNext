# Basic Next 0.6 language tree

**Authority for the 0.6.x line**, in order. Opened 2026-09-21. Toolchain surface: `bni` (interpreter) and `bnc` (native compiler) replacing `bn`. S1' introduces `++` / `--` as prefix and postfix expressions (§ [0.6.0 language additions](0.6.md#060-language-additions)).

## Core language authority

| File | Role |
| --- | --- |
| [`0.6.ebnf`](0.6.ebnf) | Normative grammar (ISO 14977-style EBNF) |
| [`0.6.md`](0.6.md) | Normative semantics — amended language specification incorporated inline |
| [`keywords.md`](keywords.md) | Reserved-word, special literal, and operator registry |

## Topical normative contracts and companion documents

| File | Scope |
| --- | --- |
| [`arc-conformance.md`](arc-conformance.md) | ARC / lifetime conformance checklist and test fixtures |
| [`bncrypto.md`](bncrypto.md) | Standard library module: `BNCrypto` cryptography contract |
| [`bndata.md`](bndata.md) | Standard library module: `BNData` DataFrames and tabular data |
| [`bndispatch.md`](bndispatch.md) | Standard library module: `BNDispatch` asynchronous synchronization |
| [`bnjson.md`](bnjson.md) | Standard library module: `BNJson` JSON DOM contract |
| [`bnlog.md`](bnlog.md) | Standard library module: `BNLog` structured logging contract |
| [`bnsqlite.md`](bnsqlite.md) | Standard library module: `BNSqlite` embedded database contract |
| [`bnstring.md`](bnstring.md) | Standard library module: `BNString` extended string operations |
| [`bnweb.md`](bnweb.md) | Standard library module: `BNWeb` HTTP and URL contract |
| [`console.md`](console.md) | Host capability: `HOST.Console` text I/O contract |
| [`error.md`](error.md) | Language error model, diagnostic codes, and fatal error contract |
| [`host-env.md`](host-env.md) | Host capability: `HOST.Env` environment capability contract |
| [`host-exec.md`](host-exec.md) | Host capability: `HOST.Exec` process execution contract |
| [`host-net.md`](host-net.md) | Host capability: `HOST.Net` socket capability contract |
| [`host.md`](host.md) | Host capability catalog, binding semantics, and provider rules |
| [`ide-tooling.md`](ide-tooling.md) | LSP and DAP editor tooling protocol contracts |
| [`math.md`](math.md) | Standard library module: `BNMath` functions and numeric limits |
| [`memory-migration.md`](memory-migration.md) | Memory model migration guide (legacy scopes to ARC) |
| [`temporal.md`](temporal.md) | Scalar types `DATE`, `TIME`, and `TIMESTAMP` operations |

For 0.6.x language changes, edit this tree.
