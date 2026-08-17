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

**Pre-implementation.** Language version 0.1 is under discussion; there is no
compiler, runtime, or stable API yet.

## Tool

`BN` is the official Basic Next tool, invoked as `bn`. It is the single entry
point for checking, running, and, where supported, building `.bn` source files:
`bn check file.bn`, `bn run file.bn`, and `bn build file.bn`. These commands
share one diagnostic format, source locations, and exit-code model.

The trivial case is zero-config: `bn run hello.bn` must not require a project
file or manifest. This command contract is specified; the tool itself is not
implemented yet.

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
        PRINT "Basic Next", counter
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
