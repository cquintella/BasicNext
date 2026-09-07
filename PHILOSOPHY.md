# Basic Next Philosophy

Basic Next is a small, explicitly typed, object-oriented language built to
turn ideas into clear, portable software without wasting the programmer's
attention. Design favors restraint and deliberate choices over novelty.

## Mission

Make programming more readable, predictable, and accessible through a small
language with explicit types and a short path from idea to working program.

## Design principles

1. **Low cognitive load** — common code should be easy to understand. Intent
   stays local, rules stay predictable, and feedback stays close to the edit.
2. **Readability first** — source code is communication between people, not
   only instructions to a machine.
3. **Ideas into software** — useful abstractions should be direct to express.
   Prefer the simplest construct that states the idea clearly (KISS).
   Complexity must earn its place through a concrete problem, not anticipation.
4. **Objects where they help** — behavior belongs to cohesive objects when
   state and responsibility travel together. Straight procedural code remains
   fine for scripts and examples; object orientation is the default model for
   structure, not a tax on every line.
5. **Small core, broad reach** — richness lives in well-typed, replaceable,
   explicitly imported modules and `HOST` capabilities. Reserved words cover
   only fundamental language concepts. A feature earns new syntax only when a
   module or capability cannot express it clearly.
6. **Cross-platform through capabilities** — programs target `HOST`
   capabilities, not vendors or operating systems.
7. **Explicit contracts** — types, boundaries, and effects should not surprise.
   Every variable and constant declaration states its type. Imports are
   explicit; standard libraries are not a silent prelude.
8. **One validated meaning** — the language specification precedes
   implementation. The interpreter is the executable reference; compile and
   interpret consume the same validated intermediate representation on the
   supported subset.
9. **Deliberate evolution** — proposals, working examples, simplicity, and
   compatibility outweigh novelty. Clean structure should feel natural in the
   language without imposing a framework or a prescribed application
   architecture.

## Anti-goals

Basic Next does not aim to be:

- a kitchen-sink language that absorbs every popular feature
- a vendor- or OS-specific toolchain disguised as portable
- a framework that dictates how applications must be layered
- an implicitly typed dialect where types are optional or inferred by default
- a moving target where implementation outruns the specification

## How to use this document

When a design choice is unclear, prefer the option that lowers cognitive load,
keeps the core small, keeps contracts explicit, and can be shown in a short
example. If a change needs new syntax, first ask whether a module or `HOST`
capability already says it clearly.
