import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).parents[1]
BN = ROOT / "target" / "debug" / "bn"
HOST = ROOT / "bin" / "bn-wasm"
FIXTURES = ROOT / "tests" / "grammar" / "valid"


class WasmParityTests(unittest.TestCase):
    def assert_parity(self, fixture, input_data=b""):
        source = FIXTURES / fixture
        interpreted = subprocess.run(
            [BN, "run", source], input=input_data, capture_output=True, check=False
        )
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program.wasm"
            built = subprocess.run(
                [BN, "build", "--target", "wasm32", source, "-o", artifact],
                capture_output=True,
                check=False,
            )
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            compiled = subprocess.run(
                ["node", HOST, artifact],
                input=input_data,
                capture_output=True,
                check=False,
            )
        self.assertEqual(compiled.returncode, interpreted.returncode, fixture)
        self.assertEqual(compiled.stdout, interpreted.stdout, fixture)

    def test_non_tty_subset_matches_interpreter(self):
        for fixture in (
            "empty-start.bn",
            "print-integer.bn",
            "print-float.bn",
            "print-string.bn",
            "host-random.bn",
            "host-random-twice.bn",
            "print-args-length.bn",
        ):
            with self.subTest(fixture=fixture):
                self.assert_parity(fixture)

    def test_input_matches_interpreter(self):
        self.assert_parity("build-input.bn", b"hello\r\n")
        self.assert_parity("build-input.bn", b"x" * 5000 + b"\n")


if __name__ == "__main__":
    unittest.main()
