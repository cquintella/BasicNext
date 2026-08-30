# Parallel Computing Proposal

**Status:** syntax reserved; not part of Basic Next 0.1.

Basic Next reserves `PARALLEL` for structured parallel work. The reservation
prevents a future parallel model from conflicting with existing program names
while the runtime, memory, and diagnostic semantics are designed.

## Candidate surface syntax

```basic
PARALLEL
    PrepareIndependentWork()
    ProcessIndependentWork()
END PARALLEL
```

```basic
PARALLEL FOR i AS INTEGER = 0 TO 999
    Process(i)
END PARALLEL FOR
```

```basic
PARALLEL FOR EACH value AS FLOAT IN values
    Transform(value)
END PARALLEL FOR
```

`PARALLEL FOR EACH` uses the existing `EACH` keyword. The closing form remains
`END PARALLEL FOR`, so the opening and closing structure is visible at a glance.

## Direction

The construct is inspired by OpenMP's structured parallel regions and loops,
but Basic Next will not copy OpenMP pragmas or its C/C++ data-sharing defaults.
Before this proposal can enter a language version, it must define:

- which variables are private, shared, read-only, or reduced;
- whether a parallel body may call I/O, mutate objects, allocate memory, or use
  pointers;
- ordering, cancellation, failure propagation, and deterministic diagnostics;
- reductions such as sum, minimum, maximum, and collection;
- CPU scheduling and the mapping to CUDA, ROCm, Metal, or WebGPU capabilities;
- host/device memory transfer and synchronization.

The first implementation target should be CPU data parallelism. GPU backends
must preserve the same source-level contract rather than introduce CUDA- or
ROCm-specific BN syntax.
