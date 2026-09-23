# The Basic Next Programming Language

## [Preface](00_preface.md)
- The problem the language addresses
- The design decisions and what each one costs
- What was borrowed, and what the language does not have
- How this book is organized

## 1. [Introduction](01_introduction.md)
- What Basic Next is, and who it is for
- Design principles
- Installing the toolchain
- The `bn` commands (`check`, `run`, `build`, `eval`, `lex`, `lsp`, `dap`)
- A first program, interpreted and compiled
- Modules and the entry point (`Start`, `EXPORT`, `IMPORT`, `STATIC` state)
- Reading a diagnostic
- Editor and notebook integration

## 2. [Common Programming Concepts](02_common_programming_concepts.md)
- Comments
- Variables (`LET`), defaults, and alternative-typed bindings
- Constants (`CONST`) and literal inference
- Type inspection (`TYPEOF`)
- Primitive types: numeric widths and overflow, `BOOLEAN`, `STRING`, temporal
- Operators: arithmetic, the three divisions, comparison, logical and bitwise
- Explicit conversion (`AS`) and numeric limits (`BNMath`)
- Console input and output (`PRINT`, `INPUT`, `HOST.Console`)
- Choosing types deliberately

## 3. [Control Flow](03_control_flow.md)
- Boolean-only conditions
- Block `IF`, chained alternatives, and the single-line form
- Guard clauses instead of nesting
- Narrowing an alternative type (`IS`)
- Pre-condition and post-condition loops (`WHILE`, `REPEAT ... UNTIL`)
- Counted and collection iteration (`FOR`, `FOR EACH`)
- Leaving and skipping iterations (`EXIT`, `CONTINUE`)
- Loops and return analysis
- Halting the program (`STOP`) and a worked example

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
- Appendix C: Accepted Syntax — see the normative [0.6 EBNF](../../../language/0.6/0.6.ebnf)
- Appendix D: The `bn` Tool (`bn(1)`)
- Appendix E: [`BNJson`](11_bnjson.md)
- Appendix F: [`BNLog`](12_bnlog.md)
- Appendix G: [`BNWeb`](13_bnweb.md)
- Appendix H: [`BNData`](14_bndata.md)
- Appendix I: [External module conventions](15_external_modules.md)
- Appendix J: [`BNDispatch`](16_bndispatch.md)
- Appendix K: [`BNCrypto`](18_bncrypto.md)

---

[Start reading: Preface →](00_preface.md)
