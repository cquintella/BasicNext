# Basic Next Flow and Experience Contract

Basic Next should make programming pleasurable by preserving attention. This
contract turns that goal into requirements for the language, `bn` tool, and
future editor support.

## Flow by clarity

Common code must reveal its purpose locally, follow predictable rules, and
keep the path from idea to feedback short. A language feature earns its place
only when it reduces cognitive load or solves a concrete problem that justifies
its cost.

BN avoids syntax that hides meaningful control flow, allocation, error
propagation, or host interaction. This does not ban concise local operations
such as `+=`; it rejects magic that forces readers to reconstruct important
steps elsewhere.

## Structural reading

Every compound form closes with `END` and the opening keyword. This makes a
program's coarse structure visible before a reader understands its details.
Indentation is not syntactic, but code inside a block should be indented
consistently; the official examples use four spaces per nesting level.

```basic
WHILE active
    IF ready THEN
        Process()
    END IF
END WHILE
```

The official BN editor or editor extension should highlight the opening and
closing members of these pairs together. This is a tooling requirement, not a
new language construct.

## Explicit local contracts

Every `LET` and `CONST` declaration has an explicit `AS TYPE`. BN must not add
an inferred-binding shortcut for scripts, prototypes, or other special modes.
One consistent declaration rule is easier to learn and keeps a binding's
contract visible at the point where it is introduced.

## One concept, one form

BN keeps synonyms close to zero. A new language concept receives one canonical
spelling and one mental model. `//` is the only line-comment form; new features
must not introduce alternative spellings for the same concept.

Host capabilities use the same import form as modules:

```basic
IMPORT HOST.memory AS Memory
```

Plugins, extensions, and host APIs must not introduce a parallel import or
activation syntax.

## One tool, one diagnostic model

The official tool is branded `BN` and invoked as `bn`:

```text
bn check file.bn
bn run file.bn
bn build file.bn
```

Every command uses the same diagnostic shape, source locations, terminology,
and exit-code model. Diagnostics should identify the problem, point to the
relevant source, explain the likely cause in plain language, and suggest the
smallest useful correction. They must be understandable without requiring the
programmer to leave the current tool and search a numeric error-code catalogue.

The trivial path is zero-config: `bn run hello.bn` must work without a project
file or manifest. A manifest is introduced only when a program needs declared
dependencies, package metadata, or non-default build configuration.

## Complexity ladder and version contracts

Every Basic Next language version must state its **minimum useful program
subset**: the smallest group of constructs sufficient for a complete program
with useful input, processing, and output. The 0.1 subset is defined in the
language specification.

Features beyond that subset are introduced as a visible complexity ladder:

1. Program core: entry point, explicit bindings, expressions, control flow,
   functions, and console I/O.
2. Structure: classes, interfaces, modules, and exports.
3. Environment: `HOST` capabilities and standard libraries.
4. Specialized work: typed memory, data facilities, parallel devices, and
   other advanced capabilities.

The language keeps a small core with broad reach: users meet advanced concepts
only when their program needs them.
