import os
from pathlib import Path
import unittest

from bn_kernel.kernel import execute_cell


ROOT = Path(__file__).resolve().parents[1]
BN = os.environ.get("BN_TEST_BINARY", str(ROOT / "target" / "debug" / "bn"))


class ProgramKernelTests(unittest.TestCase):
    def test_cell_requires_a_complete_program(self):
        result = execute_cell('PRINT "not a program"', bn=BN, cwd=ROOT)
        self.assertNotEqual(result.returncode, 0)

    def test_cell_runs_as_a_fresh_program_without_filesystem(self):
        result = execute_cell(
            'FUNCTION Start() AS VOID\nPRINT "fresh"\nEND FUNCTION\n',
            bn=BN,
            cwd=ROOT,
        )
        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.output, "fresh\n")

    def test_filesystem_capability_is_denied(self):
        source = (
            "IMPORT HOST.FileSystem AS FS\n"
            "FUNCTION Start() AS VOID\n"
            "FS.Exists(\"Cargo.toml\")\n"
            "END FUNCTION\n"
        )
        result = execute_cell(source, bn=BN, cwd=ROOT)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("HOST_CAPABILITY_UNAVAILABLE", result.error or "")


if __name__ == "__main__":
    unittest.main()
