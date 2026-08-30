import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).parents[1]
BN = ROOT / "target" / "debug" / "bn"


class CompilerParityTests(unittest.TestCase):
    def test_supported_constant_programs_match_interpreter(self):
        fixtures = (
            "examples/hello.bn",
            "print-integer.bn",
            "print-const.bn",
            "print-expression.bn",
            "print-call.bn",
            "print-call-local.bn",
            "print-call-nested.bn",
            "print-predicate-call.bn",
            "print-string-call.bn",
            "print-float.bn",
            "print-string.bn",
            "print-comparison.bn",
            "print-variable.bn",
            "print-args-length.bn",
            "print-if-constant.bn",
            "print-if-boolean-expression.bn",
            "print-if-or.bn",
            "print-while-false.bn",
            "build-euclidean-div.bn",
            "build-euclidean-rem.bn",
            "build-float-one.bn",
            "build-print-same-value.bn",
        )
        for fixture in fixtures:
            path = ROOT / fixture if fixture.startswith("examples/") else ROOT / "tests" / "grammar" / "valid" / fixture
            interpreted = subprocess.run([BN, "run", path], capture_output=True, check=False)
            with tempfile.TemporaryDirectory() as directory:
                artifact = pathlib.Path(directory) / "program"
                built = subprocess.run(
                    [BN, "build", path, "-o", artifact], capture_output=True, check=False
                )
                self.assertEqual(built.returncode, 0, fixture)
                compiled = subprocess.run([artifact], capture_output=True, check=False)
            self.assertEqual(compiled.returncode, interpreted.returncode, fixture)
            self.assertEqual(compiled.stdout, interpreted.stdout, fixture)

    def test_input_program_matches_interpreter(self):
        path = ROOT / "tests" / "grammar" / "valid" / "build-input.bn"
        input_data = b"hello\r\n"
        interpreted = subprocess.run([BN, "run", path], input=input_data, capture_output=True, check=False)
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run(
                [BN, "build", path, "-o", artifact], capture_output=True, check=False
            )
            self.assertEqual(built.returncode, 0)
            compiled = subprocess.run([artifact], input=input_data, capture_output=True, check=False)
        self.assertEqual(compiled.returncode, interpreted.returncode)
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_seeded_random_program_matches_interpreter(self):
        path = ROOT / "tests" / "grammar" / "valid" / "host-random.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, check=False)
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run(
                [BN, "build", path, "-o", artifact], capture_output=True, check=False
            )
            self.assertEqual(built.returncode, 0)
            compiled = subprocess.run([artifact], capture_output=True, check=False)
        self.assertEqual(compiled.returncode, interpreted.returncode)
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_seeded_random_sequence_matches_interpreter(self):
        path = ROOT / "tests" / "grammar" / "valid" / "host-random-twice.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, check=False)
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run(
                [BN, "build", path, "-o", artifact], capture_output=True, check=False
            )
            self.assertEqual(built.returncode, 0)
            compiled = subprocess.run([artifact], capture_output=True, check=False)
        self.assertEqual(compiled.returncode, interpreted.returncode)
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_seeded_random_branch_matches_interpreter(self):
        path = ROOT / "tests" / "grammar" / "valid" / "build-random-branch.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, check=False)
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, check=False)
            self.assertEqual(built.returncode, 0)
            compiled = subprocess.run([artifact], capture_output=True, check=False)
        self.assertEqual(compiled.returncode, interpreted.returncode)
        self.assertEqual(compiled.stdout, interpreted.stdout)


if __name__ == "__main__":
    unittest.main()
