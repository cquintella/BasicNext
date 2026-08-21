# Proposal: C Foreign Function Interface

## Status

Proposed for post-0.1. This document does not add syntax, keywords, or runtime
behavior to Basic Next 0.1.

## Motivation

BN needs a controlled path to established native libraries: operating-system
APIs, numerical libraries, device runtimes, and small project-specific C
adapters. C is the first foreign boundary because its ABI is the common
interoperability layer for these ecosystems.

The boundary must be visible in source, explicitly typed, and contained by a
`HOST` capability. It must not make raw process memory or undefined behavior a
normal BN programming model.

## Design principles

- C FFI is a host capability, not a language-wide escape hatch.
- A program imports it through the one normal capability form:
  `IMPORT HOST.c AS C`.
- Every foreign symbol has a declared BN signature and an explicit C symbol
  name. No dynamic string-based invocation exists in source code.
- The reference runtime owns ABI calls and validation. BN source has no
  `UNSAFE` block or equivalent keyword.
- The first profile favors a small C adapter around a complex native library
  over reproducing an entire C header in BN.
- FFI failures are ordinary `Error` values where they can be detected. A C
  function with an incorrect declaration remains an integration error that no
  runtime can make safe.

## Candidate source form

The following is illustrative syntax, not valid BN 0.1:

```basic
IMPORT HOST.c AS C

EXTERN C "math"
    FUNCTION Cos(value AS FLOAT64) AS FLOAT64 = "cos"
END EXTERN

FUNCTION Start() AS INTEGER
    LET value AS FLOAT64 = Cos(0.0)
    PRINT value
    RETURN 0
END FUNCTION
```

`C` is the imported capability alias. `"math"` is a logical library name,
resolved by the host profile or a future package manifest; source code does not
contain platform paths such as `.dylib`, `.so`, or `.dll`. The name after `=`
is the exact exported C symbol. Omitting it uses the BN function name unchanged.

`EXTERN` is the only proposed new keyword. It creates declarations, not a
general executable block. `END EXTERN` follows BN's explicit block rule.

## Candidate grammar impact

If accepted, the grammar would add the following production family:

```ebnf
declaration          = ... | c-extern-declaration ;
c-extern-declaration = "EXTERN" identifier string-literal NEWLINE
                       { c-function-declaration }
                       "END" "EXTERN" NEWLINE ;
c-function-declaration = "FUNCTION" identifier "(" [ parameters ] ")" "AS"
                         return-type [ "=" string-literal ] NEWLINE ;
```

Semantic validation requires `identifier` after `EXTERN` to name an imported
C capability. It also rejects duplicate BN names and unsupported FFI types.

## Profile 1: supported calls

The first profile supports only non-variadic C functions using the target's C
ABI. It supports parameters and returns of these exact-width values:

| BN type | Required C counterpart |
| --- | --- |
| `BYTE` | `uint8_t` |
| `INT16` | `int16_t` |
| `INT32` / `INTEGER` | `int32_t` |
| `UINT32` | `uint32_t` |
| `FLOAT32` | `float` |
| `FLOAT64` / `FLOAT` | `double` |
| `VOID` | `void` |

`BOOLEAN`, `STRING`, unsized C types (`int`, `long`, `size_t`, `char`), C
enums, bit-fields, and all BN class or interface references are not valid in a
Profile 1 foreign signature. This avoids pretending that their layout or
ownership is portable.

A `POINTER TO` an allowed fixed-width type may be passed or returned. The C
header must document whether it is nullable, borrowed, writable, or owned.
An owned pointer must have a matching imported C release function; BN never
applies `DELETE` to memory allocated by C. Conversely, C must never free memory
allocated by BN unless an explicit future transfer contract says otherwise.

## Deliberately excluded from Profile 1

- C variadic functions such as `printf`.
- C callbacks and C function pointers.
- C structures or unions passed by value.
- Automatic conversion between BN `STRING` and `char *`.
- C++ APIs, exceptions, `longjmp`, or unwinding across the FFI boundary.
- Arbitrary library paths, implicit dynamic loading, and platform-specific
  calling-convention annotations in BN source.

Use a small C adapter library when an API needs any excluded form. For example,
an adapter can replace a variadic API with a fixed signature, convert a C
structure to fixed-width fields, or make ownership explicit through `Create`
and `Destroy` functions.

## Runtime and diagnostics

The `HOST.c` provider resolves the logical library and its symbols before a
foreign function can be called. It reports a source-spanned `Error` for:

- unavailable `HOST.c` capability;
- unknown logical library;
- unresolved exported symbol;
- unsupported declaration type;
- unsupported target ABI; or
- duplicate foreign declaration name.

The runtime uses the platform's C ABI, never a Rust ABI. It must prevent Rust
panic unwinding from crossing the boundary and report host failures without
inventing a BN exception mechanism.

## Open questions

1. `INT64` and `UINT64` are Basic Next 0.1 primitive types. Should a future C
   profile also introduce explicit `C.SIZE` and `C.SSIZE` types for `size_t` and
   `ptrdiff_t`?
2. What manifest format maps a logical library name to package, static, or
   dynamic linkage on each host?
3. Should a later `C STRUCT` declaration guarantee a C-compatible field layout,
   or should C adapters remain the only structure boundary?
4. What explicit ownership notation is sufficient for returned pointers without
   adding an `UNSAFE` model to BN?

## Adoption path

1. Finish BN 0.1 syntax and semantic analysis.
2. Define the `HOST.c` capability and logical-library resolution in a host
   profile.
3. Accept `EXTERN` only with Profile 1 types, diagnostics, and positive and
   negative conformance fixtures.
4. Add a small C adapter example and test it on macOS, Linux, and Windows.
5. Consider structures, callbacks, strings, and GPU runtime adapters only after
   the fixed-signature profile is reliable.

## References

- [Rust Reference: external blocks and ABI](https://doc.rust-lang.org/reference/items/external-blocks.html)
- [Rust Reference: application binary interface](https://doc.rust-lang.org/reference/abi.html)
- [Rust `core::ffi`: platform-specific C types](https://doc.rust-lang.org/stable/core/ffi/)
