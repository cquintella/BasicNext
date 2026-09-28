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

## Parse and Stringify (unchanged from 0.3)

```basic
IMPORT BNJson AS Json
LET value AS Json.Json OR Error = Json.Json.Parse("{\"ok\":true}")
```

The Rust provider supplies parsing and serialization through the bounded
`Json.Json` type:

- `Parse(text)` returns `Error` (`PARSE_FAILED`) for malformed JSON, invalid
  UTF-8, trailing input, control characters, invalid surrogate pairs, nesting
  deeper than 64 levels, and input larger than 8 MiB;
- `Stringify(value)` emits valid JSON and returns `Error` (`LIMIT`) for output
  larger than 8 MiB;
- all operations are synchronous and do not access the filesystem, network, or
  any implicit host capability.

`BNJson` has no dependency on `BNWeb`, `BNLog`, or `HOST.Net`. Applications
must import it explicitly when they use JSON functionality.

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

## Errors (0.6.2)

`Parse`, `Stringify`, and the DOM members return `Error` rather than stopping
the program. `Code` of a BNJson `Error` ([error.md](error.md)) is one of these
`INTEGER` constants of the module (`Json.NOT_FOUND` under
`IMPORT BNJson AS Json`):

| Constant | Value | When |
| --- | ---: | --- |
| `INVALID_ARGUMENT` | 1 | A non-finite `FLOAT` written into a document; a document moved into itself |
| `NOT_FOUND` | 2 | A missing key |
| `TYPE_MISMATCH` | 3 | The value at the key or index is not the requested type; `Length` of a scalar; a write into a value that is not an object (key writes) or an array (index writes, appends) |
| `OUT_OF_RANGE` | 4 | An array index outside the array |
| `LIMIT` | 5 | A write past the depth limit; `Stringify` output past 8 MiB |
| `UNAVAILABLE` | 6 | The operation is not provided |
| `PARSE_FAILED` | 7 | `Parse` of text that is not valid JSON under the bounds above; `Cause` names the position |

A released `Json` handle is not an `Error`: using it is `USE_AFTER_RELEASE`.

## Companion codecs (option E)

Domain types stay free of JSON. A sibling module `<Type>Json` owns `Encode` /
`Decode` (S-3: **`BirdJson`**, not `Bird.Json`). Optional `EncodeText` /
`DecodeText` sugar is **deferred** (S-4); companions may add it locally.

See `examples/serialization/` (`Bird.bn`, `BirdJson.bn`, `main.bn`) and the
book appendix on BNJson.
