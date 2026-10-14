# Bucket 0.6.8a — native HOST gaps (from 0.6.2f), two frontend defects, and cleanup

Claim: `bnc` compiles every `HOST` operation `bni` runs, two programs that
pass `bni check` today stop failing at run time, and no user-visible text
names the removed `bn` executable. Collected when bucket 0.6.2 was archived
(2026-09-29); each item re-verified on that date.

## Native HOST (from 0.6.2 H2b/H2c)

`bnc` rejects with `TARGET_UNSUPPORTED_HOST` (probed on `f8c2c80`):

**Moved 2026-10-09 (Carlos):** the `Net.Address` predicates, `Net.CIDR` and
`TCPStream.SetTimeouts` / `Shutdown*` below close in
`ongoing/bucket-0.6.5.md` E.S2.a (BNWeb needs them); they leave this bucket.

- [X] `HOST.NumProcs` — shipped: `tests/host_conformance.rs` `numprocs.bn` is `Native::Same` (verified 2026-10-09)
- [ ] `Net.Address.IsIPv4`, `IsIPv6`, `IsLoopback`, `IsPrivate`,
      `IsLinkLocal`, `IsMulticast` (`bn_rt::net::addressing` has them)
- [ ] `Net.CIDR.Parse` and the `CIDR` members
- [ ] `TCPStream.SetTimeouts`, `ShutdownRead`, `ShutdownWrite`
- [ ] Native `R.Random()` requires `R.Seed` in the same function.
- [ ] Flip each case in `tests/host_conformance.rs` to `Native::Same` and
      refresh book chapter 8 "Interpreter and compiler support".

## Defects (pass `bni check`, fail when run)

- [X] (fixed; verified 2026-10-09: `PI * 2` prints `6` on `bni` and `bnc`) A module reading its own `EXPORT CONST` by bare name: `bni run`
      stops with `UNINITIALIZED_VALUE`; the native build prints `0` for
      `PI * 2` with `PI = 3` (it printed garbage or crashed in 0.6.1).
      Through an import alias (`K.PI`) both are correct.
- [X] (fixed; verified 2026-10-09: alias works, `TRUE` on `bni` and `bnc`) `IMPORT HOST.NumProcs AS N` then `N()`: accepted by `check`; `bni`
      stops with `UNINITIALIZED_VALUE`, the native program dies (status
      133). Either a static error or a working alias; the spec decides.

## Cleanup

- [X] (fixed; verified 2026-10-09: messages say `bni`) `bni eval` says "bn eval requires exactly one source form"
      (`bn_interpret_driver/src/eval.rs:110`); `bni run --help` says "help
      is available as bn --help" (`bn_cli/src/options.rs:149`).
- [X] (fixed; verified 2026-10-09: no `bn run` left in `examples/`) `examples/`: eight `.bn` and two `.md` files still teach `bn run`.
- [ ] (2026-10-09: the fallback now exists for 0.5 tags, but `--no-build` on a 0.6 checkout without `bni` still falls into it) `scripts/install.sh` from a checkout without `target/release/bni`
      falls back to the removed `target/release/bn` (line 262).
- [ ] `language/0.6/memory-migration.md` and `arc-conformance.md` link
      `todo/proposals/bucket-0.5.0-corrective.md`, which is not versioned.

## Triage (2026-10-09)

Shipped items are marked `[X]` above. Still open: native `Net.Address.*`,
`Net.CIDR.*`, `TCPStream.SetTimeouts`/`ShutdownRead`/`ShutdownWrite`
(`Native::Unsupported` in `tests/host_conformance.rs`); native
`R.Random()` without `R.Seed` in the same function (`bnc` still reports
`TARGET_UNSUPPORTED_OP`); the two `language/0.6/` links to
`todo/proposals/bucket-0.5.0-corrective.md` (resolve once `todo/` is
versioned); `install.sh --no-build` on an unbuilt 0.6 checkout. Rescheduled after 0.6.7 as 0.6.8a.
