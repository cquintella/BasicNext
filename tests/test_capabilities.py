import json
import pathlib
import subprocess
import tempfile
import unittest

from scripts.differential_runner import run


ROOT = pathlib.Path(__file__).parents[1]
BN = ROOT / "target" / "debug" / "bn"
MANIFEST = ROOT / "tests" / "compiler-capabilities.json"


class CompilerCapabilityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))

    def test_manifest_has_valid_paths_and_support_labels(self):
        programs = self.manifest["programs"]
        self.assertTrue(programs)
        self.assertEqual(len({program["path"] for program in programs}), len(programs))
        for program in programs:
            path = ROOT / program["path"]
            self.assertTrue(path.is_file(), program["path"])
            self.assertIn(program["support"], {"llvm-supported", "llvm-deferred"})
            for field in ("id", "target", "op", "type_constraints", "conditions", "tests"):
                self.assertIn(field, program, program["path"])
            self.assertEqual(program["target"], "llvm-native")
            self.assertIsInstance(program["tests"], list)
            self.assertTrue(program["tests"], program["path"])
            if program["support"] == "llvm-deferred":
                self.assertRegex(program.get("reject_diag", ""), r"^BUILD_")

    def test_declared_capabilities_match_user_visible_commands(self):
        for program in self.manifest["programs"]:
            with self.subTest(program=program["path"]):
                path = ROOT / program["path"]
                checked = run([BN, "check", path])
                self.assertEqual(checked.returncode, 0, checked.stderr.decode())
                interpreted = run([BN, "run", path])
                expected_exit_code = program.get("exit_code", 0)
                self.assertEqual(interpreted.returncode, expected_exit_code, program["path"])
                expected_fragment = program.get("run_stdout_contains")
                if expected_fragment:
                    self.assertIn(expected_fragment.encode(), interpreted.stdout)

                with tempfile.TemporaryDirectory() as directory:
                    artifact = pathlib.Path(directory) / "program"
                    built = run([BN, "build", path, "-o", artifact])
                    if program["support"] == "llvm-supported":
                        self.assertEqual(built.returncode, 0, built.stderr.decode())
                        compiled = run([artifact])
                        self.assertEqual(compiled.returncode, program["exit_code"], program["path"])
                        self.assertEqual(compiled.stdout, program["stdout"].encode(), program["path"])
                    else:
                        self.assertNotEqual(built.returncode, 0, program["path"])
                        diagnostic = built.stderr.decode()
                        self.assertIn(program["build_diagnostic_contains"], diagnostic)


if __name__ == "__main__":
    unittest.main()
