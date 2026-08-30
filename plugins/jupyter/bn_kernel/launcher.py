# Author: Carlos Quintella
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

from __future__ import annotations

import json
import sys

from .kernel import execute_cell


def _option(arguments: list[str], flag: str, default: str | None = None) -> str | None:
    if flag not in arguments:
        return default
    index = arguments.index(flag) + 1
    if index == len(arguments) or arguments[index].startswith("-"):
        raise ValueError(f"{flag} requires a value")
    return arguments[index]


def main() -> int:
    try:
        connection_file = _option(sys.argv, "-f")
        bn = _option(sys.argv, "--bn", "bn")
    except ValueError as error:
        print(f"bn-kernel: {error}", file=sys.stderr)
        return 2
    if connection_file is not None:
        from .jupyter import JupyterKernel

        JupyterKernel(connection_file, bn=bn).serve()
        return 0
    for line in sys.stdin:
        request = json.loads(line)
        result = execute_cell(request["code"], stdin=request.get("stdin", ""), bn=request.get("bn", "bn"))
        print(json.dumps({"output": result.output, "error": result.error, "returncode": result.returncode}), flush=True)
    return 0
