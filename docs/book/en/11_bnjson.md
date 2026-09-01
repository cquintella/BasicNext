# Appendix E: BNJson

`BNJson` is an external provider-backed module. It is not part of the Basic
Next core and every consumer must import it explicitly:

```basic
IMPORT BNJson AS Json
```

The accepted 0.3 contract, ownership rules, limits, and errors are defined in
[`docs/language/0.3/bnjson.md`](../../language/0.3/bnjson.md). Providers may
use Rust implementations, but must preserve the documented behavior.
