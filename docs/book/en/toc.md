# The Basic Next Programming Language

- [Preface — Reclaiming the Joy of Programming](00_preface.md)

## 1. [Introduction](01_introduction.md)
- What is Basic Next?
- Installation and the `bn` CLI (`bn run`, `bn check`, `bn build`, `bn lex`, `bn eval`)
- Hello, World!
- Modules and the `Start` Function
- Ecosystem Tools (Jupyter, VS Code)

## 2. [Common Programming Concepts](02_common_programming_concepts.md)
- Variables and Constants (`LET`, `CONST`)
- Type Inspection (`TYPEOF`)
- Primitive Types (Integers, Floats, Boolean, String, Temporal)
- Operators and Expressions
- Explicit Type Conversion (`AS`)
- Basic Console I/O (`PRINT`, `INPUT`)

## 3. [Control Flow](03_control_flow.md)
- Conditional Branching (`IF`, `ELSE`, single-line `IF`)
- Pre-condition and Post-condition Loops (`WHILE`, `REPEAT ... UNTIL`)
- Counted and Collection Iteration (`FOR`, `FOR EACH`)
- Loop Control and Termination (`EXIT`, `CONTINUE`, `STOP`)

## 4. [Compound Data and Error Handling](04_compound_data.md)
- Fixed-Size Vectors
- Value Types (`STRUCT`)
- String Indexing
- Alternative Types and Absence (`OR`, `IS`, `NULL`, `NA`, `EOF`)
- Error Values (The `Error` object)

## 5. [Functions and Program Structure](05_functions_and_program_structure.md)
- Function Declarations and Return Analysis
- Function Values
- Modules, Namespaces, and the `BN` Root
- Visibility (`EXPORT`, `IMPORT`)

## 6. [Object-Oriented Features](06_object_oriented_features.md)
- Reference Types (`CLASS`)
- Visibility (`PRIVATE`, `PUBLIC`) and `STATIC` Members
- Constructors and Destructors
- Inheritance
- Contracts (`INTERFACE` and `IMPLEMENTS`)

## 7. [Memory Management](07_memory_management.md)
- Memory Management (ARC, `RELEASE`, pointers)
- Pointers (`POINTER TO TYPE`, array pointers)
- Memory Safety and Runtime Errors

## 8. [Standard Library and HOST](08_standard_library_and_host.md)
- The external module boundary
- HOST capabilities (`HOST.Args`, `HOST.Clock`, `HOST.Console`, `HOST.Random`, `HOST.FileSystem`, `HOST.Exec`)
- Temporal Data (`TIMESTAMP`, `DATE`, `TIME`, `TIMEZONE`)
- Built-ins (`LEN`, `SIZEOF`)

## 9. [I/O and Concurrency](09_io_and_concurrency.md)
- Synchronous, Bounded I/O (`HOST.FileSystem`, `HOST.Net`, `BNWeb`)
- Concurrency and Parallelism (`BNDispatch`)
- Constraints and Resource Management

## 10. [Architecture and Execution Policy](17_architecture.md)
- Unrestricted and sandboxed filesystem profiles
- Artifact ceilings and execution-time restrictions
- Runtime authorization and target support
- Sandboxed path handling and policy precedence

## [Appendices](10_appendices.md)
- Appendix A: Keywords Reference
- Appendix B: Language Diagnostics
- Appendix C: Accepted Syntax — see the normative [0.5 EBNF](../../language/0.5/0.5.ebnf)
- Appendix D: The `bn` Tool (`bn(1)`)
- Appendix E: [`BNJson`](11_bnjson.md)
- Appendix F: [`BNLog`](12_bnlog.md)
- Appendix G: [`BNWeb`](13_bnweb.md)
- Appendix H: [`BNData`](14_bndata.md)
- Appendix I: [External module conventions](15_external_modules.md)
- Appendix J: [`BNDispatch`](16_bndispatch.md)
