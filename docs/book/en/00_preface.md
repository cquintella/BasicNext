# Preface

[Contents](toc.md)

My name is Carlos Álvaro Quintella, and Basic Next is a language I started because of a problem I could describe precisely but could not solve by changing tools. Over a career spent in systems architecture, infrastructure, and teaching, I watched the distance grow between having an idea about a program and having the program. The distance is not made of algorithms. It is made of build configuration, framework conventions, implicit conversions that have to be remembered rather than read, and error messages that arrive at runtime, in production, about a decision made months earlier.

This preface explains the reasoning that produced the language, states what it does and does not contain, and describes how the rest of the book is organized.

## The Problem the Language Addresses

Programming is a task with a high working-memory cost. Psychological research on optimal experience describes a state, commonly called flow, in which attention is fully absorbed by a task whose difficulty matches the practitioner's skill; the characteristic conditions include clear goals, immediate feedback, and the absence of attentional demands unrelated to the task itself (Csikszentmihalyi, 1990). Programming satisfies the first two conditions when the tools cooperate. The third is where most of the loss occurs.

Software engineering has its own name for the loss. Brooks distinguished the *essential* difficulty of a software problem — the complexity of the thing being modelled — from the *accidental* difficulty introduced by the representation chosen for it, and argued that progress in the discipline comes from reducing the second (Brooks, 1987). A programmer who spends an afternoon on a dependency conflict has spent it entirely on accidental difficulty. So has one who spends it locating an implicit conversion between a signed and an unsigned integer, or determining which of four configuration files supplies a setting at runtime.

Basic Next is an attempt to reduce accidental difficulty at the level where it is cheapest to remove: the language itself. The specific decisions follow from that goal, and each of them costs something.

## What the Decisions Are

**Types are written, not inferred across boundaries.** Every binding, parameter, field, and return value states its type. This is more to type and less to guess. A reader of a function knows its contract from its signature without consulting the definitions of everything it calls, and a change that breaks the contract is reported where the contract is written.

**There are no implicit conversions.** A value of one numeric type does not silently become another, integer overflow raises `NUMERIC_OVERFLOW` rather than wrapping, and a condition must have type `BOOLEAN` rather than being interpreted as one. The cost is explicit casts in arithmetic that mixes widths. What is bought is that the set of things a program can do when it runs is the set of things its source says it does.

**Every block is closed by an explicit `END`.** `END IF`, `END WHILE`, `END FUNCTION`. Indentation carries no meaning. The structure of a program survives being pasted into an email, and a misplaced line produces a syntax error rather than a silently different program.

**Every execution path of a non-`VOID` function must return.** The compiler verifies this and rejects the program otherwise. Functions that can fail say so in their return type — `INTEGER OR NA`, `File OR Error` — and the caller cannot reach the value without handling the alternative first.

**Memory is managed by automatic reference counting, not by a tracing collector.** Object lifetimes follow scope and reference count, destructors run at a determinable point, and cycles are broken with `WEAK` references written by the programmer. The cost is that reference cycles are the programmer's responsibility. What is bought is that a program's memory behaviour does not depend on when a collector decides to run.

**The core is small and the reach is modular.** `HOST` is the only built-in interface to the outside world, and every library capability — mathematics, JSON, logging, HTTP, tabular data, concurrency — is a module that must be imported by name. Nothing is in scope that was not asked for.

These decisions have a common shape. Each one moves a class of error from runtime to compile time, or from implicit to written. Each one costs keystrokes. The bet of the language is that the trade is favourable for the programs most people write, and especially for the programs people write while learning.

## What Was Borrowed

The syntax is deliberately familiar, and the sources are specific rather than sentimental.

From BASIC comes the keyword vocabulary and the statement shapes — `LET`, `PRINT`, `FOR ... NEXT` rendered as `FOR ... END FOR` — which were designed at Dartmouth for students who were not specialists and which remain readable for the same reason (Kemeny and Kurtz, 1964). From Pascal comes the discipline of declaring before use and the structured-programming posture that a block has one entry and one exit (Wirth, 1971). From C comes the small core and the willingness to expose the machine: pointers, fixed-width integers, and a defined memory layout, but written without C's declarator syntax. From C++ comes the class model used for organizing state and behaviour, restricted to single inheritance with interfaces. From R comes the treatment of vectors as first-class values for numerical work.

Two omissions are equally deliberate. Implicit typing was not taken from Python, because a type that is inferred at the point of use is a type that must be reconstructed by every subsequent reader. The layered abstraction culture of enterprise Java was not taken either, because the ceremony required to express a small program in it is accidental difficulty by Brooks's definition.

## What the Language Does Not Have

A language is defined as much by its exclusions, and stating them early is more useful than discovering them in chapter nine. As of version 0.5, Basic Next has no lambdas or closures, no generic classes, no `MATCH` or `ENUM` constructs, no variable-size collections in the core language, no string interpolation, no package registry, and no foreign function interface to C. Function values exist but are restricted to module-level functions and static methods. Compilation to native code and to WebAssembly covers a typed subset of the language, and a program outside that subset is rejected before emission with a diagnostic that names the reason rather than being compiled into something that behaves differently.

Some of these are scheduled and some are decided against. The normative statement of scope is the language specification, not this book; where the two disagree, the specification governs.

## Why the Language Is Taught in This Book

Basic Next was designed with a second audience in mind: the student who is meeting allocation, types, control flow, and memory for the first time. A language for that purpose has an obligation that a production language does not. It has to make the machine visible without making it cryptic, and it has to fail early and say why.

That is why the diagnostics appear throughout this book alongside the constructs that produce them. Learning which programs a compiler rejects, and for what reason, is a substantial part of learning to write programs it accepts. Each chapter therefore states a rule, shows the message the compiler emits when the rule is broken, and then shows how to structure code so that the rule is satisfied by intent rather than by accident.

## How This Book Is Organized

The introduction covers the toolchain, the first program, and the shape of a module. Chapters two through five cover the constructs every program uses: variables and types, control flow, compound data and error handling, and functions and program structure. Chapters six and seven cover classes, interfaces, and the memory model. Chapter eight covers the standard library and the `HOST` boundary, chapter nine input, output, and concurrency, and chapter ten the execution policy that governs what a compiled program is permitted to do. The appendices index the keywords, the diagnostics, the normative grammar, the command-line tool, and the provider-backed modules.

Every code example in the early chapters has been executed against the reference toolchain, and the outputs shown are the outputs produced. Where a chapter quotes a diagnostic, that is the text the compiler emits.

The practices recommended alongside the language — naming a condition that expresses a rule, rejecting invalid input at the top of a function, keeping a function to one level of abstraction — are not specific to Basic Next. They come from the literature on software construction (Martin, 2008; McConnell, 2004) and hold wherever these constructs exist. The language is arranged so that following them is the path of least resistance rather than an act of discipline.

## References

- Brooks, F. P. (1987). *No Silver Bullet: Essence and Accidents of Software Engineering*. Computer, 20(4), 10–19. https://doi.org/10.1109/MC.1987.1663532
- Csikszentmihalyi, M. (1990). *Flow: The Psychology of Optimal Experience*. Harper & Row.
- Kemeny, J. G., and Kurtz, T. E. (1964). *BASIC: A Manual for BASIC*. Dartmouth College Computation Center. https://www.dartmouth.edu/basicfifty/basicmanual_1964.pdf
- Martin, R. C. (2008). *Clean Code: A Handbook of Agile Software Craftsmanship*. Prentice Hall.
- McConnell, S. (2004). *Code Complete: A Practical Handbook of Software Construction* (2nd ed.). Microsoft Press.
- Wirth, N. (1971). *Program Development by Stepwise Refinement*. Communications of the ACM, 14(4), 221–227. https://doi.org/10.1145/362575.362577

---

[Next: Introduction →](01_introduction.md)
