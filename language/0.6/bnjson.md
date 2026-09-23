# BNJson Standard Library (0.6.1c)

`BNJson` is an explicitly imported external module. It is not a language
keyword, a `HOST` capability, or part of the core interpreter.

```basic
IMPORT BNJson AS Json
```

After bucket **0.6.1c**, the same surface runs under **`bni` (interpret)** and
**`bnc` (AOT via `bn_rt` + `bn_llvm`)**. There is no `SERIALIZE` keyword and no
`ToJson` method on domain classes — encode/decode live in companion modules
(see below).

The 0.3 text under [`language/0.3/bnjson.md`](../0.3/bnjson.md) remains the
historical Parse/Stringify contract; this file is the normative DOM + AOT
successor for the 0.6 line.

## Bounds (unchanged from 0.3)

| Bound | Rule |
| --- | --- |
| Nesting depth | **64** — enforced at `Parse` and at every DOM write that would breach it |
| Size | **8 MiB** input (`Parse`) and output (`Stringify`) |
| Trailing input | rejected |
| Duplicate object keys | rejected |
| Non-finite numbers | rejected (`SetFloat` / `AppendFloat` / parse) |
| Invalid surrogates / control characters | rejected on parse |

A `Set*` / `Append*` that would push a document past depth 64 returns `Error`
**at that call**, not later at `Stringify`.

## DOM members

Accessors are typed so the result type is visible at the call site. There is
**no** generic `Get` / `Set`.

| Member | Signature | Notes |
| --- | --- | --- |
| `Object()` / `Array()` | `AS Json` | new owning handle |
| `Kind(doc)` | `AS STRING` | `object` / `array` / `string` / `number` / `boolean` / `null` |
| `Has(doc, key)` | `AS BOOLEAN` | present-and-null ≠ absent |
| `Length(doc)` | `AS INTEGER OR Error` | scalars have no length → `Error` |
| `Clone(doc)` | `AS Json OR Error` | explicit duplicate |
| `Parse` / `Stringify` | unchanged | `Parse` allocates |
| `SetString` / `SetInteger` / `SetFloat` / `SetBoolean` / `SetNull` | `AS VOID OR Error` | object writes |
| `GetString` / `GetInteger` / `GetFloat` / `GetBoolean` | scalar `OR Error` | missing / wrong kind → `Error` |
| `SetJson(doc, key, child)` | `AS VOID OR Error` | **moves** `child` |
| `GetJson(doc, key)` | `AS Json OR Error` | fresh handle; caller `RELEASE` |
| `Append*` / `Get*At` / `Set*At` | array twins | OOB → `Error`; `AppendJson` / `SetJsonAt` **move** |

### Move semantics

`SetJson` / `SetJsonAt` / `AppendJson` **consume** the child handle. Using it
afterwards is `USE_AFTER_RELEASE` under `bni` (a diagnostic, not an `Error`
value). Duplication is only via `Clone`. `GetJson` / `GetJsonAt` do **not**
consume the parent.

## Companion codecs (option E)

Domain types stay free of JSON. A sibling module `<Type>Json` owns `Encode` /
`Decode` (S-3: **`BirdJson`**, not `Bird.Json`). Optional `EncodeText` /
`DecodeText` sugar is **deferred** (S-4); companions may add it locally.

See `examples/serialization/` (`Bird.bn`, `BirdJson.bn`, `main.bn`) and the
book appendix on BNJson.
