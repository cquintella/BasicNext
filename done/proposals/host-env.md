# Proposal: `HOST.Env` — read the process environment

## Status

**Accepted for planning 2026-10-06 (Carlos): needed.** Planned in
[`ongoing/bucket-0.6.5.md`](../../ongoing/bucket-0.6.5.md) (section B).
Language change: it adds one importable host capability name.

## Motivation

A program, and a standard module acting for it, sometimes needs a value from
the process environment: a configuration value, or the library search path
that `BNFfi` uses ([`bnffi.md`](bnffi.md), bucket
[`0.6.7`](../bucket-0.6.7.md)). Today nothing in the HOST catalog reads the
environment; each runtime reads its own `BN_*` variables outside any
capability or policy.

`BNFfi` reaches the environment through `HOST.Env`, as `BNWeb` reaches the
network through `HOST.Net`: the same capability, provider and policy decide
what it may read.

## Surface

```basic
IMPORT HOST.Env AS Env

FUNCTION Start() AS INTEGER
    LET home AS STRING OR Error = Env.Get("HOME")
    IF home IS Error THEN
        PRINT home.Message
        RETURN 1
    END IF
    PRINT home

    LET known AS BOOLEAN OR Error = Env.Has("BN_LIBRARY_PATH")
    RETURN 0
END FUNCTION
```

| Member | Result |
| --- | --- |
| `Env.Get(name AS STRING)` | `STRING OR Error`: the value; `Error` when the variable is not set |
| `Env.Has(name AS STRING)` | `BOOLEAN OR Error` |

- **Read-only.** No `Set` or `Remove`: changing the environment of a running
  process is not thread-safe (Rust 2024 makes `std::env::set_var` `unsafe`
  for that reason), and a child's environment belongs to `HOST.Exec`.
- **No enumeration** in the first cut: listing every variable exposes values
  the program did not ask for.
- An empty value is a set variable whose value is `""`; it is not "not set".

## Errors

Each failure is an `Error` with a stable code (numbers decided 2026-10-06,
bucket 0.6.5a D4; `error.md`):

| Condition | Code |
| --- | --- |
| The variable is not set (`Get` only) | `Env.NOT_SET` = 1 |
| `name` is empty or contains `=` or NUL | `Env.INVALID_NAME` = 2 |
| The value is not valid UTF-8 | `Env.INVALID_UTF8` = 3 |
| Execution policy denies the read | `Env.POLICY_DENIED` = 4 |

A host without the capability fails at the call with `HOST_CAPABILITY_UNAVAILABLE`, distinct from policy denial.

## Policy and targets

- `BN_ENV_POLICY=deny` denies every read, honoured identically by `bni run` and
  a compiled program, under the existing policy-input rules: an empty value is
  absent; a malformed value is `CONFIG_INVALID` and the process does not start.
  The compiled policy reaches the binary and `bn_rt` re-checks at the call.
- **Allowed by default** (Carlos, 2026-10-06), as `HOST.Exec`. Restricted
  profiles (the notebook kernel) deny by default.
- Native `bni` and native `bnc`. `wasm32`: as `HOST.Exec`, the module compiles and imports the runtime calls; a host without the provider fails at the call with `HOST_CAPABILITY_UNAVAILABLE` (`0.6.md`, capability availability).

## Language changes (spec first, in the bucket)

| File | Change |
| --- | --- |
| `language/0.6/0.6.ebnf` | `"Env"` added to `importable-host-capability-name` (not to `host-capability-name`: it is reached only through `IMPORT`, like `Exec`) |
| `language/0.6/0.6.md` | § Host capabilities lists `Env`; § `HOST.Env` with the surface, errors and policy |
| `language/0.6/keywords.md` | No reserved word: `Env`, `Get`, `Has` are identifiers on a host type |
| `language/0.6/host-env.md` | The capability contract (as `host-exec.md`) |
| Fixtures | Positive: import, `Get`, `Has`, empty value. Negative: `HOST.Env` used without `IMPORT`; each `Error` code; policy denied; `wasm32` without a provider (`HOST_CAPABILITY_UNAVAILABLE`) |

## Use by BNFfi

`modules/bn/BNFfi.bn` imports `HOST.Env`. Library resolution reads the search
path variable `BN_LIBRARY_PATH` (platform `PATH` separator) through the `HOST.Env` provider, so `BN_ENV_POLICY=deny` also
removes that search path; `C.Call` then searches `--ffi-path` and the system
loader only. One provider serves programs and `BNFfi`; there is no second
environment reader in `bn_rt`.
