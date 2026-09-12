# Basic Next Language Specification 0.5.0

## Status

**0.5.0 draft** (Quorra-gate). This document defines the 0.5.0 amendments to
the accepted 0.4 baseline. It **incorporates
[`docs/language/0.4/0.4.md`](../language/0.4/0.4.md) by reference** except
where the amendments below override it.

**Authority** for Basic Next 0.5.0 (this tree):

1. [`0.5.0.ebnf`](0.5.0.ebnf) — normative syntax
2. This file — normative static semantics, runtime behaviour, and diagnostics intent
3. [`keywords.md`](keywords.md) — reserved-word registry

Plan locks that this draft implements as language DNA are recorded in
[`todo/proposals/bucket-0.5.0-corrective.md`](../../todo/proposals/bucket-0.5.0-corrective.md).
Nothing outside this tree plus the 0.4 material it incorporates is part of
Basic Next 0.5.0 until accepted.

This is a **docs lock** for Quorra review. Parser / frontend / runtime
implementation of ARC and typed await may follow in later waves; the grammar
and semantics here are the contract those waves must honour.

## Amendment overview

| Topic | 0.5.0 lock |
| --- | --- |
| **ARC** | Class instances use automatic strong retain/release on assign, parameter, return, and end of scope. Zero strong → destructor chain, then free. Weak for cycles (spelling TBD). Unowned deferred. No force-dispose. Interpret = reference. |
| **`DELETE` removed** | Keyword purged from grammar, reserved-word list, and teaching surface. Not “deprecate on classes only.” |
| **`RELEASE`** | Optional **advanced** statement: drop **one** strong binding only; deinit only when the strong count reaches zero; never kill-all-aliases. Hello programs need not use it. |
| **Typed `AWAIT`** | Primary surface: when the ticket comes from `FUNCTION … AS T OR Error`, `AWAIT ticket(ms)` yields `T OR Error`. `Ticket.Result()` is not primary. ABI remains `bn_rt_dispatch_await`. Replay stored success until `Close`. |
| **HOST Close** | Capability methods stay `Close` / `*_close` (and ticket close). Never reintroduce `DELETE` as sugar over those closes. |

## Memory model (ARC)

### Scope

These rules apply to **class instances** (reference types). Structs, scalars,
and fixed vectors keep **value / copy** semantics and are not ARC.

### Strong references (default, automatic)

Locals, fields, parameters, and returns of class types are **strong** unless
annotated weak. The toolchain inserts retain and release at the natural
lifetime points:

- **Assign** — retain the new value; release the previous value of the target
  (when it held a class instance).
- **Parameter** — retain for the callee’s binding for the duration of the call
  (exact calling convention is an ABI detail; the observable is that the
  callee’s strong binding participates in the count).
- **Return** — retain for the caller’s received value; release locals that
  go out of scope as the frame unwinds.
- **End of scope** — release every strong local that leaves the block.

The User does **not** manually retain. Hello-world class use needs no dispose
keyword.

### Zero strong → destructor

When an object’s strong count reaches zero, the implementation runs the
destructor chain (most-derived toward base), then frees the allocation. Each
`FUNCTION DESTRUCTOR` runs at most once. A class without a destructor is a
no-op step in the chain. There is no `SUPER` in a destructor; the chain is
implicit (unchanged from 0.4’s destructor chaining shape).

### Weak references (cycles)

Weak references are **non-owning**. They break retain cycles. When the object
dies, a weak reference becomes empty / nil (exact empty form follows the
spelling lock). At least one teaching fixture must show a cycle broken with
weak.

**Spelling TBD (Carlos open).** This draft does **not** invent attribute,
type-wrapper, or method syntax for weak. The EBNF likewise omits fake weak
productions. Until locked, treat “weak” as a semantic requirement with
surface syntax deferred.

### Unowned

**Deferred** out of MVP. Confirm with Carlos before any future surface.

### No force-dispose

There is **no** operation that destroys a live object while other strong
aliases remain. That anti-ARC pattern is rejected for 0.5.0.

### Interpret = reference

The interpreter must implement these ARC rules as the **executable
reference**. LLVM / `bn_rt` must not claim ARC parity until retain/release
insertion and support-matrix evidence exist. Internal `BnArc` is an
implementation detail, not a language API.

## Construction and destruction

### Construction (`NEW`)

`NEW` still allocates. Allocation of `NEW Derived(...)` proceeds as in 0.4:

1. Allocate the most-derived object.
2. Evaluate `SUPER(...)` (explicit or implicit): base field initializers in
   source order, then base constructor body, and so on until the class that
   does not `EXTENDS` another.
3. Run field initializers of the derived class in source order.
4. Run the remainder of the derived constructor body after `SUPER`.

A successful `NEW` yields a strong reference whose count starts at one for
the binding that receives it.

### Lifetime end (no `DELETE`)

Lifetime ends when the **strong count reaches zero** — typically when the last
strong binding is released at end of scope or on reassignment — not when a
manual dispose keyword runs.

While a constructor or destructor of class `C` is executing on an object, a
`PUBLIC` instance method call **on that object** uses the implementation in
`C` (or the nearest ancestor of `C` that declares it), not an override in a
subclass of `C`. After `NEW` returns, and until destruction begins, dispatch
uses the most-derived class as usual. `SUPER.Name(...)` is unchanged.

```basic
CLASS Counter
    PUBLIC count AS INTEGER = 0

    FUNCTION CONSTRUCTOR()
    END FUNCTION

    FUNCTION DESTRUCTOR()
        PRINT "Counter gone"
    END FUNCTION

    PUBLIC FUNCTION Inc() AS VOID
        count = count + 1
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET c AS Counter = NEW Counter()
    c.Inc()
    PRINT c.count
    // c released at end of Start; destructor runs when strong count → 0
END FUNCTION
```

Sharing aliases keeps the object alive until all strong bindings are gone:

```basic
FUNCTION Share() AS VOID
    LET a AS Counter = NEW Counter()
    LET b AS Counter = a
    // both strong; object lives until both leave scope
END FUNCTION
```

Reassignment drops the previous strong binding:

```basic
FUNCTION Rebind() AS VOID
    LET x AS Counter = NEW Counter()
    x = NEW Counter()
    // first instance released if no other strong aliases remain
END FUNCTION
```

## `RELEASE` (optional advanced)

`RELEASE expression` drops **one** strong binding held by the expression’s
designated storage (typically a local or field binding of class type). It
decrements the strong count by one for that binding and leaves the binding
empty / unusable for further strong use of that slot (exact post-state
diagnostics are a frontend concern).

Rules:

- Deinit runs **only** if the strong count reaches zero after this drop.
- `RELEASE` **never** kills all aliases. Other strong references keep the
  object alive.
- Hello and ordinary teaching examples **need not** use `RELEASE`. Prefer
  end of scope and reassignment.
- `RELEASE` is not a rename of force-dispose and is not a substitute for
  HOST `Close`.

```basic
FUNCTION EarlyDrop() AS VOID
    LET c AS Counter = NEW Counter()
    c.Inc()
    RELEASE c
    // if no other strong aliases, destructor runs here
END FUNCTION
```

## HOST: `Close` / `*_close` only

Opaque HOST / registry resources (files, DataFrame handles, net objects,
dispatch tickets, and similar) continue to use capability methods such as
`Close` and `*_close`. **Do not** reintroduce the `DELETE` keyword as sugar
over those closes. Unifying HOST handles into ARC class types is a **later**
proposal, not part of this 0.5.0 DNA lock.

```basic
IMPORT HOST.FileSystem AS FS

FUNCTION ReadOnce(path AS STRING) AS VOID OR Error
    LET file AS FS.File OR Error = FS.Open(path)
    IF file IS Error THEN
        RETURN file
    END IF
    // … use file …
    RETURN file.Close()
END FUNCTION
```

## Typed `AWAIT`

0.4 introduced bounded `ASYNC` / `AWAIT`. 0.5.0 keeps the surface spelling and
amends the **result type** contract:

| Rule | Decision |
| --- | --- |
| Primary surface | Typed `AWAIT` → `T OR Error` when the ticket comes from a named function declared `AS T OR Error` |
| Not primary | `Ticket.Result()` — optional later compat only |
| ABI | Wire to existing `bn_rt_dispatch_await(ticket, timeout_ms, out_result, out_error)` |
| Replay | Replay the stored success value until the ticket is `Close`d (not consume-once) |
| MVP `T` (plan) | `VOID` \| `INTEGER` \| `FLOAT` \| `STRING` \| `BOOLEAN` (STRING confirm still open if needed) |

```basic
IMPORT BNDispatch AS Dispatch

ASYNC FUNCTION Worker(n AS INTEGER) AS INTEGER OR Error
    RETURN n * 2
END FUNCTION

FUNCTION Start() AS VOID OR Error
    LET queue AS Dispatch.Queue OR Error = Dispatch.Queue.Concurrent(4)
    IF queue IS Error THEN
        RETURN queue
    END IF
    LET ticket AS Dispatch.Ticket OR Error = ASYNC queue Worker(21)
    IF ticket IS Error THEN
        RETURN ticket
    END IF
    LET result AS INTEGER OR Error = AWAIT ticket(60000)
    IF result IS Error THEN
        RETURN result
    END IF
    PRINT result
    RETURN ticket.Close()
END FUNCTION
```

`AWAIT ticket(timeoutMs)` still accepts an integer timeout from 1 through
60,000 milliseconds, may be repeated after completion (replay), and never
exposes a native synchronization handle. Queue close cancels pending work;
running work is not force-killed. Timeout, queue failure, or task failure
produces a typed `Error` result.

**Note:** whether replay-until-Close and MVP inclusion of `STRING` remain
fully locked is listed under Open questions if still open in the proposal.

## Migration: 0.4.x → 0.5.0

| 0.4.x pattern | 0.5.0 replacement |
| --- | --- |
| `DELETE x` on a class instance | Prefer end of scope or reassignment so ARC releases the last strong. Use `RELEASE x` only when an intentional early drop of one strong binding is required. |
| `DELETE` on HOST / library handles | Already wrong as keyword sugar in the ARC story — use `Close` / `*_close` (unchanged). |
| Teaching that “you must DELETE” | Teach strong/weak + scope instead. |
| Diagnostics named `USE_AFTER_DELETE`, `DOUBLE_DELETE`, and similar | Rename in later frontend work to ARC-oriented names (e.g. use-after-release / invalid binding). Not required for this docs wave. |

Programs that compiled under 0.4 with `DELETE` are **not** source-compatible
with 0.5.0 grammar: the keyword is gone. Migration is a deliberate break with
a short note, not a soft deprecation period inside the DNA.

## Non-goals (pointer)

Out of 0.5.0 for this corrective tree (see proposal non-goals for the full
list):

- Tracing GC
- Full COW containers / changing struct value semantics
- `unowned` in MVP
- Making DataFrame / File / net handles into ARC classes in the **same** slice
- Shipping `Ticket.Result()` as an equal primary API beside typed `AWAIT`
- Force-dispose / kill-all-aliases
- Keeping or reintroducing the `DELETE` keyword on any BN surface

## Open questions (Carlos)

Still open or confirm-deferred (from the corrective proposal):

1. **Weak spelling** — attribute vs type wrapper vs method; pick one for M1.
2. **0.5.0 tag content** — locks + typed dispatch only, or include interpret ARC (M2)?
3. **Unowned** — confirm deferred.
4. **Dispatch** — confirm **replay until Close** and MVP type set including `STRING` if still open.

Locked and not reopened here: `DELETE` DNA purge; `RELEASE` optional advanced;
HOST `Close` / `*_close` only; typed `AWAIT` primary; ARC compliance required.

## History

- **2026-09-12** — Drafted from `docs/language/0.4/` plus locks in
  `todo/proposals/bucket-0.5.0-corrective.md` (ARC, `DELETE` purge, `RELEASE`,
  typed `AWAIT`, HOST Close). Docs only — no parser/runtime in this commit.
