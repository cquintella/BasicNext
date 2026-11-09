# Basic Next Examples — Table of Contents (TOC)

This document provides a comprehensive catalog of all example programs in the `examples/` directory. Each entry outlines the purpose of the program, demonstrated language features, and relevant dependencies.

## Language Core & Fundamentals

- [hello.bn](hello.bn): Minimal canonical entry point demonstrating `FUNCTION Start()` and basic `PRINT` output.
- [variables.bn](variables.bn): Variable declarations (`LET`), constants (`CONST`), scalar data types, and initializers.
- [arithmetic.bn](arithmetic.bn): Arithmetic operators, compound assignments (`+=`, `-=`), and statement increments/decrements (`++`, `--`).
- [div.bn](div.bn): Integer division (`DIV`) and remainder arithmetic (`%`).
- [control-flow.bn](control-flow.bn): Flow control constructs including block `IF`/`ELSE`, inline conditionals, `WHILE`, `REPEAT...UNTIL`, counted `FOR`, and vector `FOR EACH`.
- [conversions.bn](conversions.bn): Explicit type conversions using `AS`, string concatenation, and numeric truncation semantics.
- [lexical.bn](lexical.bn): Lexical syntax specification validation covering identifiers, comments, escape sequences, and numerical literals.
- [type_test.bn](type_test.bn): Dynamic and static type inspection, union types (`OR`), and runtime type queries using `IS`.
- [global.bn](global.bn): Module-level global declarations and scope lifetime.
- [objects.bn](objects.bn): Object-oriented programming covering class declarations, `PROTECTED` fields, inheritance (`EXTENDS`), method overriding (`OVERRIDE`), and polymorphic dispatch.
- [string_class.bn](string_class.bn): Object-oriented String wrapper demonstrating encapsulation and string transformations.
- [language-tour.bn](language-tour.bn): Broad language tour covering structs, classes, interfaces, dynamic memory, unions, and host capabilities.

## Modular Architecture & Multi-File Programs

- [two_files.bn](two_files.bn): Main entry point demonstrating multi-file program architecture with relative imports.
- [other.bn](other.bn): Intermediate library module imported by `two_files.bn`, re-exporting operations and importing `another.bn`.
- [another.bn](another.bn): Terminal module in the multi-file import chain demonstrating `EXPORT CLASS` and static methods.
- [import-qualified.bn](import-qualified.bn): Namespace qualification and alias scoping with `IMPORT ... AS ...`.

## Standard Library Tours

- [bnmath_tour.bn](bnmath_tour.bn): Complete validation suite for `BNMath` APIs including trigonometry, logarithms, limits, rounding, and descriptive statistics.
- [math_expressions.bn](math_expressions.bn): Numerical evaluation using compound `BNMath` operators and precedence rules.
- [bncrypto_tour.bn](bncrypto_tour.bn): Comprehensive tour of cryptographic primitives in `BNCrypto` (SHA-256, SHA-512, AES-GCM, HMAC, UUIDv4/v7, and random generation).
- [bndata_tour.bn](bndata_tour.bn): Data processing with `BNData.DataFrame`, covering column management, selections, joins, and CSV I/O.
- [bnlog_tour.bn](bnlog_tour.bn): Structured logging tour using `BNLog` across multiple output formats, log levels, and destinations.
- [bnstring_tour.bn](bnstring_tour.bn): String manipulation methods provided by the standard `BNString` library module.
- [filesystem_tour.bn](filesystem_tour.bn): File system I/O via `HOST.FileSystem` covering open modes, reading, writing, seeking, and path handling.
- [clock.bn](clock.bn): High-resolution timestamps and monotonic execution benchmarking using `HOST.Clock`.
- [sqlite.bn](sqlite.bn): Relational database operations with `BNSqlite`, including table creation, parameterized queries, and row iteration.

## Data Structures & Algorithms

- [linear_collections.bn](linear_collections.bn): Stack, Queue, and Circular Deque data structures backed by heap-allocated pointer regions.
- [shortest_path.bn](shortest_path.bn): Directed graph traversal and shortest path computation using 2D matrix representations.
- [graph_generator.bn](graph_generator.bn): Reusable graph generation module sampling target edge counts from a normal distribution via Box-Muller transform.
- [roulette_wheel.bn](roulette_wheel.bn): Virtual Roulette Wheel selection module implementing fitness-proportionate selection for minimization problems.
- [tsp_ga.bn](tsp_ga.bn): Evolutionary Traveling Salesperson Problem (TSP) solver combining stochastic graph generation, roulette selection, OX1 crossover, and 2-opt mutation.
- [factorial.bn](factorial.bn): Mathematical recursion and iteration comparing factorial execution paths.
- [rpn-calculator.bn](rpn-calculator.bn): Reverse Polish Notation stack-based arithmetic expression evaluator.
- [huffman.bn](huffman.bn): Huffman coding algorithm for lossless data compression and frequency prefix tree construction.
- [boyer-moore.bn](boyer-moore.bn): Substring search algorithm using the Boyer-Moore bad-character heuristic.
- [kmp.bn](kmp.bn): Knuth-Morris-Pratt (KMP) linear-time pattern matching with prefix function preprocessing.
- [rabin-karp.bn](rabin-karp.bn): Substring search using rolling polynomial hash signatures.
- [naive_search.bn](naive_search.bn): Brute-force baseline substring matching algorithm.
- [levenshtein.bn](levenshtein.bn): Interactive command-line utility for Levenshtein edit distance computation.
- [edit_distance.bn](edit_distance.bn): Dynamic programming implementation of Levenshtein string metric using heap pointer buffers.

## Concurrency, Parallelism & Simulations

- [parallel_pi.bn](parallel_pi.bn): Multi-threaded Monte Carlo / numerical estimation of $\pi$ distributing compute across workers.
- [parallel_work.bn](parallel_work.bn): Data-parallel batch task processing across CPU cores.
- [parallel-examples.md](parallel-examples.md): Architecture documentation detailing concurrency models, scheduling, and task parallelism.
- [dispatch_cellular_automaton.bn](dispatch_cellular_automaton.bn): Parallel cellular automaton simulation utilizing asynchronous task dispatching.
- [dispatch_game_tournament.bn](dispatch_game_tournament.bn): Concurrent player tournament scheduling and state management via task dispatchers.
- [dispatch_reliability_simulation.bn](dispatch_reliability_simulation.bn): System resilience testing and fault injection under concurrent worker load.
- [dispatch_net_echo.bn](dispatch_net_echo.bn): Asynchronous network echo server using task dispatching.

## Networking, Web & Systems

- [net.bn](net.bn): Low-level TCP network socket communication.
- [socket.bn](socket.bn): TCP socket client and server lifecycle management, timeouts, and buffer transfers.
- [socket-examples.md](socket-examples.md): Reference manual and architectural documentation for network socket programming.
- [icmp-ping.bn](icmp-ping.bn): Raw ICMP echo packet crafting and round-trip ping latency analysis.
- [exec-demo.bn](exec-demo.bn): Subprocess execution, command invocation, and standard stream piping via host interfaces.
- [ffi.bn](ffi.bn): Foreign Function Interface declarations binding host dynamic libraries (C ABI) into Basic Next.
- [test_ffi.bn](test_ffi.bn): Test harness verifying FFI scalar marshaling, string conversions, and pointer boundaries.
- [send-to-kindle.bn](send-to-kindle.bn): Document preparation and delivery automation script.
- [bnweb/users_api.bn](bnweb/users_api.bn): RESTful HTTP API service managing user entities with JSON payloads.
- [bnweb/https_private_ca.bn](bnweb/https_private_ca.bn): Secure HTTPS client handling custom Certificate Authority verification.

## Serialization & Utilities

- [serialization/Bird.bn](serialization/Bird.bn): Clean domain model class decoupled from serialization logic.
- [serialization/BirdJson.bn](serialization/BirdJson.bn): Companion JSON codec performing structured DOM encoding and decoding for the domain model.
- [serialization/main.bn](serialization/main.bn): End-to-end execution of companion JSON serialization and deserialization round-trips.
- [book_packages.bn](book_packages.bn): Full document processing pipeline that parses Markdown book chapters and packages an EPUB 3 digital publication with BNCompress.

## Object-Oriented Design Patterns

Directory: `patterns/` (detailed in [patterns/README.md](patterns/README.md))

### Creational Patterns
- [patterns/abstract_factory.bn](patterns/abstract_factory.bn): Families of related objects created without specifying concrete classes.
- [patterns/builder.bn](patterns/builder.bn): Step-by-step complex object construction separating representation from assembly.
- [patterns/factory_method.bn](patterns/factory_method.bn): Subclasses decide which concrete class to instantiate via factory methods.
- [patterns/prototype.bn](patterns/prototype.bn): Object duplication semantics (specifying cloning boundaries).
- [patterns/singleton.bn](patterns/singleton.bn): Process-wide single instance access control via private constructor and static accessor.

### Structural Patterns
- [patterns/adapter.bn](patterns/adapter.bn): Interface translation enabling incompatible classes to cooperate.
- [patterns/bridge.bn](patterns/bridge.bn): Decoupling abstraction from implementation hierarchy.
- [patterns/composite.bn](patterns/composite.bn): Tree structures representing part-whole object hierarchies uniformly.
- [patterns/decorator.bn](patterns/decorator.bn): Dynamic behavior attachment wrapping target objects transparently.
- [patterns/facade.bn](patterns/facade.bn): Simplified high-level interface over a complex subsystem.
- [patterns/flyweight.bn](patterns/flyweight.bn): Fine-grained resource sharing to support massive object counts efficiently.
- [patterns/proxy.bn](patterns/proxy.bn): Surrogate placeholder controlling access, lazy loading, or authorization to an underlying object.

### Behavioral Patterns
- [patterns/chain_of_responsibility.bn](patterns/chain_of_responsibility.bn): Passing requests along an ordered handler chain until handled.
- [patterns/command.bn](patterns/command.bn): Encapsulating requests as objects, enabling queues, logging, and undo operations.
- [patterns/interpreter.bn](patterns/interpreter.bn): Sentence evaluation against a defined domain grammar.
- [patterns/iterator.bn](patterns/iterator.bn): Sequential traversal of collection elements without exposing underlying storage.
- [patterns/mediator.bn](patterns/mediator.bn): Centralized object communication hub preventing direct interdependent references.
- [patterns/memento.bn](patterns/memento.bn): State capture and restoration without violating internal encapsulation.
- [patterns/observer.bn](patterns/observer.bn): Publish-subscribe event notification across decoupled listener objects.
- [patterns/state.bn](patterns/state.bn): Dynamic object behavior alteration governed by state transitions.
- [patterns/strategy.bn](patterns/strategy.bn): Family of interchangeable algorithms selectable at runtime.
- [patterns/template_method.bn](patterns/template_method.bn): Fixed algorithmic skeleton deferring individual steps to subclasses.
- [patterns/visitor.bn](patterns/visitor.bn): Operation definition on object structures (detailing double-dispatch constraints).

### GRASP Principles
- [patterns/grasp_controller.bn](patterns/grasp_controller.bn): First non-UI coordinator receiving and handling system events.
- [patterns/grasp_creator.bn](patterns/grasp_creator.bn): Allocating object creation responsibility to aggregating or containing classes.
- [patterns/grasp_high_cohesion.bn](patterns/grasp_high_cohesion.bn): Designing focused classes with tightly related responsibilities.
- [patterns/grasp_indirection.bn](patterns/grasp_indirection.bn): Mediating intermediate objects to maintain low coupling between components.
- [patterns/grasp_information_expert.bn](patterns/grasp_information_expert.bn): Assigning responsibility to the class possessing the necessary information.
- [patterns/grasp_low_coupling.bn](patterns/grasp_low_coupling.bn): Structuring dependencies to minimize ripple effects of changes.
- [patterns/grasp_polymorphism.bn](patterns/grasp_polymorphism.bn): Handling behavioral variations through polymorphic method dispatch.
- [patterns/grasp_protected_variations.bn](patterns/grasp_protected_variations.bn): Encapsulating instability concepts behind stable interfaces.
- [patterns/grasp_pure_fabrication.bn](patterns/grasp_pure_fabrication.bn): Artificial helper classes maintaining high cohesion without domain representation.
