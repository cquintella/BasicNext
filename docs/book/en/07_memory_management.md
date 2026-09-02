# Memory Management

Basic Next version 0.3 does not feature a garbage collector. Memory management is strictly manual. Developers are responsible for allocating memory when needed and explicitly freeing it when it is no longer required.

## Manual Allocation (`NEW` and `DELETE`)

The `NEW` keyword is the sole mechanism for dynamic allocation. It is used to create class instances or contiguous typed memory regions. 

When you allocate a class instance, `NEW` executes the constructor. When you are finished with the object, you release it using `DELETE`, which runs the class's `DESTRUCTOR` (if defined) before freeing the memory.

```basic
LET customer AS Customer = NEW Customer(10)
// ... use the object ...
DELETE customer
```

If a constructor fails, the partially constructed object is discarded without executing the destructor. At program termination, the runtime recovers any memory not released by `DELETE`, but destructors are *not* run for those leaked objects. `DELETE` is the only deterministic destruction point.

## Pointers

Pointers reference dynamically allocated, contiguous numeric data. In version 0.3, pointer elements must be numeric types; pointers to strings, booleans, or classes are excluded.

There are three ways to declare a pointer type, depending on its size constraints:

1. **Single Value**: `POINTER TO TYPE`
2. **Fixed-Size Region**: `POINTER TO TYPE[length]`
3. **Dynamic Region**: `POINTER TO TYPE[]`

```basic
// Allocating a single value
LET value AS POINTER TO INTEGER = NEW INTEGER
value[0] = 42
DELETE value

// Allocating a dynamic region
LET count AS INTEGER = 1024
LET samples AS POINTER TO FLOAT[] = NEW FLOAT[count]
samples[0] = 1.5
DELETE samples
```

Allocated memory is zero-initialized (filled with the type's default value). Pointer indexing is strictly bounds-checked by the runtime, and pointer arithmetic is not permitted in version 0.3. In the current version, you can also use `LEN()` on region pointers (`POINTER TO TYPE[length]` and `POINTER TO TYPE[]`) to get their element count, but `LEN` on a single-value pointer remains a static error.

Pointer assignment and parameter passing copy the pointer handle (creating an alias) without transferring ownership implicitly. `DELETE` accepts any alias to the base pointer originally returned by `NEW`.

## Memory Safety and Runtime Errors

Because memory is managed manually, Basic Next enforces strict runtime checks to prevent silent corruption:

- **Null Pointers**: Pointers can be `NULL`. Indexing or dereferencing a `NULL` pointer—or attempting to `DELETE NULL`—raises a `NULL_POINTER_ACCESS` error. You must explicitly test optional pointers using `IS NULL`.
- **Use After Delete**: Once an allocation is deleted, all aliases become invalid. Attempting to access the memory later raises a `USE_AFTER_DELETE` error.
- **Double Delete**: Attempting to delete memory that has already been deleted raises a `DOUBLE_DELETE` error. An allocation is considered deleted while its destructor runs, so a reentrant `DELETE` also triggers this error.
- **Out of Bounds**: Any index outside the allocated region raises an `INDEX_OUT_OF_BOUNDS` error.
- **Allocation Limits**: Requesting memory with a computed negative count raises `ALLOCATION_SIZE_INVALID`. If the requested size overflows or exceeds the host's capacity, `ALLOCATION_SIZE_OVERFLOW` or `ALLOCATION_TOO_LARGE` is raised.
