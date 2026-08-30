# Where Basic Next Can Matter First

Basic Next is intended to be general-purpose. Its opportunity is not to imitate
every established ecosystem on day one; it is to make useful programming feel
far more approachable without treating the programmer as less capable.

## Teach how computers work - through programs that read like programs

BN can be a serious teaching language because its code exposes structure:
types, loops, objects, memory, modules, and host boundaries are visible. A
student can learn what the computer is doing without first learning to work
around an opaque toolchain or a framework's private conventions.

This makes it useful for introductory programming, data structures, algorithms,
computer architecture, and compiler courses.

## Automate the work that should not be manual

Small scripts are where people first experience programming as leverage. BN is
designed for the path from "I do this every week" to "the computer does this
for me": file transformations, reports, data cleanup, command-line tools,
teaching utilities, and workflow automation.

```text
bn run report.bn
```

No project manifest. No framework bootstrap. No ceremony before a useful run.

## Build readable domain software

Business applications are made of concepts people must discuss: customer,
order, schedule, payment, inventory, policy, alert. BN's classes, interfaces,
visibility, and explicit types are designed to keep those concepts present in
the code instead of dissolving them into incidental infrastructure.

## Talk to the environment without losing the language

BN's `HOST` model is a path to capabilities such as console, memory, files,
devices, web interfaces, and parallel hardware. The aim is one understandable
import model, not a different mental model for every platform.

## Where BN should not pretend to be ready

BN 0.1 has a reference interpreter (`bn check` / `bn run`). It is not yet the
right choice for a production system that needs a package ecosystem, GPU
stack, kernel interface, or hard real-time profile. Those are future
engineering targets, not present promises.

The first win is simpler and more important: make programming itself feel more
available, more readable, and more useful to more people.
