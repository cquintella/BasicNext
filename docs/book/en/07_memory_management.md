# Memory Management

Basic Next 0.5.0 uses automatic reference counting (ARC) for class objects.
The runtime tracks strong references, releases them when bindings leave scope, and
runs a class destructor exactly once when the last strong reference disappears.

## Strong references and destructors

Assignment to a class binding creates another strong reference. `RELEASE` drops
one binding early; it does not force destruction while another strong reference
still exists.

```basic
CLASS Box
    PUBLIC value AS INTEGER = 7

    PUBLIC FUNCTION CONSTRUCTOR()
    END FUNCTION

    FUNCTION DESTRUCTOR()
        PRINT "DEINIT"
    END FUNCTION
END CLASS

FUNCTION Start() AS VOID
    LET first AS Box = NEW Box()
    LET second AS Box = first
    RELEASE first
    PRINT second.value
END FUNCTION
```

The assignment to `second` retains the object. Releasing `first` therefore leaves
`second` valid. When `second` leaves `Start`, the count reaches zero and
`DEINIT` is printed once.

## Weak references

A weak reference does not keep an object alive. Declare it with `AS WEAK` and
make the nullable state explicit:

```basic
CLASS Node
    PUBLIC next AS WEAK Node OR NULL = NULL
END CLASS

FUNCTION Start() AS VOID
    LET owner AS Node = NEW Node()
    LET observer AS WEAK Node OR NULL = owner
    RELEASE owner
    IF observer IS NULL THEN
        PRINT "expired"
    END IF
END FUNCTION
```

When the last strong reference is released, every weak reference to that object
is cleared to `NULL`.

## `RELEASE`

`RELEASE` is valid for managed object bindings and for owned values such as
primaries, structs, and vectors. It is a statement on a binding:

```basic
LET number AS INTEGER = 10
LET name AS STRING = "temporary"
LET values AS INTEGER[] = [1, 2, 3]
RELEASE number
RELEASE name
RELEASE values
```

Element expressions are not bindings, so `RELEASE values[0]` is rejected. Close
HOST resources through their API (for example `file.Close()`) and then release
the owning binding. `DELETE` is not a 0.5.0 language keyword.

## Pointers and diagnostics

Pointer allocation follows the value/memory ABI rules. Release an owned pointer
binding with `RELEASE`; never access it after that point. The validator and
runtime diagnose `USE_AFTER_RELEASE`, `DOUBLE_RELEASE`, `NULL_POINTER_ACCESS`,
and invalid bounds instead of silently continuing.

The normative ownership and ABI contract is in
[`0.5.md`](../../language/0.5/0.5.md#memory-model-arc). The implementation evidence
and conformance fixtures are tracked with the 0.5.0 release bucket.
