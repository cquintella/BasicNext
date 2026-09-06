import pathlib
import subprocess
import sys
import tempfile
import time
import unittest

from scripts.differential_runner import run


class DifferentialRunnerTests(unittest.TestCase):
    def test_failure_retains_actionable_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            old = __import__("os").environ.get("BN_FAILURE_ARTIFACT_DIR")
            __import__("os").environ["BN_FAILURE_ARTIFACT_DIR"] = directory
            try:
                result = run([sys.executable, "-c", "print('out'); raise SystemExit(3)"])
            finally:
                if old is None:
                    __import__("os").environ.pop("BN_FAILURE_ARTIFACT_DIR", None)
                else:
                    __import__("os").environ["BN_FAILURE_ARTIFACT_DIR"] = old
            self.assertEqual(result.returncode, 3)
            reports = list(pathlib.Path(directory).glob("*.json"))
            self.assertEqual(len(reports), 1)
            self.assertIn('"status": "failed"', reports[0].read_text())

    def test_timeout_is_bounded(self):
        started = time.monotonic()
        with self.assertRaises(subprocess.TimeoutExpired):
            run([sys.executable, "-c", "import time; time.sleep(2)"], timeout=0.05)
        self.assertLess(time.monotonic() - started, 1.0)


if __name__ == "__main__":
    unittest.main()
