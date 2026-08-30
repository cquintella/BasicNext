# Proposal: File I/O in Basic Next

**Status:** Accepted into 0.2 with amendments. Normative text is
[`docs/language/0.2/0.2.md`](../../docs/language/0.2/0.2.md) and
[`docs/library/host.md`](../../docs/library/host.md), not this proposal. 0.2 keeps
the object-oriented `HOST.FileSystem` / `FS.File` shape and drops directory
permission APIs, `ChangeDirectory`, and octal literals.

## Motivation
File access is a fundamental requirement for most applications, but it is currently absent from the `0.1` and `0.2` specifications. To adhere to Basic Next's philosophy (KISS, explicit types, cross-platform via capabilities, and no exceptions), this proposal outlines a clean, object-oriented approach to reading and writing both text and binary files.

## 1. Capability Import
File access is a host capability. It is not part of the standard library math or temporal namespaces. It must be explicitly imported and can be restricted by the runtime environment (e.g., sandboxed in WebAssembly or limited in a Jupyter kernel).

```basic
IMPORT HOST.FileSystem AS FS
```

## 2. Core File Operations: Object-Oriented Approach
To fully align with the "Object-oriented by default" philosophy, file operations are exposed as methods on the `FS.File` object rather than global keywords. This keeps the parser lean, allows IDE auto-completion, and provides a clear path for future extensions (like `Seek` or `Flush`).

### Opening and Closing
The `FS.Open` method acts as a factory, returning a file instance or an `Error`. The `Close` method releases the handle.
The mode is specified by predefined constants (e.g., `FS.READ`, `FS.WRITE`, `FS.APPEND`).

```basic
LET file AS FS.File OR Error = FS.Open("data.txt", FS.READ)

IF file IS FS.File THEN
    // File operations go here
    
    file.Close()
ELSE IF file IS Error THEN
    PRINT "Failed to open file:", file.Message
END IF
```

### Reading (Read)
The `Read` method infers behavior from the requested type or arguments.

**Text files:**
```basic
// Reads a single line (returns STRING OR EOF)
LET line AS STRING OR EOF = file.Read()

// Reads the entire remaining file contents
LET allText AS STRING OR EOF = file.ReadAll()
```

**Binary files:**
```basic
LET buffer AS BYTE[1024]
// Reads into the provided buffer; returns the number of bytes read OR EOF
LET readCount AS INTEGER OR EOF = file.ReadBytes(buffer)
```

### Writing (Write)
The `Write` method writes data to the file handle.

**Text files:**
```basic
// Writes text.
file.Write("Result: ")
// Alternatively, WriteLine appends a newline
file.WriteLine("42")
```

**Binary files:**
```basic
LET data AS BYTE[4] = [0xDE, 0xAD, 0xBE, 0xEF]
// Writes the byte array
file.WriteBytes(data)
```

## 3. Full Examples

### Example: Copying a text file line by line
```basic
IMPORT HOST.FileSystem AS FS

FUNCTION CopyTextFile() AS VOID
    LET inFile AS FS.File OR Error = FS.Open("input.txt", FS.READ)
    IF inFile IS Error THEN
        PRINT "Error reading input:", inFile.Message
        RETURN
    END IF

    LET outFile AS FS.File OR Error = FS.Open("output.txt", FS.WRITE)
    IF outFile IS Error THEN
        PRINT "Error creating output:", outFile.Message
        inFile.Close()
        RETURN
    END IF

    WHILE TRUE
        LET line AS STRING OR EOF = inFile.Read()
        IF line IS EOF THEN
            EXIT WHILE
        END IF
        
        outFile.WriteLine(line)
    END WHILE

    inFile.Close()
    outFile.Close()
    PRINT "Copy complete."
END FUNCTION
```

### Example: Copying a binary file
```basic
IMPORT HOST.FileSystem AS FS

FUNCTION CopyBinaryFile() AS VOID
    LET inFile AS FS.File OR Error = FS.Open("image.png", FS.READ)
    IF inFile IS Error THEN
        RETURN
    END IF

    LET outFile AS FS.File OR Error = FS.Open("image_copy.png", FS.WRITE)
    IF outFile IS Error THEN
        inFile.Close()
        RETURN
    END IF

    LET buffer AS BYTE[1024]
    
    WHILE TRUE
        LET bytesRead AS INTEGER OR EOF = inFile.ReadBytes(buffer)
        IF bytesRead IS EOF THEN
            EXIT WHILE
        END IF
        
        // Note: semantics would write exactly bytesRead
        outFile.WriteBytes(buffer, bytesRead)
    END WHILE

    inFile.Close()
    outFile.Close()
END FUNCTION
```

## 4. File and Directory Management
In addition to reading and writing, `HOST.FileSystem` exposes standard management operations. They follow the same `OR Error` pattern for explicit failure handling.

### Navigation (cd)
```basic
// cd: Changes the process's current working directory.
LET cdResult AS VOID OR Error = FS.ChangeDirectory("/var/log")

// Returns the current working directory.
LET cwd AS STRING OR Error = FS.CurrentDirectory()
```

### Movement and Deletion (rm, mv, cp)
```basic
// rm: Removes a file.
LET rmResult AS VOID OR Error = FS.DeleteFile("old.txt")

// rm -r: Removes a directory.
LET rmdirResult AS VOID OR Error = FS.DeleteDirectory("temp/")

// mv: Renames or moves a file or directory.
LET mvResult AS VOID OR Error = FS.Move("source.txt", "dest.txt")

// cp: Copies a file.
LET cpResult AS VOID OR Error = FS.Copy("source.txt", "backup.txt")
```

### Permissions and Ownership (chmod, chown, chgrp)
```basic
// chmod: Changes file permissions (e.g., using integer literal for mode).
LET chmodResult AS VOID OR Error = FS.SetPermissions("script.sh", 0o755)

// chown / chgrp: Changes the owner and group names/IDs.
LET chownResult AS VOID OR Error = FS.SetOwner("data.db", "carlos", "staff")
```

## 5. Error Handling & Types
- **No Exceptions**: All operations that touch the file system can fail (e.g., file not found, permission denied, disk full, invalid UTF-8). They return the standard `Error` type (with `Code` and `Message`) using the `OR Error` syntax.
- **Explicit Separation**: Using `STRING` for text and `BYTE[]` for binary eliminates implicit encoding/decoding errors at the call site.
- **Paths**: Paths are passed as standard `STRING`. Path manipulation (joining, normalization) should be proposed as a separate object (e.g., `FS.Path`).

## 6. Alignment with Philosophy
- **KISS & Low cognitive load**: Exposing methods directly on `FS.File` reduces the global keyword count and keeps the object model predictable.
- **Explicit contracts**: Returning `OR Error` forces the programmer to recognize that I/O operations are not guaranteed to succeed, preventing unexpected crashes.
- **Deliberate evolution**: Leaving random-access (seek) and asynchronous I/O out of the initial proposal keeps the core small and manageable.
