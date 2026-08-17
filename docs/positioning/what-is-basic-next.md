# What Is Basic Next?

## A general-purpose language for clear software

Basic Next (BN) is a general-purpose, object-oriented programming language
designed to reduce cognitive load and turn ideas into clear, cross-platform
software. It combines BASIC-inspired readability with explicit types, cohesive
objects, and capability-based access to the host environment.

BN is not a nostalgia project and is not a framework. Its goal is a small
language core that keeps everyday code readable while leaving room for serious
software: command-line tools, data work, applications, integrations, and
future device or web targets.

## The design in one example

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

The program begins in an explicit method, uses readable control flow, and does
not require a ceremony-heavy application class or a framework to do simple
work.

## What BN values

- **Readable source:** code is communication before it is instruction.
- **Explicit contracts:** types, visibility, imports, and host capabilities are
  visible in the source.
- **Object-oriented structure:** behavior lives in cohesive classes; public and
  private boundaries are deliberate.
- **Small core:** language keywords solve fundamental problems; libraries and
  host capabilities carry the rest.
- **Cross-platform reach:** software targets `HOST` capabilities rather than a
  particular vendor API.

## Current status

BN is pre-implementation. The Basic Next 0.1 specification is being written
before the interpreter, runtime, and package workflow are built. This is a
deliberate choice: the project will validate semantics through examples and
proposals before treating them as implementation commitments.

## The promise

Basic Next aims to make the common case calm and understandable, while keeping
explicit mechanisms available when software needs structure, types, memory, or
host integration.
