# Basic Next Philosophy

Basic Next pursues BASIC's readability while retaining explicit types,
modularity, and useful diagnostics. Its design is informed by Zen: clarity,
restraint, and deliberate choices over novelty.

1. A program should be easy to read before it is easy to abbreviate.
2. Explicit types are part of a program's contract, not optional decoration.
3. Simple is better than clever; one clear form is better than several nearly
   equivalent forms.
4. The standard library and host capabilities provide features; reserved words
   express only fundamental language concepts.
5. A new reserved word is justified only when existing constructs or a library
   cannot express the concept adequately.
6. The specification precedes the implementation, and executable examples
   validate the specification.
7. The language should grow by small, reversible steps. Proposals precede
   commitments, and commitments precede implementations.
8. Host integration must preserve portability: a program states the capability
   it needs, while an environment chooses the supported implementation.
9. The project should support incremental implementation in a Compilers course.
