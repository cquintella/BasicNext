# How Basic Next Differs

## A different center of gravity

Basic Next is not trying to be a smaller Rust, a friendlier C++, or a typed
Python clone. Its center of gravity is low cognitive load: readable
object-oriented code, explicit types and contracts, and cross-platform host
capabilities.

| Language | Primary emphasis | BN's distinct choice |
| --- | --- | --- |
| Zig | Explicit systems programming and control | Readability and OO structure over systems-level minimalism. |
| Rust | Memory safety and zero-cost abstraction | A gentler, BASIC-inspired surface with explicit but simpler object and memory concepts. |
| Go | Operational simplicity and concurrency | Object-oriented modeling, explicit visibility, and a richer type vocabulary. |
| Java | Mature VM ecosystem and enterprise tooling | Less ceremony and a smaller language core, without requiring a framework. |
| C++ | Performance, hardware access, and broad abstraction mechanisms | A deliberately smaller, more readable language surface. |
| Python | Dynamic productivity and ecosystem breadth | Explicit static types and stronger structural contracts. |

## Important limits to the comparison

These languages are mature and have large ecosystems. BN is pre-implementation
and should not be judged as a replacement for their tooling, libraries,
performance, or production track record. The comparison explains design
intent, not feature parity.

## Where BN can be distinctive

- **Readable by construction:** `END IF`, `END WHILE`, typed declarations, and
  named object boundaries favor scanning and review.
- **Explicit without excessive ceremony:** types, imports, visibility, and
  host dependencies are visible; a program still begins with a simple
  `FUNCTION Start() AS VOID`.
- **BASIC heritage, modern boundaries:** familiar directness without line
  numbers, global loose code, or implicit architectural conventions.
- **Capabilities over vendor APIs:** `HOST` is intended to expose portable
  environmental services without making platform names part of normal program
  structure.
- **Deliberate scope:** BN will only add concepts that earn their cognitive
  cost through a clear need.

## The honest position

Use Zig, Rust, Go, Java, C++, or Python when their mature strengths are the
reason for the project. Follow BN when you value a language experiment focused
on making general-purpose, object-oriented code easier to read, discuss, and
evolve.
