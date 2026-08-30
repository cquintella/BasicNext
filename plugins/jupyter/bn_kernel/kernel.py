# Author: Carlos Quintella
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import subprocess
import tempfile


@dataclass(frozen=True)
class ExecutionResult:
    """Result forwarded by ``bn run`` for one isolated cell."""

    output: str
    error: str | None
    returncode: int


def execute_cell(
    source: str,
    *,
    bn: str = "bn",
    stdin: str = "",
    cwd: str | os.PathLike[str] | None = None,
) -> ExecutionResult:
    """Run one complete-program cell in a fresh temporary ``.bn`` file."""
    with tempfile.TemporaryDirectory(prefix="basicnext-kernel-") as directory:
        path = Path(directory) / "cell.bn"
        path.write_text(source, encoding="utf-8")
        process = subprocess.run(
            [bn, "run", "--no-filesystem", str(path)],
            input=stdin,
            text=True,
            capture_output=True,
            cwd=cwd,
            check=False,
        )
    return ExecutionResult(process.stdout, process.stderr or None, process.returncode)
