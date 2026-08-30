# Author: Carlos Quintella
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

"""Jupyter host adapter for complete Basic Next programs."""

from .kernel import ExecutionResult, execute_cell

__all__ = ["ExecutionResult", "execute_cell"]
