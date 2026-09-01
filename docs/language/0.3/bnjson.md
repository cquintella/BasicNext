# BNJson Standard Library 0.3

`BNJson` is an explicitly imported external module. It is not a language
keyword, a `HOST` capability, or part of the core interpreter.

```basic
IMPORT BNJson AS Json
LET value AS Json.Json OR Error = Json.Json.Parse("{\"ok\":true}")
```

The Rust provider supplies parsing and serialization through the bounded
`Json.Json` type:

- `Parse(text)` rejects malformed JSON, invalid UTF-8, trailing input, control
  characters, invalid surrogate pairs, nesting deeper than 64 levels, and input
  larger than 8 MiB;
- `Stringify(value)` emits valid JSON and rejects output larger than 8 MiB;
- all operations are synchronous and do not access the filesystem, network, or
  any implicit host capability.

`BNJson` has no dependency on `BNWeb`, `BNLog`, or `HOST.Net`. Applications
must import it explicitly when they use JSON functionality.
