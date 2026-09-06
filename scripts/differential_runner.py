"""Small, dependency-free runner used by capability/parity checks.

Failures retain command, status, stdout and stderr in an artifact directory;
timeouts are reported separately from process failures.
"""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import tempfile
from typing import Sequence


def run(command: Sequence[str], *, timeout: float = 30.0) -> subprocess.CompletedProcess[bytes]:
    try:
        result = subprocess.run(command, capture_output=True, check=False, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        report = {
            "command": [str(item) for item in command],
            "status": "timeout",
            "timeout_seconds": timeout,
            "stdout": (error.stdout or b"").decode(errors="replace"),
            "stderr": (error.stderr or b"").decode(errors="replace"),
        }
        _retain(report)
        raise
    if result.returncode != 0:
        _retain(
            {
                "command": [str(item) for item in command],
                "status": "failed",
                "returncode": result.returncode,
                "stdout": result.stdout.decode(errors="replace"),
                "stderr": result.stderr.decode(errors="replace"),
            }
        )
    return result


def _retain(report: dict[str, object]) -> pathlib.Path:
    directory = pathlib.Path(os.environ.get("BN_FAILURE_ARTIFACT_DIR", tempfile.gettempdir()))
    directory.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix="bn-diff-", suffix=".json", dir=directory)
    os.close(fd)
    path = pathlib.Path(name)
    path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return path
