# Basic Next

A modern programming language with BASIC-inspired syntax, designed as an open
source project and a teaching laboratory for Compilers courses.

This repository starts with the specification: an implementation is introduced
only after the corresponding semantics have been defined and reviewed.

## Status

**Pre-implementation.** Language version 0.1 is under discussion; there is no
compiler, runtime, or stable API yet.

## Repository layout

- `docs/language/0.1.md` — minimum language specification.
- `docs/proposals/` — ideas that are not part of the language yet.
- `docs/project/` — delivery planning documents.
- `examples/` — programs that guide the specification.
- `PHILOSOPHY.md` — design principles.
- `ROADMAP.md` — incremental delivery roadmap.
- `GOVERNANCE.md` — how decisions are made.
- `TRADEMARK.md` — use of the project name.

## Example

```basic
IMPORT HOST.main AS main

SUB Start()
    LET counter AS INTEGER = 0

    WHILE counter < 10
        PRINT "Basic Next", counter
        counter += 1
    END WHILE
END SUB
```

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). Language evolution begins as proposals
in `docs/proposals/`; specification changes require examples.

## Support

See [SPONSORSHIP.md](SPONSORSHIP.md) to support Basic Next maintenance without
interfering with its technical governance.

## License

[MIT](LICENSE).
