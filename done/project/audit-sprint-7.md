# Sprint 7 audit

Status: Complete

Normative sources reviewed: `docs/language/0.2/0.2.md`,
`ongoing/WBS-0.2.md`, `ongoing/bucket.md`.

| Requirement | Evidence | Result |
| --- | --- | --- |
| Fresh temporary source and `bn run` subprocess | `plugins/jupyter/bn_kernel/kernel.py`; `tests/test_kernel.py` | pass |
| Complete program per cell and no state | `execute_cell` writes one file per call; complete-program test | pass |
| Stream output forwarding | `plugins/jupyter/bn_kernel/jupyter.py` IOPub `stream`; `tests/test_jupyter.py` | pass |
| `INPUT()` via Jupyter stdin | `bn run --jupyter-stdin` signals each executed `INPUT()`; `jupyter.py` exchanges signed `input_request` / `input_reply`; live wire test covers a value and cancelled input | pass |
| Filesystem and positioned console unavailable before `Start` | preflight guards; kernel integration test | pass |
| Kernelspec and launcher | `plugins/jupyter/kernelspec/kernel.json`, `plugins/jupyter/bin/bn-kernel`, `plugins/jupyter/bn_kernel/jupyter.py` | pass |

Direct evidence:
- 2026-08-29: `uv run --frozen python -m unittest -v
  tests/test_kernel.py tests/test_jupyter.py`: pass (11 tests).
- Live ZeroMQ evidence includes `kernel_info`, signed messages, large output,
  heartbeat, `input_request`/`input_reply`, interrupt, and shutdown.
- Launcher coverage includes missing `-f` and `--bn` values without traceback.

Open requirements: none within Sprint 7 scope.
Completion decision: complete after R1, R2, and R8 closure; revalidated on
2026-08-29.
