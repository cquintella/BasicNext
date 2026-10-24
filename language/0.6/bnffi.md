# BNFfi Standard Library (0.6)

## Status

Accepted 0.6 standard module. Normative companion contract for foreign C function invocation.

```basic
IMPORT BNFfi AS C
```

`BNFfi` provides foreign function interface (FFI) capabilities enabling Basic Next programs to invoke precompiled C library functions with exact observable equivalence across `bni run` (interpreter) and native `bnc` (compiler), without any modification to the core language syntax, keywords, or grammar.

---

## 1. Surface Specification

### 1.1 Marker Interface `C.Value`

All C parameter and result wrapper classes implement the marker interface `C.Value`:

```basic
EXPORT INTERFACE Value
END INTERFACE
```

### 1.2 Scalar Value Classes

Each scalar class wraps a C primitive type. Every scalar class implements `C.Value`, provides a constructor taking the matching Basic Next primitive, and exposes an accessor method `Value()` returning that primitive:

| Class | C Primitive Type | Constructor Parameter | Accessor Return Type |
| --- | --- | --- | --- |
| `C.Int8` | `int8_t` | `v AS INT8` | `INT8` |
| `C.UInt8` | `uint8_t` | `v AS BYTE` | `BYTE` |
| `C.Int16` | `int16_t` | `v AS INT16` | `INT16` |
| `C.UInt16` | `uint16_t` | `v AS UINT16` | `UINT16` |
| `C.Int32` | `int32_t` | `v AS INTEGER` | `INTEGER` |
| `C.UInt32` | `uint32_t` | `v AS UINT32` | `UINT32` |
| `C.Int64` | `int64_t` | `v AS INT64` | `INT64` |
| `C.UInt64` | `uint64_t` | `v AS UINT64` | `UINT64` |
| `C.Int` | `int` | `v AS INTEGER` | `INTEGER` |
| `C.UInt` | `unsigned int` | `v AS UINT32` | `UINT32` |
| `C.Long` | `long` | `v AS INT64` | `INT64` |
| `C.ULong` | `unsigned long` | `v AS UINT64` | `UINT64` |
| `C.SizeT` | `size_t` | `v AS UINT64` | `UINT64` |
| `C.Float` | `float` | `v AS FLOAT32` | `FLOAT32` |
| `C.Double` | `double` | `v AS FLOAT` | `FLOAT` |
| `C.Bool` | `_Bool` | `v AS BOOLEAN` | `BOOLEAN` |

Target range checking: For platform-varying types (`C.Long`, `C.ULong`, `C.SizeT`), if the runtime value exceeds the bit-width of the target ABI (e.g. `long` on 64-bit Windows), `C.Call` fails with error code `C.VALUE_OUT_OF_RANGE`.

### 1.3 String & Void Classes

* `C.String`: Wraps a UTF-8 string passed as a C `const char *` argument.
  * Constructor: `CONSTRUCTOR(v AS STRING)`
  * Argument only: Passing `C.String` as a result target is invalid (`C.INVALID_SIGNATURE`).
  * If `v` contains an embedded NUL byte, `C.Call` fails with `C.INVALID_STRING`.
* `C.Void`: Marker for functions returning `void`.
  * Constructor: `CONSTRUCTOR()`
  * Result only: Passing `C.Void` as an argument is invalid (`C.INVALID_SIGNATURE`).

### 1.4 Handles and Pointers

Pointers returned from C are wrapped in opaque handles:

* `C.Handle`: Represents a non-null C memory pointer.
  * Constructor: `PRIVATE FUNCTION CONSTRUCTOR()` (cannot be instantiated by user code via `NEW`; instantiation attempts fail with `PRIVATE_ACCESS`).
  * `ReadString() AS STRING OR Error`: Reads NUL-terminated C string bytes and decodes UTF-8 into a Basic Next `STRING`. Returns `C.INVALID_UTF8` if bytes are not valid UTF-8.
* `C.HandleResult`: Container for C function calls returning a pointer.
  * Constructor: `CONSTRUCTOR()`
  * Result only: Cannot be passed as an argument (`C.INVALID_SIGNATURE`).
  * `Value() AS C.Handle OR NULL`: Returns the active `C.Handle` or `NULL` if C returned a null pointer.

### 1.5 Array Classes

Fixed numeric array classes borrow memory between Basic Next regions and C:

| Class | Region Element Type | C Pointer Representation |
| --- | --- | --- |
| `C.ByteArray` | `POINTER TO BYTE[]` | `uint8_t *` |
| `C.Int8Array` | `POINTER TO INT8[]` | `int8_t *` |
| `C.Int16Array` | `POINTER TO INT16[]` | `int16_t *` |
| `C.Int32Array` | `POINTER TO INTEGER[]` | `int32_t *` |
| `C.Int64Array` | `POINTER TO INT64[]` | `int64_t *` |
| `C.UInt16Array` | `POINTER TO UINT16[]` | `uint16_t *` |
| `C.UInt32Array` | `POINTER TO UINT32[]` | `uint32_t *` |
| `C.UInt64Array` | `POINTER TO UINT64[]` | `uint64_t *` |
| `C.FloatArray` | `POINTER TO FLOAT32[]` | `float *` |
| `C.DoubleArray` | `POINTER TO FLOAT[]` | `double *` |

Each array class provides:
* `ReadFrom(h AS C.Handle) AS VOID OR Error`: Copies `LEN(region)` elements from pointer `h` into the Basic Next region.
* `WriteTo(h AS C.Handle) AS VOID OR Error`: Copies `LEN(region)` elements from the Basic Next region into memory at pointer `h`.

### 1.6 Argument List & Invocation

* `C.Args`:
  * `CONSTRUCTOR()`: Allocates an empty argument vector.
  * `Add(v AS C.Value) AS VOID`: Appends an argument to the vector in call order.
  * `Count() AS INTEGER`: Returns the number of arguments in the vector.
* `C.Call(library AS STRING, symbol AS STRING, args AS C.Args, result AS C.Value) AS VOID OR Error`:
  * Resolves `library` and looks up `symbol`.
  * Verifies argument signatures and range limits.
  * Executes the native C function call via `libffi`.
  * Writes the return value into `result`.

---

## 2. Library Resolution Protocol

The `library` parameter is resolved as follows:

1. **Explicit Path:** If `library` contains `/` or `\`, it is treated as a path and loaded directly without search.
2. **Explicit Filename:** If `library` contains `.` (e.g. `"libm.so.6"`), it is loaded as an exact file name without appending platform extensions.
3. **Platform Logical Name:** Any other string is formatted according to host conventions (`lib<name>.so` on Linux, `lib<name>.dylib` on macOS, `<name>.dll` on Windows).
4. **Search Locations (in precedence order):**
   * Directories specified via `--ffi-path <dir>` (CLI flag on `bni run`, repeatable, ordered).
   * Directories specified in the `BN_LIBRARY_PATH` environment variable (platform path separator, empty entries ignored).
   * Host operating system dynamic library loader search paths.

Libraries are loaded once per process and never unloaded. C library initializers (`__attribute__((constructor))` or `DllMain`) execute at load time on the first `C.Call`. Neither `bni check`, `bni lsp`, nor `bnc` ever load libraries.

---

## 3. Error Codes

When a call or operation cannot proceed, `Error.Code` carries one of the following constants:

| Error Constant | Value | Raised by | Condition |
| --- | ---: | --- | --- |
| `C.INVALID_ARGUMENT` | 1 | `Call` | `library` or `symbol` is empty or contains NUL. |
| `C.LIBRARY_NOT_FOUND` | 2 | `Call` | Library file could not be located in any search path. |
| `C.LIBRARY_LOAD_FAILED` | 3 | `Call` | Dynamic loader failed to load library (incompatible architecture, missing dependencies). OS message in `Cause`. |
| `C.SYMBOL_NOT_FOUND` | 4 | `Call` | Symbol is not exported by the target library. |
| `C.INVALID_SIGNATURE` | 5 | `Call` | Mismatched call parameter (e.g. `C.Void` or `C.HandleResult` as argument, `C.String` as result). |
| `C.VALUE_OUT_OF_RANGE` | 6 | `Call` | Value exceeds width of target C type on host ABI. |
| `C.INVALID_STRING` | 7 | `Call` | `C.String` payload contains an internal NUL character. |
| `C.INVALID_UTF8` | 8 | `ReadString` | Bytes read from pointer are not valid UTF-8 text. |

---

## 4. Security, Sandbox & Target Limitations

* **No Sandboxing Confinement:** C code executes directly within the host process with unmanaged capabilities. It does not observe `HOST.FileSystem` policy, execution timeouts, or memory sandboxing.
* **Sandbox Incompatibility:** `bni run --sandbox` fails pre-flight if the target program imports `BNFfi`, refusing to execute because confinement cannot be guaranteed.
* **Target Support:** Native `bni` and `bnc` only. WebAssembly compilation is not supported.
