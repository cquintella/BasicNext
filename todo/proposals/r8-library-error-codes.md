# Proposal: portable `Error` codes for the BN* modules (bucket 0.6.2b R8)

Status: **approved 2026-09-28**. BNCrypto and BNJson shipped in 0.6.2b
(BNJson with `PARSE_FAILED`: `Parse` returns `Error`); the rest follow in
`todo/bucket-0.6.2d-library-codes.md`.

## Why

Every BN* module returns `T OR Error` with `Code 1` for every failure
(BNWeb 58 sites, BNData 26, BNLog 20, BNDispatch 4, BNJson 2, BNCrypto 1 at
the last count), so a program cannot tell a missing key from a closed
logger. BNDispatch prints its failure as the Rust enum name ("Timeout",
"InvalidTimeout"). `error.md` asks for "a portable constant of the capability
or module that owns the operation"; HOST.FileSystem, HOST.Net, and HOST.Exec
already publish theirs.

## Principles

1. One table per module, in the module's own spec file
   (`language/0.6/bn<module>.md`), exposed as `INTEGER` members of the
   import alias (`Json.NOT_FOUND`), exactly as `FS.*` and `Net.*`.
2. Codes classify the **failure**, not the message: a program branches on
   them. The message names the inputs; the cause says why (error.md).
3. The same names mean the same thing in every module: `INVALID_ARGUMENT`
   is always `1`; the other shared names (`NOT_FOUND`, `LIMIT`, `CLOSED`,
   `TIMEOUT`, `UNAVAILABLE`, `IO_FAILED`, `POLICY_DENIED`) keep the meaning
   they have in `host-net.md`, with module-local values.
4. `UNAVAILABLE` is "this build or host does not provide the operation";
   it is not a policy denial (`POLICY_DENIED`).
5. No code for a program error the checker already rejects (a type error at
   the call site stays a diagnostic).

## BNCrypto (`language/0.6/bncrypto.md`)

| Constant | Value | When |
| --- | ---: | --- |
| `Crypto.INVALID_ARGUMENT` | 1 | A key, nonce, seed, or salt of the wrong length; Argon2id cost parameters out of range; `FromHex` of odd-length or non-hex text; `Slice` outside the buffer |
| `Crypto.AUTHENTICATION_FAILED` | 2 | `Open*` rejects the ciphertext (tampered data, AAD, key, nonce, or tag); `MlKemDecapsulate` rejects the ciphertext |
| `Crypto.UNAVAILABLE` | 3 | The build has no provider for the algorithm |

`Verify*` keep returning `FALSE`, never an `Error` (the spec's fail-closed
rule).

## BNJson (`language/0.6/bnjson.md`)

| Constant | Value | When |
| --- | ---: | --- |
| `Json.INVALID_ARGUMENT` | 1 | A non-finite `FLOAT` written into a document |
| `Json.NOT_FOUND` | 2 | A missing key |
| `Json.TYPE_MISMATCH` | 3 | The value at the key or index is not the requested type; `Length` of a scalar; a write into a value that is not an object or array |
| `Json.OUT_OF_RANGE` | 4 | An array index outside the array |
| `Json.LIMIT` | 5 | A write past the depth limit |
| `Json.UNAVAILABLE` | 6 | The operation is not provided |

Open question for the review: `Parse` of invalid text is a runtime
diagnostic today (`INVALID_JSON`, the program stops). If `Parse` should
return `Error`, add `Json.PARSE_FAILED` (7); that is a behaviour change,
so it is listed, not assumed.

## BNDispatch (`language/0.6/0.6.md` "BNDispatch" section)

The runtime already distinguishes these failures (`DispatchError`); they
only lack codes and text.

| Constant | Value | When |
| --- | ---: | --- |
| `Dispatch.INVALID_ARGUMENT` | 1 | A worker count outside 1..64, barrier parties outside 1..64, semaphore permits outside 1..1024, a timeout out of range |
| `Dispatch.TIMEOUT` | 2 | `AWAIT`, `Join`, a barrier, a semaphore, or a group wait ran past its timeout |
| `Dispatch.CLOSED` | 3 | Submitting to, or waiting on, a closed queue or ticket |
| `Dispatch.SATURATED` | 4 | The queue's bounded backlog is full |
| `Dispatch.CANCELLED` | 5 | The ticket was cancelled |
| `Dispatch.TASK_FAILED` | 6 | The worker returned an `Error`; `Cause` carries the worker's message (its own code is kept in the message) |
| `Dispatch.INVALID_STATE` | 7 | A group released more than it added, a semaphore released without being held, a lock released by another task, a queue joined from its own worker |
| `Dispatch.UNAVAILABLE` | 8 | Entropy for ticket ids is unavailable, or the operation is not provided |

## BNLog (`language/0.6/bnlog.md`)

| Constant | Value | When |
| --- | ---: | --- |
| `Log.INVALID_ARGUMENT` | 1 | An unknown level; an invalid transport; a label, message, field key, or field value past its bound |
| `Log.DUPLICATE` | 2 | A field or entry key that already exists |
| `Log.LIMIT` | 3 | Too many transports, fields, or entry fields |
| `Log.CLOSED` | 4 | An operation on a closed logger |
| `Log.IO_FAILED` | 5 | A transport could not write or flush |
| `Log.CAPABILITY_REQUIRED` | 6 | `AddFile` without `IMPORT HOST.FileSystem`, `AddConsole` without `IMPORT HOST.Console` (bnlog.md: programs import every capability they use) |
| `Log.TIMEOUT` | 7 | `Flush` or `Close` ran past its timeout |
| `Log.UNAVAILABLE` | 8 | The operation or provider is not available |

## BNData (`language/0.6/bndata.md`)

| Constant | Value | When |
| --- | ---: | --- |
| `Data.INVALID_ARGUMENT` | 1 | An invalid CSV separator; a negative slice bound; a destination of another length |
| `Data.NOT_FOUND` | 2 | A column name that does not exist |
| `Data.TYPE_MISMATCH` | 3 | A column of another type (or NA where none is allowed); a failed column conversion |
| `Data.OUT_OF_RANGE` | 4 | A row or column index outside the frame |
| `Data.IO_FAILED` | 5 | Reading or writing the CSV file failed (the FS `Error` keeps its own code in `Cause`) |
| `Data.INVALID_FORMAT` | 6 | Ragged rows or unparsable CSV |

## BNWeb (`language/0.6/bnweb.md`)

| Constant | Value | When |
| --- | ---: | --- |
| `Web.INVALID_ARGUMENT` | 1 | A status outside 100..599; an unsupported method; a negative cookie age; an invalid SameSite policy or `SameSite=None` without `Secure`; an invalid session idle timeout; a route handler or filter that is not a FUNCTION |
| `Web.NOT_FOUND` | 2 | A cookie, session, header, or route that does not exist |
| `Web.OUT_OF_RANGE` | 3 | An index outside a collection |
| `Web.LIMIT` | 4 | Too many filters, handlers, or connections |
| `Web.CLOSED` | 5 | The server is not accepting requests (stopped or not started) |
| `Web.TIMEOUT` | 6 | A request, response, or egress call ran past its deadline |
| `Web.EGRESS_DENIED` | 7 | The egress policy denies the destination or the redirect |
| `Web.HTTP_FAILED` | 8 | An outbound request failed (connection, TLS, protocol); `Cause` carries the Net or TLS reason |
| `Web.UNAVAILABLE` | 9 | The provider (TLS, session store, scraper, …) is not in this build |

## Implementation (after approval), per module

1. The table in the module's spec file; `TODO (normative gap)` removed.
2. `bn_types::error_codes::<module>` constants, exposed as alias members
   through `host_spec::declare_error_codes` (as FS/Net).
3. One producer per module (`<Module>Error { code, operation, message,
   cause }`) used by the interpreter provider and, where the module has a
   native ABI, by `bn_rt`.
4. A `tests/host/<module>_errors.bn` parity fixture covering every code
   reachable without the network.

Order: BNCrypto, BNJson, BNDispatch, BNLog, BNData, BNWeb (smallest first).
