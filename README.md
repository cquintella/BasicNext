# Basic Next

An object-oriented, general-purpose programming language designed to reduce
cognitive load and turn ideas into clear, cross-platform software.

Basic Next combines BASIC-inspired readability, explicit types, and host
capabilities without prescribing a framework or architecture. It is designed
to make programming pleasurable: clear, predictable code should help
programmers sustain attention and enter a state of flow while reading and
writing software.

This repository starts with the specification: an implementation is introduced
only after the corresponding semantics have been defined and reviewed.

## Design goals

- Readability before abbreviation.
- Low cognitive load, flow by clarity, and explicit contracts.
- KISS: complexity must solve a concrete problem.
- Every `LET` and `CONST` declaration states its type explicitly.
- Clean Code and Clean Architecture should be natural, never mandatory.
- Cross-platform software through `HOST` capabilities rather than vendor APIs.

Read [PHILOSOPHY.md](PHILOSOPHY.md) for the mission, vision, and complete set
of design principles.

## Status

**Reference implementation in progress.** The lexer, parser, core semantic
analyzer, typed BN IR, and core IR interpreter are executable. The extended
0.1 runtime for modules, objects, pointers, and `HOST` capabilities is not yet
complete, and there is no stable API or release yet.

## Active implementation objective

Build the Basic Next 0.1 reference implementation in Rust: a source-spanned
lexer, handwritten recursive-descent/Pratt parser, syntax AST, semantic
analyzer, typed BN IR, and deterministic IR interpreter. It provides:

- `bn check file.bn`, accepting every valid conformance fixture and reporting a
  documented source-spanned diagnostic for every invalid fixture;
- `bn run file.bn`, executing the accepted 0.1 language semantics and official
  examples without unchecked runtime failures; and
- one shared conformance suite for the interpreter and any later compiler.

Native-code/WebAssembly compilation, `bn build`, optional `HOST` capabilities,
and proposed scientific-library surfaces are not part of this implementation
objective. See the [0.1 WBS](docs/project/WBS-0.1.md) for acceptance criteria
and delivery order.

`bn check -v file.bn` reports completed stages and `bn check -vv file.bn` also
prints lexer tokens. Frontend artifacts are available through `--emit tokens`,
`--emit ast`, `--emit typed-ast`, and `--emit ir`; `-o file` writes an emitted
artifact to a file.

## Tool

`BN` is the official Basic Next tool, invoked as `bn`. The 0.1 reference
implementation provides `bn check file.bn` and `bn run file.bn`; a post-0.1
compiler may later add `bn build file.bn`. The commands share one diagnostic
format, source locations, and exit-code model.

The trivial case is zero-config: `bn run hello.bn` does not require a project
file or manifest. While developing from this repository, use:

```shell
cargo run -- run examples/hello.bn
cargo run -- check --emit ir examples/factorial.bn
cargo run -- --help
```

## Repository layout

- `docs/language/0.1.md` — minimum language specification.
- `docs/proposals/` — ideas that are not part of the language yet.
- `docs/project/` — delivery planning documents.
- `docs/project/experience-contract.md` — flow and developer-experience rules.
- `examples/` — programs that guide the specification.
- `PHILOSOPHY.md` — design principles.
- `ROADMAP.md` — incremental delivery roadmap.
- `GOVERNANCE.md` — how decisions are made.
- `TRADEMARK.md` — use of the project name.

## Example

```basic
IMPORT HOST.main AS main

FUNCTION Start() AS VOID
    LET counter AS INTEGER = 0

    WHILE counter < 10
        PRINT "Basic Next ", counter
        counter += 1
    END WHILE
END FUNCTION
```

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Language evolution begins as proposals
in `docs/proposals/`; specification changes require examples.

## Support

See [SPONSORSHIP.md](SPONSORSHIP.md) to support Basic Next maintenance without
interfering with its technical governance.

## License

[MIT](LICENSE).
