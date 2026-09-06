# Contributing

Before implementing a language change, open a proposal in `todo/proposals/`
that states the motivation, examples, and grammar or semantic impact.
Accepted proposals are archived under `done/proposals/`.

For changes to 0.1:

1. Keep the scope small.
2. Update the specification and an affected example.
3. Do not introduce dependencies without justification.
4. Do not change unrelated files.

Before sending a change, run:

```shell
tests/check-forbidden-deps.sh
cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings && git diff --check
```

For CLI, LSP, or DAP changes, preserve the shared frontend → IR sequence
documented in [`docs/architecture/driver-sequence.md`](docs/architecture/driver-sequence.md);
do not add a parallel parser, semantic analyzer, lowering path, or interpreter
entrypoint.

The dependency-direction checker uses the reviewed baseline in
[`scripts/forbidden-deps.allowlist`](scripts/forbidden-deps.allowlist). It
rejects new backend references to frontend modules and frontend references to
runtime execution entrypoints; update the allowlist only with an explicit
architecture decision.

See [`docs/project/usage.md`](docs/project/usage.md) for how to run programs
and [`docs/man/bn.1`](docs/man/bn.1) for the Unix man page.

The project's governance is defined in [GOVERNANCE.md](GOVERNANCE.md). A code
of conduct will be added before public contributions are opened at scale.
