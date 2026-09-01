# Appendix G: BNWeb

`BNWeb` is an external web provider module. It is separate from the core and
requires an explicit import:

```basic
IMPORT BNWeb AS Web
```

The accepted 0.3 server, routing, TLS, ACL, and bounded transport contract is
specified in [`docs/language/0.3/bnweb.md`](../../language/0.3/bnweb.md).
Transport-to-BN threading, HTTP access-log delivery, and HTTPS client support
are gated for Sprint 13 through the external `BNDispatch` module.
