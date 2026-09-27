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

For CLI, LSP, or DAP changes, preserve the shared frontend → IR sequence; do
not add a parallel parser, semantic analyzer, lowering path, or interpreter
entrypoint.

The dependency-direction checker uses the reviewed baseline in
[`scripts/forbidden-deps.allowlist`](scripts/forbidden-deps.allowlist). It
rejects new backend references to frontend modules and frontend references to
runtime execution entrypoints; update the allowlist only with an explicit
architecture decision.

See the [README](README.md) for how to install and run programs, and
[`bni(1)`](man/bni.1) and [`bnc(1)`](man/bnc.1) for the Unix man pages.

The project's governance is defined in the project governance document,
maintained outside this repository. A code of conduct will be added before
public contributions are opened at scale.
