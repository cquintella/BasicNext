# Basic Next Jupyter kernel

The Python package, kernelspec, and launcher live together in this directory.
The wire protocol and host contract are documented in
[`docs/project/kernel.md`](../../docs/project/kernel.md).

Install from the repository root:

```shell
python -m pip install -e plugins/jupyter
```

Run its tests from the repository root:

```shell
PYTHONPATH=plugins/jupyter python -m unittest tests/test_kernel.py tests/test_jupyter.py
```

## Execution contract

The supported mode is `Program`: every cell is compiled as a complete `.bn`
program and must define `FUNCTION Start()`. The kernel starts a fresh
`bn run --no-filesystem` process for each cell, so declarations, imports,
objects, and mutable values do not survive between cells. This is a whole
program notebook, not a REPL. Filesystem access remains denied and a cell
cannot inspect or mutate a previous cell's process.
