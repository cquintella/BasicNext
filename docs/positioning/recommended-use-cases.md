# Recommended Use Cases for Basic Next

## General-purpose, with an intentional starting point

Basic Next is intended to be a general-purpose language. Its first useful
territory is software where readability, explicit structure, and portability
matter more than platform-specific tricks or a mature ecosystem on day one.

## Strong fits

### Command-line tools and automation

BN's explicit entry method, `PRINT`, `INPUT()`, types, and small language core
fit utilities that read input, transform it, and produce dependable output.

### Business and domain applications

Classes, interfaces, visibility, and modules support software organized around
real domain concepts: customers, invoices, schedules, inventories, policies,
and workflows.

### Data-oriented utilities

Explicit numeric types, fixed-size vectors, typed memory, and a future data
library direction make BN a credible home for parsers, import/export tools,
calculators, and structured data processing.

### Teaching and learning

BN can help students see program structure without first navigating an
ecosystem full of build systems, framework conventions, or hidden runtime
behavior. This is a useful use case, but not the language's sole purpose.

### Cross-platform host integrations

The `HOST` capability model is intended to make a program's environmental
dependencies visible and portable. It is a direction for the runtime, not a
claim that every capability already exists.

## Not the first target

BN should not initially be chosen for kernel work, bare-metal firmware,
ultra-low-latency systems, performance-critical GPU kernels, or a project that
depends on the mature package ecosystem of another language. Those are future
evaluation areas, not current promises.

## A practical selection rule

Choose BN when the primary value is code that is explicit, approachable, and
structured around clear objects and contracts. Choose a mature alternative
when its existing runtime, libraries, tooling, or proven operational profile is
the decisive requirement.
