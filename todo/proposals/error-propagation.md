# Proposal: Early `Error` propagation (language)

**Status:** Proposed — **language DNA** (not a toolchain-only change).
**Date:** 2026-09-06
**Does not** change toolchain diagnostics / Fluent / `validate_for`. Those remain a
separate channel from the BN value `Error` (`docs/library/error.md`).

**Architecture impact:** None required on the FE→IR→interpret|compile shape.
If accepted, Frontend lowers the new form to existing IR control flow + `return`
of `Error`; both backends consume the same IR. No second exception runtime.

---

## Problem

Basic Next models expected failure as values: many APIs return `T OR Error`
(files, HOST, web, dispatch, …). Call sites must **explicitly** branch or
re-wrap on every step:

```text
LET file AS FS.File OR Error = FS.Open("data.txt", FS.READ)
IF file IS Error THEN
  RETURN file
END IF
LET n AS INTEGER OR Error = file.ReadBytes(buf)
IF n IS Error THEN
  RETURN n
END IF
```

That is honest and exception-free, but **verbose**. In deep call chains the
noise hides the happy path. Languages with the same “result value” model
(notably Rust’s `Result` + `?`) offer a **concise early return** when the
enclosing function already returns `… OR Error`: on failure, propagate the
`Error` to the caller immediately; on success, unwrap `T` for the rest of the
expression/statement.

BN has **no** such operator today. There is also **no** `try/catch`. This
proposal asks only for early propagation of the existing `Error` value — not
exceptions, stack traces, or `Error.Causes`.

---

## Goals

1. A **small**, explicit surface to propagate `Error` from a `T OR Error`
   operand to the **caller** when the current function’s result type allows it.
2. Preserve the mental model: failure is still a **value**, not an unwind.
3. Keep cognitive load low: one operator, strict type rules, no hidden catch.
4. Lowering must be expressible in today’s IR (branch + return) so interpret and
   compile stay equivalent on the support subset.

## Non-goals

- `try` / `catch` / `finally`, resumable exceptions, or panic-style abort as
  the primary API failure mode.
- Changing `Error` fields (`Code`, `Message`) or adding `Error.Causes`.
- Mixing toolchain diagnostics (`error[CODE]` from `bn check` / build) with
  program `Error` values.
- Automatic propagation across functions that do **not** return `… OR Error`.
- Changing overflow/trap policy (numeric traps remain as specified today).

---

## Proposed solution

### Surface (preferred)

Introduce a **postfix propagation operator** `?` on expressions whose static
type is `T OR Error` (and analogous multi-alternatives that include `Error`,
subject to the rules below).

**Informal meaning** inside a function whose return type is `U OR Error`
(or a documented superset that includes `Error`):

```text
expr?
```

1. Evaluate `expr` to a value of type `T OR Error`.
2. If the value **is** `Error`, **return** that `Error` from the current
   function immediately (same object / observable fields).
3. If the value is the `T` alternative, the type of `expr?` is `T` and
   evaluation continues.

Sketch:

```text
FUNCTION LoadFirst(path AS STRING) AS STRING OR Error
  LET file AS FS.File = FS.Open(path, FS.READ)?
  LET n AS INTEGER = file.ReadBytes(buf)?
  RETURN Decode(buf, n)?
END FUNCTION
```

### Type rules (normative intent)

| Context | Rule |
| --- | --- |
| Operand type | Must be `T OR Error` (exact shape TBD with `OR` chains that include `Error`, e.g. `INTEGER OR EOF OR Error` — see open questions). |
| Enclosing callable | Result type must include `Error` (typically `U OR Error`) so a returned `Error` is type-correct. |
| Otherwise | Static error — do not insert implicit `Error` into a `VOID` / pure `T` return. |
| `?` on non-OR types | Static error. |
| Interaction with `IS Error` | Still allowed; `?` is sugar for early return, not a replacement for local handling. |

### Lowering (toolchain — no arch fork)

Desugar before or during lower to IR equivalent to:

```text
LET tmp AS T OR Error = expr
IF tmp IS Error THEN
  RETURN tmp
END IF
# use tmp as T
```

So: same CFG / definite-assignment story; no new runtime exception path;
`validate` / interpret / compile see normal control flow. Support-matrix and
`validate_for` are unaffected except insofar as new surface uses existing ops.

### Syntax alternatives (if `?` is rejected)

| Option | Pros | Cons |
| --- | --- | --- |
| **A. Postfix `?` (preferred)** | Familiar; tiny; expression-friendly | New token; BASIC readers may not expect it |
| **B. Keyword prefix `CHECK expr`** | Very explicit | More DNA; awkward in nested expressions |
| **C. Statement `PROPAGATE expr`** | Clear statement form | Does not help inside larger expressions |

Stakeholder pick is required before EBNF freeze. Default recommendation: **A**.

---

## Spec / doc touch list (when accepted)

1. `language/0.4/0.4.ebnf` — postfix op in expression grammar + precedence.
2. `language/0.4/0.4.md` — static rules + examples; clarify vs toolchain diagnostics.
3. `language/0.4/keywords.md` — only if a keyword form (B/C) wins; `?` is not a keyword.
4. `docs/library/error.md` — point to propagation rules; `Error` object unchanged.
5. Positive/negative fixtures; interpret + compile parity on the support subset.
6. Optional: mention in `docs/architecture/value-memory-abi.md` that propagation
   is language sugar over return-of-`Error`, not a new ABI.

---

## Open questions

| Id | Question |
| --- | --- |
| Q1 | Exact behaviour for `INTEGER OR EOF OR Error` (and similar): does `?` propagate only `Error`, leaving `EOF` as a value? **Recommended:** yes — propagate **only** the `Error` alternative; other alternatives remain. |
| Q2 | Allowed in `ASYNC FUNCTION` / `AWAIT` chains? **Recommended:** yes, same rules on the task function’s `… OR Error` result. |
| Q3 | Allowed at module/script top level outside a function? **Recommended:** no — only inside callables with a suitable return type (keeps REPL/script policy explicit later). |
| Q4 | Precedence relative to member access / calls (`a.b()?` vs `(a.b())?`)? **Recommended:** same binding tightness band as postfix call/member; document in EBNF. |
| Q5 | Which release bucket if accepted (language bump vs 0.4.x amendment)? Stakeholder schedule — not 0.4.4 soft-prep; likely post–0.4.5 or a dedicated language slice. |

---

## Acceptance criteria

- [ ] Surface syntax chosen (A/B/C) and recorded here as locked.
- [ ] EBNF + `0.4.md` rules merged; fixtures green on interpret.
- [ ] Compile path matches interpret on the support subset for `?` programs.
- [ ] Static errors for illegal contexts (no `OR Error` return; non-OR operand).
- [ ] Explicit note: toolchain diagnostics remain a separate channel.

## Rejected / out of scope reminders

- Using Fluent / `DiagId` to “pretty-print” program `Error` as if it were
  `bn check` output — different channel.
- Implicit `?` on every `OR Error` without spelling the operator.

## See also

- [`docs/library/error.md`](../../docs/library/error.md)
- [`language/0.4/0.4.md`](../../language/0.4/0.4.md)
- [`expressive-diagnostics.md`](expressive-diagnostics.md) — toolchain only
- Architecture: FE→IR→interpret|compile unchanged
