# Appendix E: BNJson

[← Previous: Appendices](10_appendices.md) · [Contents](toc.md)

`BNJson` is an external provider-backed module. Every consumer must import it
explicitly:

```basic
IMPORT BNJson AS Json
```

## DOM and AOT (0.6.1c)

After bucket 0.6.1c the module exposes a typed document Object Model (DOM) in
addition to `Parse` / `Stringify`. The same members run under **`bni`** and
**`bnc`** (native link through `bn_rt`). There is still no language keyword
`SERIALIZE` and no `ToJson` on domain classes.

Normative contract: [`language/0.6/bnjson.md`](../../../language/0.6/bnjson.md).

Bounds are unchanged from 0.3: nesting depth **64**, size **8 MiB**, no
trailing input, no duplicate keys, no non-finite numbers. Depth is checked at
the DOM write that would breach it.

Ownership for nested documents is **move**: `SetJson` / `AppendJson` /
`SetJsonAt` consume the child handle. `Clone` is the explicit duplicate.
Scalar accessors (`GetString`, `SetInteger`, …) allocate no handles.

## Companion codecs

Keep domain types free of JSON. Put `Encode` / `Decode` in a **sibling**
companion module named `<Type>Json` (locked **S-3**: `BirdJson`, not a
`Bird.Json` path):

```basic
IMPORT Bird.Bird AS Bird
IMPORT BirdJson AS BirdJson
IMPORT BNJson AS Json

FUNCTION Start() AS VOID
    LET eagle AS Bird = NEW Bird("eagle", 2)
    LET doc AS Json.Json OR Error = BirdJson.Encode(eagle)
    IF doc IS Error THEN
        RETURN
    END IF
    LET text AS STRING OR Error = Json.Json.Stringify(doc)
    LET again AS Bird OR Error = BirdJson.Decode(doc)
    IF again IS Error THEN
        RETURN
    END IF
END FUNCTION
```

Both companion entry points preserve failures as `Error` values under `bni`
and `bnc`; `Decode` never fabricates a placeholder domain object after a
missing or wrongly typed field.

`EncodeText` / `DecodeText` sugar is **optional and local** to a companion
(locked **S-4** — deferred from the 0.6.1c convention). The book teaches
`Encode` then `Stringify` so the text boundary stays visible.

Working example: `examples/serialization/` (`Bird.bn`, `BirdJson.bn`,
`main.bn`).

---

[Next: Appendix F: BNLog →](12_bnlog.md)
