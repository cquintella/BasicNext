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
