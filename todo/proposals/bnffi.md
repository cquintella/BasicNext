# Proposal: BNFfi — C foreign function calls as a standard module

## Status

**Accepted 2026-10-06 (Carlos).** Planned as
[`todo/bucket-0.6.7.md`](../bucket-0.6.7.md) (not started); nothing here is
implemented. The bucket was 0.6.6 until 2026-10-14, when Carlos moved the
test-system bucket ahead of it; decisions cited as "bucket 0.6.7 D…" were
recorded under the old number.

This proposal supersedes [`c-ffi.md`](c-ffi.md) and the AQ-22 decision
recorded in
[`native-stdlib-binding.md`](../../docs/architecture/native-stdlib-binding.md)
(one `EXTERN` mechanism for stdlib native bindings and foreign C).

## Constraint

**Basic Next does not change.** No grammar production, keyword, semantic
rule, HOST capability, or ARC behaviour is added or altered. C calls are
reached only through an ordinary standard module, `BNFfi`, written in the
form of `modules/bn/BNSqlite.bn` (exported classes whose behaviour the
native runtime supplies). Every construct a program uses is already valid
0.6; see [Language conformance](#language-conformance).

## Example

```basic
IMPORT BNFfi AS C

FUNCTION Start() AS INTEGER
    LET args AS C.Args = NEW C.Args()
    args.Add(NEW C.Double(10.0))

    LET r AS C.Double = NEW C.Double(0.0)
    LET ok AS VOID OR Error = C.Call("m", "cos", args, r)
    IF ok IS Error THEN
        PRINT ok.Message
        RETURN 1
    END IF
    PRINT r.Value()
    RETURN 0
END FUNCTION
```

The C types of the arguments come from the argument objects; the C return
type comes from the class of the result object. `args` is the argument
vector and `args.Count()` its count.

## Surface

### C value types

Every class below implements the exported interface `C.Value`, a marker
interface with no members (the runtime reads the class tag; no method is
dispatched through `C.Value`). The names follow C, not Basic Next, so the
boundary stays visible in source. The class list is under review in bucket
0.6.7 (D10, D11).

| Class | C type | Constructor / `Value()` result |
| --- | --- | --- |
| `C.SChar`, `C.UChar` | `signed char`, `unsigned char` | `INT8`, `BYTE` |
| `C.Short`, `C.UShort` | `short`, `unsigned short` | `INT16`, `UINT16` |
| `C.Int`, `C.UInt` | `int`, `unsigned int` | `INT32`, `UINT32` |
| `C.Long`, `C.ULong` | `long`, `unsigned long` | `INT64`, `UINT64` |
| `C.LongLong`, `C.ULongLong` | `long long`, `unsigned long long` | `INT64`, `UINT64` |
| `C.SizeT` | `size_t` | `UINT64` |
| `C.Int8` … `C.UInt64` | `int8_t` … `uint64_t` | the BN integer of the same width |
| `C.Float`, `C.Double` | `float`, `double` | `FLOAT32`, `FLOAT64` |
| `C.Bool` | `_Bool` | `BOOLEAN` |
| `C.String` | `const char *` (argument only) | built from `STRING` |
| `C.Handle` | a non-null pointer owned by C (argument) | opaque; no public constructor |
| `C.HandleResult` | a pointer returned by C (result only) | `Value() AS C.Handle OR NULL`; C `NULL` is BN `NULL` |
| `C.Void` | `void` (result only) | no `Value()` |

- `C.Long`, `C.ULong` and `C.SizeT` hold the widest BN type. When the target
  C type is narrower (`long` on Windows), a value out of range fails
  `C.Call` with `Error`; it is never truncated.
- Plain `char` is excluded: its signedness is platform-defined.
- Values are read with the accessor method `Value()`, not a field (Carlos,
  2026-10-07): native standard modules support methods only (bucket 0.6.7
  D6).

### Calls

| Member | Meaning |
| --- | --- |
| `NEW C.Args()` | An empty argument vector. |
| `args.Add(v AS C.Value) AS VOID` | Appends one argument, in call order. |
| `args.Count() AS INTEGER` | The argument count. |
| `C.Call(library AS STRING, symbol AS STRING, args AS C.Args, result AS C.Value) AS VOID OR Error` | Resolves `symbol` in `library`, calls it with the C types of `args`, and writes the C return value into `result`. |

`C.Call` checks, before calling, that the library is found, the
symbol exists, and every argument and the result hold a representable value.
Each failure returns `Error`; nothing is called. `C.Call` cannot check the
declaration against the binary: a C function called with the wrong types is
an integration error no runtime can detect. A wrapper class written in BN
(see [Wrapper pattern](#wrapper-pattern)) keeps each declaration in one place.

### Memory

| Situation | Tool |
| --- | --- |
| BN owns the memory; C reads or writes it during the call | `NEW C.DoubleArray(region)` (and one class per numeric element type), passed as a `T *` argument |
| C owns the memory; BN reads it | `array.ReadFrom(h AS C.Handle) AS VOID OR Error` |
| C owns the memory; BN writes it | `array.WriteTo(h AS C.Handle) AS VOID OR Error` |
| C text to BN | `h.ReadString() AS STRING OR Error` (reads to NUL, validates UTF-8) |
| C returned `NULL` | `r.Value() IS NULL` on the `C.HandleResult` |

- An array class holds an ordinary strong reference to its region; the
  region stays alive while the array object exists. The C side must not keep
  the pointer after the call returns.
- Copies move `LEN(region)` elements, so the BN side never writes outside
  its region. Whether the C area is that large is the C API's contract.
- **A handle is born only from C** (Carlos, 2026-10-06). `C.Handle` has a
  `PRIVATE` constructor, so `NEW C.Handle()` is `PRIVATE_ACCESS`; the runtime
  creates one only from a non-null pointer a C function returned, and stores
  it in the `C.HandleResult` (read with `Value()`). A program cannot forge an address and cannot
  hold a null `C.Handle`, so `ReadFrom`, `WriteTo` and `ReadString` never see
  a null pointer. Passing C `NULL` as an argument is out of the first cut.

  ```basic
  LET r AS C.HandleResult = NEW C.HandleResult()
  LET ok AS VOID OR Error = C.Call("libc.so.6", "fopen", args, r)
  IF r.Value() IS NULL THEN
      PRINT "fopen failed"
  ELSE
      LET f AS C.Handle = r.Value()
  END IF
  ```

- `C.Handle` is an ordinary class instance. ARC manages the BN object only and
  never frees C memory. C memory is released explicitly, by calling the C
  library's own release function through `C.Call` (`fclose`, `sqlite3_close`,
  `free`). Leaks, use after release, and double release on the C side are not
  detected.
- No `C.LongArray` or `C.SizeTArray` in the first cut: their element width
  differs from the BN region's on some targets, so they cannot be borrowed.

## Library resolution

- A `library` containing `/` or `\` is a path, used as given.
- Otherwise it is a logical name. A name containing `.` is used as the file
  name (`"libm.so.6"`); any other name takes the platform pattern
  (`lib<name>.so`, `lib<name>.dylib`, `<name>.dll`). The runtime tries: each
  `--ffi-path` directory (`bni run` only; repeatable, ordered); each
  directory of `BN_LIBRARY_PATH` (platform `PATH` separator; empty entries
  ignored, never the current directory), read through `HOST.Env`
  ([`host-env.md`](host-env.md)) at run time and never recorded in a compiled
  binary; then the system loader's default search (on Windows without the
  current directory and `PATH`).
- A library is loaded once and never unloaded during the process.
- Loading a library runs its initialisers. Only `C.Call` at run time loads
  one; `bni check`, `bni lsp` and `bnc` never do.
- Not found: `Error` naming every location tried.

System library names are not portable: on Linux with glibc, `libm.so` and
`libc.so` are linker scripts, so the name is `"libm.so.6"`; on macOS `"m"`;
on Windows the C math functions are in `"ucrtbase"`.

## Errors

`BNFfi` publishes its codes as `INTEGER` constants on the module (`error.md`).
`Operation` is the canonical name (`BNFfi.Call`,
`BNFfi.DoubleArray.ReadFrom`, `BNFfi.Handle.ReadString`).

| Constant | Value | Raised by | Condition |
| --- | ---: | --- | --- |
| `C.INVALID_ARGUMENT` | 1 | `Call` | `library` or `symbol` is empty or contains NUL |
| `C.LIBRARY_NOT_FOUND` | 2 | `Call` | No search location holds the library; `Message` names every location tried |
| `C.LIBRARY_LOAD_FAILED` | 3 | `Call` | The library was found but the loader refused it (wrong architecture, missing dependency); the loader's text goes in `Cause` |
| `C.SYMBOL_NOT_FOUND` | 4 | `Call` | The library does not export `symbol` |
| `C.INVALID_SIGNATURE` | 5 | `Call` | A value cannot appear where it is used: `C.Void` or `C.HandleResult` as an argument; `C.String`, `C.Handle` or an array class as the result |
| `C.VALUE_OUT_OF_RANGE` | 6 | `Call` | A value does not fit the target's C type (`C.Long` above 2³¹−1 on Windows, `C.SizeT` on a narrower target) |
| `C.INVALID_STRING` | 7 | `Call` | A `C.String` contains NUL, so it has no C representation |
| `C.INVALID_UTF8` | 8 | `ReadString` | The C text is not valid UTF-8 |

No code exists for a call whose declared C types differ from the real
function: that mismatch cannot be detected. An unsupported target is a
support diagnostic, not an `Error`.

## Policy and targets

- **No policy** (Carlos, 2026-10-06, bucket D3): `BNFfi` is always
  enabled. No environment variable, flag or profile denies `C.Call`.
- **`--sandbox` does not confine `BNFfi`.** C code runs with the process's
  full rights and ignores sandbox roots and `BN_FS_POLICY`. The module
  contract, `host-traits.md` and the book state this; a program that must be
  confined does not import `BNFfi`.
- Native `bni` and native `bnc` only. `IMPORT BNFfi` on `wasm32` fails a
  **support** diagnostic (no native code and no `dlopen` in Wasm).

## Wrapper pattern

Static typing is recovered with an ordinary BN class, written once per
library:

```basic
IMPORT BNFfi AS C

CLASS LibM
    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    PUBLIC FUNCTION Cos(theta AS FLOAT64) AS FLOAT64 OR Error
        LET args AS C.Args = NEW C.Args()
        args.Add(NEW C.Double(theta))
        LET r AS C.Double = NEW C.Double(0.0)
        LET ok AS VOID OR Error = C.Call("m", "cos", args, r)
        IF ok IS Error THEN
            RETURN ok
        END IF
        RETURN r.Value()
    END FUNCTION
END CLASS
```

## Language conformance

Checked on 2026-10-06 against `language/0.6/` and `bni 0.6.4`, with a
scratch module that declares the surface above with stub bodies
(`bni check` runs lexical, syntax, semantic analysis, lowering and IR
validation). The program passes once the defect in finding 1 is worked
around.

| Construct | Basis |
| --- | --- |
| `IMPORT BNFfi AS C` | `import-declaration`, `module-import-path` |
| `EXPORT INTERFACE Value`, `EXPORT CLASS Double IMPLEMENTS Value` | `declaration`, `class-declaration` |
| `NEW C.Double(10.0)` | `new-expression` with a qualified `named-type` |
| `C.Long(-7)` with an `INT64` parameter | implicit integer widening |
| `C.String`, `C.Void`, `C.Long` | identifiers; keywords are upper case |
| class object to `C.Value` | implicit interface upcast (`0.6.md` § interfaces) |
| `result` written by `C.Call` | class instances are references |
| `NEW C.DoubleArray(v)` with `v AS POINTER TO FLOAT64[]` | ordinary parameter of that exact type |

## Findings

1. **Defect (toolchain, predates current work).** Passing a class object
   directly to an `INTERFACE`-typed parameter fails IR validation on both
   backends (`INVALID_IR: call arguments or result do not match the callee
   signature`); binding it first (`LET s AS Shape = d`) works. Reproduced on
   a clean worktree of `HEAD` (`da51b5b`, 0.6.4). The specification requires
   the implicit upcast, so the lowering is wrong. Fixing it is a
   prerequisite: `args.Add(NEW C.Double(…))` and `C.Call(…, r)` both pass a
   class object to an interface parameter.

   ```basic
   INTERFACE Shape
       FUNCTION Name() AS STRING
   END INTERFACE
   CLASS Dot IMPLEMENTS Shape
       PUBLIC FUNCTION CONSTRUCTOR()
       END FUNCTION
       PUBLIC FUNCTION Name() AS STRING
           RETURN "dot"
       END FUNCTION
   END CLASS
   FUNCTION One(s AS Shape) AS STRING
       RETURN s.Name()
   END FUNCTION
   FUNCTION Start() AS INTEGER
       PRINT One(NEW Dot())    // INVALID_IR
       RETURN 0
   END FUNCTION
   ```

2. **Normative gap.** `0.6.md` (opaque pointers) says a typed pointer
   converts implicitly to `POINTER TO VOID`, then qualifies the reverse with
   "when their pointer region lengths are compatible". `bni` accepts
   `POINTER TO T` → `POINTER TO VOID` and rejects `POINTER TO T[]` →
   `POINTER TO VOID` (`POINTER TO VOID[]` is a syntax error). The text does not
   say which is intended. This proposal does not depend on it: it uses one
   array class per element type.
3. **Library vector rule.** The rule that lets a library parameter accept a
   fixed vector of any length (`0.6.md` § Library vector parameters) covers
   `INTEGER`, `FLOAT`, `STRING` and `BOOLEAN` elements only. A
   `C.Value[]` parameter would extend it, which is a language change; hence
   the `C.Args` object.
4. **Diagnostic text.** The frontend grants unsized vector parameters to
   every standard module (`module.standard_module.is_some()`), but its
   message says "reserved for the BNData library".
5. **Native backend support.** `bnc` rejects calls to interface methods
   (`TARGET_UNSUPPORTED_OP`). User code calling `C.Value` methods compiles
   only once that support lands; `C.Call` itself is a runtime call.

## Implementation prerequisites (toolchain, not language)

- Fix finding 1 at the producer, with a parity fixture.
- A `StandardModule::BNFfi` entry and `modules/bn/BNFfi.bn`.
- `libffi`/`libffi-sys` and `libloading` in `bni` and in `bn_rt`, reviewed
  against the forbidden-dependency gate (approved 2026-10-06).
- A narrow `unsafe` use in `crates/bn_rt/src/ffi.rs` only (approved
  2026-10-06, recorded in `AGENTS.md`).
- `language/0.6/bnffi.md` as the module contract (as
  `bnsqlite.md`), `man/bni.1`, `man/bnc.1`, and the support matrix.

## Out of the first cut

Structures by value, callbacks and function pointers, variadic functions
(`printf`), returned `char *` other than through `ReadString`, C `NULL` as an argument, `long` and
`size_t` arrays, reading C `errno`, static libraries, Wasm. Exporting BN
functions to C is a separate proposal, [`c-export.md`](c-export.md).

## Open questions

- Decided (technical, 2026-10-06): in a compiled program `bn_rt` identifies
  the C type of a `C.Value` object by a fixed numeric tag per class stored in
  the object, not by its class name.
