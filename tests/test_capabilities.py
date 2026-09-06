import json
import pathlib
import re
import subprocess
import tempfile
import unittest

from scripts.differential_runner import run


ROOT = pathlib.Path(__file__).parents[1]
BN = ROOT / "target" / "debug" / "bn"
MANIFEST = ROOT / "tests" / "compiler-capabilities.json"
LLVM_RUNTIME_DECLARATIONS = (
    ROOT / "crates" / "bn_llvm" / "src" / "llvm" / "runtime.rs",
    ROOT / "crates" / "bn_llvm" / "src" / "llvm" / "math.rs",
)
ABI_CONTRACT = ROOT / "docs" / "architecture" / "value-memory-abi.md"


def llvm_declared_runtime_symbols():
    return {
        symbol
        for declaration in LLVM_RUNTIME_DECLARATIONS
        for symbol in re.findall(r"@(?P<symbol>bn_rt_[A-Za-z0-9_]+)", declaration.read_text())
    }


def runtime_exported_symbols():
    export_pattern = re.compile(r'pub extern "C" fn (?P<symbol>bn_rt_[A-Za-z0-9_]+)')
    return {
        symbol
        for source in (ROOT / "crates" / "bn_rt" / "src").rglob("*.rs")
        for symbol in export_pattern.findall(source.read_text())
    }


def abi_documented_symbols():
    contract = ABI_CONTRACT.read_text()
    marker = "```text llvm-emitted-bn-rt-symbols\n"
    start = contract.find(marker)
    if start == -1:
        return set()
    end = contract.find("\n```", start + len(marker))
    if end == -1:
        return set()
    return set(re.findall(r"\bbn_rt_[A-Za-z0-9_]+\b", contract[start:end]))


class CompilerCapabilityTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))

    def test_manifest_has_valid_paths_and_support_labels(self):
        self.assertEqual(self.manifest.get("schema_version"), 1)
        programs = self.manifest["programs"]
        self.assertTrue(programs)
        self.assertEqual(len({program["path"] for program in programs}), len(programs))
        for program in programs:
            path = ROOT / program["path"]
            self.assertTrue(path.is_file(), program["path"])
            self.assertIn(program["support"], {"llvm-supported", "llvm-deferred"})
            for field in (
                "id",
                "target",
                "op",
                "ir_instructions",
                "type_constraints",
                "conditions",
                "tests",
            ):
                self.assertIn(field, program, program["path"])
            self.assertEqual(program["target"], "llvm-native")
            self.assertNotIn("program.fixture", program["op"])
            self.assertNotIn("fixture-defined", program["type_constraints"])
            self.assertNotIn("fixture-defined", program["conditions"])
            self.assertIn(program.get("provider"), {"language", "bn_rt"})
            self.assertEqual(program.get("evidence"), "fixture-exact")
            self.assertIsInstance(program["tests"], list)
            self.assertTrue(program["tests"], program["path"])
            self.assertIsInstance(program["ir_instructions"], list)
            self.assertTrue(program["ir_instructions"], program["path"])
            self.assertEqual(
                program["ir_instructions"],
                sorted(set(program["ir_instructions"])),
                program["path"],
            )
            if program["support"] == "llvm-deferred":
                self.assertRegex(program.get("reject_diag", ""), r"^(BUILD_|TARGET_UNSUPPORTED_)")

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

    def test_catalogued_ir_inventory_matches_lowered_fixture(self):
        """Keep the matrix tied to the IR artifact, not only command outcomes."""
        instruction_pattern = re.compile(r"^\s{24}([A-Z][A-Za-z0-9]*) \{", re.MULTILINE)
        for program in self.manifest["programs"]:
            with self.subTest(program=program["path"]):
                emitted = run([BN, "check", "--emit", "ir", ROOT / program["path"]])
                self.assertEqual(emitted.returncode, 0, emitted.stderr.decode())
                actual = sorted(set(instruction_pattern.findall(emitted.stdout.decode())))
                self.assertEqual(actual, program["ir_instructions"], program["path"])

    def test_support_matrix_gap_report_covers_the_ir_and_target_space(self):
        report = subprocess.run(
            ["python3", "scripts/support_matrix_report.py", "--json"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        inventory = json.loads(report.stdout)
        self.assertEqual(inventory["schema_version"], 1)
        self.assertEqual(
            inventory["inventory_count"],
            inventory["instruction_count"]
            * inventory["type_count"]
            * len(inventory["targets"]),
        )
        self.assertGreater(inventory["gap_count"], 0)
        self.assertGreater(inventory["covered_count"], 0)
        print_instruction = next(
            entry
            for entry in inventory["inventory"]
            if entry["instruction"] == "Print"
            and entry["type"] == "Pointer"
            and entry["target"] == "llvm-native"
        )
        self.assertEqual(print_instruction["evidence"], [])

    def test_llvm_declared_symbols_have_runtime_exports_and_abi_groups(self):
        declared = llvm_declared_runtime_symbols()
        exported = runtime_exported_symbols()
        documented = abi_documented_symbols()

        self.assertTrue(declared)
        self.assertEqual(sorted(declared - exported), [])
        self.assertEqual(sorted(declared - documented), [])


if __name__ == "__main__":
    unittest.main()
