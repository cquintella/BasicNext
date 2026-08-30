# Proposal: Host Capabilities and Bound Functions

## Status

Exploratory. This proposal is not part of Basic Next 0.1.

## Motivation

Basic Next should integrate with parallel devices and web environments without
making CUDA, ROCm, a specific GPU vendor, or the DOM into language keywords.
Programs should not need loose top-level commands, and host-specific code
should remain visibly attached to the capability that owns it.

## Direction

The runtime exposes named objects through `HOST`. A module imports the
capability it needs and may declare a function qualified by that imported
object.

```basic
IMPORT HOST.gpu AS gpu

FUNCTION gpu.VectorAdd(a, b, result, n) AS VOID
    LET i AS INTEGER = gpu.globalId()

    IF i < n THEN
        result[i] = a[i] + b[i]
    END IF
END FUNCTION
```

The qualified declaration expresses association; it does not modify a host
object at runtime. `gpu.VectorAdd` is a module-owned function bound to the
`gpu` capability by the compiler or runtime.

The same form can describe a web event handler:

```basic
IMPORT HOST.dom AS dom

FUNCTION dom.OnSave() AS VOID
    dom.setText("#status", "Saved")
END FUNCTION

FUNCTION Start() AS VOID
    dom.onClick("#save", dom.OnSave)
END FUNCTION
```

## Principles

- `HOST` exposes capabilities, not vendors. CUDA, ROCm/HIP, SPIR-V, WebGPU, or
  CPU simulation are implementation choices for a `gpu` capability.
- A capability defines its own contract, types, and permitted operations.
- A GPU-bound function must have explicit restrictions on memory access,
  supported types, and host interaction. It cannot silently behave like an
  ordinary host function.
- A DOM-bound function is a host event handler, not a GPU kernel; shared
  syntax does not imply shared execution rules.
- Environments may omit a capability. Programs must receive a clear diagnostic
  when a required capability is unavailable.
- Capability APIs belong to the standard library or host profiles, not to the
  language's reserved-word set.

## Open questions

1. Should qualified declarations use `FUNCTION gpu.Name`, an annotation, or a
   separate declaration form?
2. How are buffers, transfers, and synchronization expressed for GPU targets?
3. How does a module declare required versus optional capabilities?
4. Which portable GPU subset should be specified before any CUDA or ROCm
   backend is considered?
5. How are DOM handlers scheduled and how will asynchronous work be expressed?

## Adoption path

1. Finish the host import contract in 0.1 without adding bound functions.
2. Define the generic capability model in a post-0.1 specification.
3. Define one portable GPU profile and a CPU simulation for conformance.
4. Add vendor-specific backends only after the portable profile is stable.
