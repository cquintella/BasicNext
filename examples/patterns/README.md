# Design-pattern examples

This directory contains one Basic Next example for each of the nine GRASP
patterns presented by Craig Larman and each of the 23 Gang of Four patterns
used as the standard object-oriented pattern vocabulary in *Applying UML and
Patterns*.

Every `.bn` file has a `Start` function and can be checked with:

```text
bn check examples/patterns/<pattern>.bn
```

Examples marked `UNSUPPORTED` are still valid Basic Next programs. Their
comments identify the missing language capability instead of pretending that a
different construct implements the pattern. The executable output in those
files makes the limitation visible when the file is run.
