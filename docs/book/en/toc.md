# The Basic Next Programming Language

## 1. Introduction
- What is Basic Next?
- Installation and the `bn` CLI (`bn run`, `bn check`, `bn build`, `bn lex`)
- Hello, World!
- Modules and the `Start` Function
- Ecosystem Tools (Jupyter, VS Code)

## 2. Common Programming Concepts
- Variables and Constants (`LET`, `CONST`)
- Primitive Types (Integers, Floats, Boolean, String, Temporal)
- Operators and Expressions
- Explicit Type Conversion (`AS`)
- Basic Console I/O (`PRINT`, `INPUT`)

## 3. Control Flow
- Conditional Branching (`IF`, `ELSE`)
- Pre-condition and Post-condition Loops (`WHILE`, `REPEAT ... UNTIL`)
- Counted and Collection Iteration (`FOR`, `FOR EACH`)
- Loop Control and Termination (`EXIT`, `CONTINUE`, `STOP`)

## 4. Compound Data and Error Handling
- Fixed-Size Vectors
- Value Types (`STRUCT`)
- String Indexing
- Alternative Types and Absence (`OR`, `IS`, `NULL`, `NA`, `EOF`)
- Error Values (The `Error` object)

## 5. Functions and Program Structure
- Function Declarations and Return Analysis
- Function Values
- Modules, Namespaces, and the `BN` Root
- Visibility (`EXPORT`, `IMPORT`)

## 6. Object-Oriented Features
- Reference Types (`CLASS`)
- Visibility (`PRIVATE`, `PUBLIC`) and `STATIC` Members
- Constructors and Destructors
- Inheritance
- Contracts (`INTERFACE` and `IMPLEMENTS`)

## 7. Memory Management
- Manual Allocation (`NEW`, `DELETE`)
- Pointers (`POINTER TO TYPE`, array pointers)
- Memory Safety and Runtime Errors

## 8. Standard Library and HOST
- The external module boundary
- HOST capabilities (`HOST.Args`, `HOST.Clock`, `HOST.Console`, `HOST.Random`, `HOST.FileSystem`)
- Temporal Data (`TIMESTAMP`, `DATE`, `TIME`, `TIMEZONE`)
- Built-ins (`LEN`, `SIZEOF`)

## 9. I/O and Concurrency
- Synchronous, Bounded I/O (`HOST.FileSystem`, `HOST.Net`, `BNWeb`)
- Concurrency and Parallelism (`BNDispatch`)
- Constraints and Resource Management

## Appendices
- Appendix A: Keywords Reference
- Appendix B: Language Diagnostics
- Appendix C: Accepted 0.3 Syntax (EBNF)
- Appendix D: The `bn` Tool (`bn(1)`)
- Appendix E: `BNJson`
- Appendix F: `BNLog`
- Appendix G: `BNWeb`
- Appendix H: `BNData`
- Appendix I: External module conventions
- Appendix J: [BNDispatch](16_bndispatch.md)
