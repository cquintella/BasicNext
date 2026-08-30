import unittest
import subprocess
import sys

from bn_kernel import execute_cell


class KernelTests(unittest.TestCase):
    def test_launcher_reports_missing_option_values_without_traceback(self):
        for arguments in (["-f"], ["-f", "connection.json", "--bn"]):
            process = subprocess.run(
                [sys.executable, "-m", "bn_kernel", *arguments],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(process.returncode, 2)
            self.assertIn("requires a value", process.stderr)
            self.assertNotIn("Traceback", process.stderr)

    def test_cell_is_a_complete_program(self):
        result = execute_cell(
            "FUNCTION Start() AS VOID\nPRINT 42\nEND FUNCTION\n",
            bn="target/debug/bn",
        )
        self.assertEqual((result.returncode, result.output), (0, "42\n"))

    def test_filesystem_is_rejected_before_execution(self):
        result = execute_cell(
            "IMPORT HOST . FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT FS.Exists(\"Cargo.toml\")\nEND FUNCTION\n",
            bn="target/debug/bn",
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("HOST_CAPABILITY_UNAVAILABLE", result.error or "")

    def test_filesystem_import_without_use_is_rejected_before_start(self):
        result = execute_cell(
            "IMPORT HOST.FileSystem AS FS\nFUNCTION Start() AS VOID\nPRINT \"ran\"\nEND FUNCTION\n",
            bn="target/debug/bn",
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("HOST_CAPABILITY_UNAVAILABLE", result.error or "")
        self.assertNotIn("ran", result.output or "")

    def test_cells_do_not_share_state(self):
        first = execute_cell(
            "FUNCTION Start() AS VOID\nLET value AS INTEGER = 7\nEND FUNCTION\n",
            bn="target/debug/bn",
        )
        second = execute_cell(
            "FUNCTION Start() AS VOID\nPRINT value\nEND FUNCTION\n",
            bn="target/debug/bn",
        )
        self.assertEqual(first.returncode, 0)
        self.assertEqual(second.returncode, 1)


if __name__ == "__main__":
    unittest.main()
