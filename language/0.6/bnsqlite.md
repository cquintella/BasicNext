# BNSqlite Standard Library 0.6

## Status

Accepted 0.6 standard module.

`BNSqlite` is an explicitly imported standard library module providing embedded SQL
database operations powered by an official SQLite3 C driver in the native runtime
(`bn_rt`).

```basic
IMPORT BNSqlite AS Sqlite
IMPORT BNData AS Data
```

## Dependency Direction and Companion Requirement

`BNSqlite` query operations produce instances of `Data.DataFrame`.

* Any Basic Next program that imports `BNSqlite` **must** also import `BNData` in its module imports.
* If a program imports `BNSqlite` without importing `BNData`, the semantic analyzer rejects the program with diagnostic `E0450` (`BNSqlite requires BNData`).

The host environment does not embed an SQL database engine. All database capability is mediated through `BNSqlite`.

```text
BNData ──────────────┐
                     ▼
HOST.FileSystem ──► BNSqlite
```

## Safe File Operations and Execution Policy

Database files accessed via `BNSqlite` are subject to the same security constraints and sandboxing rules as `HOST.FileSystem`:

* **Execution Policy:** If `BN_FS_POLICY` is configured as `read-only` or `deny`, or if sandbox roots are configured:
  * Opening a database for write access in an unauthorized location is rejected with `Sqlite.POLICY_DENIED`.
  * In `read-only` policy mode, mutative SQL statements (`CREATE`, `INSERT`, `UPDATE`, `DELETE`, `DROP`, `ALTER`) are rejected with `Sqlite.POLICY_DENIED`.
* **Path Validation:** Paths are validated against directory traversal and symlink escape vulnerabilities.
* **Busy Timeout:** All connections configure a default busy timeout of 5,000 milliseconds to gracefully wait for concurrent file locks before returning `Sqlite.BUSY`.
* **Single-Statement Enforcement:** By default, `Exec` and `Query` execute a single SQL statement per call, preventing compound injection attacks via chained semicolons.

## Open Modes

| Function | Signature | Meaning |
| --- | --- | --- |
| `Open` | `Open(path AS STRING) AS Sqlite.Connection OR Error` | Opens or creates a database at `path` for read and write. |
| `OpenReadOnly` | `OpenReadOnly(path AS STRING) AS Sqlite.Connection OR Error` | Opens an existing database at `path` strictly for reading. Mutative operations return `Sqlite.READ_ONLY`. |
| `OpenExisting` | `OpenExisting(path AS STRING) AS Sqlite.Connection OR Error` | Opens an existing database for read and write. If `path` does not exist, fails with `Sqlite.FILE_NOT_FOUND`. |

The special path `":memory:"` opens a private, in-memory database valid for the lifetime of the `Connection`.

## Connection Class

| Method | Signature | Meaning |
| --- | --- | --- |
| `Exec` | `Exec(sql AS STRING) AS VOID OR Error` | Executes non-row-producing statements (DDL, INSERT, UPDATE, DELETE). If called with a row-producing query (`SELECT`), returns `Sqlite.MISUSE`. |
| `Query` | `Query(sql AS STRING) AS Data.DataFrame OR Error` | Executes row-producing queries (`SELECT`). Returns a populated `DataFrame`. If called with non-row statement, returns `Sqlite.MISUSE`. |
| `Begin` | `Begin() AS VOID OR Error` | Begins an immediate transaction. Fails with `Sqlite.MISUSE` if a transaction is already active. |
| `Commit` | `Commit() AS VOID OR Error` | Commits the active transaction. Fails with `Sqlite.MISUSE` if no transaction is active. |
| `Rollback` | `Rollback() AS VOID OR Error` | Rolls back the active transaction. Fails with `Sqlite.MISUSE` if no transaction is active. |
| `Changes` | `Changes() AS INTEGER` | Returns the count of database rows modified, inserted, or deleted by the most recently completed statement. |
| `LastInsertRowId` | `LastInsertRowId() AS INTEGER` | Returns the rowid of the most recent successful `INSERT` into a rowid table. |
| `Close` | `Close() AS VOID OR Error` | Closes the connection and releases file locks immediately. Subsequent method calls on the closed connection return `Sqlite.CLOSED`. |

## DataFrame Mapping

`Query` maps SQLite column types into `BNData.DataFrame` columns:

| SQLite Storage Class | DataFrame Column Type | Notes |
| --- | --- | --- |
| `INTEGER` | `INTEGER` | 64-bit signed integer. |
| `REAL` | `FLOAT` | 64-bit IEEE 754 float. |
| `TEXT` | `STRING` | UTF-8 text. |
| `NULL` | `NA` | Represented as missing cell value in `BNData`. |
| `BLOB` | `STRING` | Encoded as text or rejected if non-UTF8. |

An empty query result (zero matching rows) returns an empty `DataFrame` containing column headers, not an `Error`.

## Errors

Every `Error` returned by `BNSqlite` carries an `Operation` naming the function or method, a `Message` describing the failure and naming the input, a `Cause`, and one of the following canonical integer codes:

| Constant | Value | When it occurs |
| --- | ---: | --- |
| `Sqlite.FILE_NOT_FOUND` | 1 | The specified database file does not exist when using `OpenExisting` or `OpenReadOnly`. |
| `Sqlite.ACCESS_DENIED` | 2 | Operating system file permission denied on the database file or parent directory. |
| `Sqlite.POLICY_DENIED` | 3 | Execution policy (`BN_FS_POLICY`) denies access to the path or prohibits write operations. |
| `Sqlite.CORRUPT` | 4 | The file is not a valid SQLite database or contains damaged/corrupted pages. |
| `Sqlite.BUSY` | 5 | Database table or file locked by another process and busy timeout expired. |
| `Sqlite.LOCKED` | 6 | Deadlock or conflict within the current connection or transaction. |
| `Sqlite.READ_ONLY` | 7 | Attempted to write to a connection opened with `OpenReadOnly` or on read-only media. |
| `Sqlite.SYNTAX_ERROR` | 8 | The SQL statement contains a syntax error. |
| `Sqlite.SCHEMA_ERROR` | 9 | A referenced table, column, index, or view does not exist in the database schema. |
| `Sqlite.CONSTRAINT_VIOLATION` | 10 | An `INSERT` or `UPDATE` violated a `PRIMARY KEY`, `FOREIGN KEY`, `UNIQUE`, `CHECK`, or `NOT NULL` constraint. |
| `Sqlite.TYPE_MISMATCH` | 11 | Value cannot be converted to the expected DataFrame column type. |
| `Sqlite.MISUSE` | 12 | API misuse: `Exec` called with `SELECT`; `Query` called with non-row statement; `Commit`/`Rollback` without active transaction; nested transactions. |
| `Sqlite.CLOSED` | 13 | Operation attempted on a connection that has already been closed. |
| `Sqlite.LIMIT_EXCEEDED` | 14 | SQL string length, parameter count, or column count exceeds safety thresholds. |
| `Sqlite.IO_FAILED` | 15 | Physical I/O failure, disk full, or unreachable storage device. |
| `Sqlite.INTERNAL_ERROR` | 16 | Internal invariant or unexpected error from the SQLite C engine. |
