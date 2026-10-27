# BNCompress standard library

## Status

External standard-library module (bucket 0.6.11). Source of truth:
`modules/bn/BNCompress.bn`. The compression algorithms and archive parsers are
**not** implemented in Basic Next source: they come from `bn_core_compress`
(wrapping `flate2`, `miniz_oxide`, and `zip`) through the C ABI in `bn_rt::compress`
and the interpreted adapter `bn_lib_compress`. Both backends (`bni` and `bnc`)
execute identical implementations.

## Access

```basic
IMPORT BNCompress AS Compress

FUNCTION Start() AS VOID
    LET archive AS Compress.ZipArchive OR Error = Compress.ZipArchive.Create("example.zip")
    IF archive IS Compress.ZipArchive THEN
        archive.AddText("hello.txt", "Hello, World!")
        archive.Close()
    END IF
END FUNCTION
```

Logical import name: `BNCompress`. An explicit alias such as `Compress` or `Zip`
is recommended. `BNCompress` is an external module, never a built-in `HOST` capability.

## Types

| Type | Role |
| --- | --- |
| `Compress.Bytes` | Opaque binary buffer representing raw or compressed data. Managed by reference counting (ARC). Supports creation from text/hex and extraction to text/hex. |
| `Compress.ZipArchive` | Opaque archive handle for reading, creating, and extracting ZIP archives. Supports Deflate compression and STORED (uncompressed) entries. Closed automatically on scope exit or via `.Close()`. |

## Buffer Construction & Conversion

| Function | Signature | Result |
| --- | --- | --- |
| `FromText` | `FromText(text AS STRING) AS Bytes` | Opaque `Bytes` buffer holding the UTF-8 bytes of `text`. |
| `FromHex` | `FromHex(hex AS STRING) AS Bytes OR Error` | Opaque `Bytes` buffer decoded from hex, or `Error` if malformed. |

### `Compress.Bytes` Methods

| Method | Signature | Result |
| --- | --- | --- |
| `Length` | `Length() AS INTEGER` | Number of bytes in the buffer. |
| `ToHex` | `ToHex() AS STRING` | Lowercase hexadecimal representation of the buffer contents. |
| `ToText` | `ToText() AS STRING OR Error` | Decodes the buffer as UTF-8 string, or `Error` if invalid UTF-8. |

## In-Memory Compression (Deflate & Gzip)

| Function | Signature | Result |
| --- | --- | --- |
| `Deflate` | `Deflate(data AS Bytes) AS Bytes OR Error` | Compresses `data` using standard DEFLATE (RFC 1951). |
| `Inflate` | `Inflate(data AS Bytes) AS Bytes OR Error` | Decompresses DEFLATE data into uncompressed bytes. |
| `Gzip` | `Gzip(data AS Bytes) AS Bytes OR Error` | Compresses `data` using the Gzip container format (RFC 1952). |
| `Gunzip` | `Gunzip(data AS Bytes) AS Bytes OR Error` | Decompresses a Gzip stream into uncompressed bytes. |
| `DeflateText` | `DeflateText(text AS STRING) AS Bytes OR Error` | Convenience function compressing UTF-8 string to DEFLATE bytes. |
| `InflateText` | `InflateText(data AS Bytes) AS STRING OR Error` | Convenience function decompressing DEFLATE bytes directly to a UTF-8 string. |
| `GzipText` | `GzipText(text AS STRING) AS Bytes OR Error` | Convenience function compressing UTF-8 string to Gzip bytes. |
| `GunzipText` | `GunzipText(data AS Bytes) AS STRING OR Error` | Convenience function decompressing Gzip bytes directly to a UTF-8 string. |

## ZIP Archive Management (`Compress.ZipArchive`)

### Archive Factories

| Method | Signature | Result |
| --- | --- | --- |
| `Create` | `ZipArchive.Create(path AS STRING) AS ZipArchive OR Error` | Creates a new empty ZIP file at `path` for writing. |
| `Open` | `ZipArchive.Open(path AS STRING) AS ZipArchive OR Error` | Opens an existing ZIP file at `path` for reading. |

### Archive Methods

| Method | Signature | Result |
| --- | --- | --- |
| `AddFile` | `AddFile(entryName AS STRING, diskPath AS STRING) AS VOID OR Error` | Adds a file from disk into the archive using DEFLATE compression. |
| `AddText` | `AddText(entryName AS STRING, content AS STRING) AS VOID OR Error` | Adds a text entry into the archive using DEFLATE compression. |
| `AddStoredText` | `AddStoredText(entryName AS STRING, content AS STRING) AS VOID OR Error` | Adds an uncompressed (STORED) text entry. Essential for EPUB `mimetype`. |
| `AddBytes` | `AddBytes(entryName AS STRING, data AS Bytes) AS VOID OR Error` | Adds binary `data` into the archive using DEFLATE compression. |
| `AddStoredBytes` | `AddStoredBytes(entryName AS STRING, data AS Bytes) AS VOID OR Error` | Adds uncompressed (STORED) binary `data`. |
| `ExtractAll` | `ExtractAll(destinationDir AS STRING) AS VOID OR Error` | Extracts all entries into `destinationDir`, preventing directory traversal attacks (Zip Slip). |
| `ExtractFile` | `ExtractFile(entryName AS STRING, diskPath AS STRING) AS VOID OR Error` | Extracts a single entry to `diskPath`. |
| `ExtractText` | `ExtractText(entryName AS STRING) AS STRING OR Error` | Extracts a single entry and decodes it as UTF-8 string. |
| `ExtractBytes` | `ExtractBytes(entryName AS STRING) AS Bytes OR Error` | Extracts a single entry as raw `Bytes`. |
| `EntryCount` | `EntryCount() AS INTEGER` | Returns the total number of entries in the archive. |
| `EntryName` | `EntryName(index AS INTEGER) AS STRING OR Error` | Returns the name of the entry at 0-based index. |
| `Close` | `Close() AS VOID OR Error` | Finalizes central directory headers and closes the file descriptor. |

## Errors

All fallible functions and methods return `T OR Error`. Failures carry structured
diagnostics with:
- `Code`: `BNCompress.INVALID_ARGUMENT`, `BNCompress.CORRUPT_DATA`, `BNCompress.IO_ERROR`, `BNCompress.NOT_FOUND`, `BNCompress.ZIP_SLIP`.
- `Operation`: Name of the failing method (e.g., `"ZipArchive.Open"`, `"Inflate"`).
- `Message`: Cataloged human-readable summary.
- `Cause`: Specific underlying OS or compression error.
