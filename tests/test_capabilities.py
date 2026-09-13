import datetime
import json
import pathlib
import re
import shutil
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
    # Inspect the built archive: source regexes miss macro-generated exports.
    archive = ROOT / "target" / "debug" / "libbn_rt.a"
    nm = shutil.which("llvm-nm") or shutil.which("nm")
    if nm is None:
        raise RuntimeError("llvm-nm or nm is required for the ABI export gate")
    symbols = subprocess.run([nm, "-g", str(archive)], capture_output=True, text=True, check=True)
    return set(re.findall(r"\bT\s+_?(bn_rt_[A-Za-z0-9_]+)\b", symbols.stdout))


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
                self.assertRegex(program.get("reject_diag", ""), r"^TARGET_UNSUPPORTED_[A-Z_]+$")
                conditions = "\n".join(program["conditions"])
                for prefix in ("Owner:", "Defer-until:", "Risk:"):
                    self.assertRegex(
                        conditions,
                        rf"(?m)^{re.escape(prefix)} .+$",
                        program["path"],
                    )
                self.assertEqual(
                    program.get("build_diagnostic_contains"),
                    f"error[{program['reject_diag']}]",
                    program["path"],
                )

    def test_declared_capabilities_match_user_visible_commands(self):
        for program in self.manifest["programs"]:
            with self.subTest(program=program["path"]):
                path = ROOT / program["path"]
                checked = run([BN, "check", path])
                self.assertEqual(checked.returncode, 0, checked.stderr.decode())
                started = datetime.datetime.now(datetime.timezone.utc)
                arguments = program.get("args", [])
                interpreted = run([BN, "run", path, "--", *arguments], input=program.get("stdin", "").encode())
                finished = datetime.datetime.now(datetime.timezone.utc)
                expected_exit_code = program.get("exit_code", 0)
                self.assertIn(
                    interpreted.returncode,
                    program.get("run_exit_codes", [expected_exit_code]),
                    program["path"],
                )
                self.assertNotIn(b"FAIL", interpreted.stdout, program["path"])
                expected_fragment = program.get("run_stdout_contains")
                if expected_fragment:
                    self.assertIn(expected_fragment.encode(), interpreted.stdout)

                with tempfile.TemporaryDirectory() as directory:
                    artifact = pathlib.Path(directory) / "program"
                    built = run([BN, "build", path, "-o", artifact])
                    if program["support"] == "llvm-supported":
                        self.assertEqual(built.returncode, 0, built.stderr.decode())
                        native_started = datetime.datetime.now(datetime.timezone.utc)
                        compiled = run([artifact, *arguments], input=program.get("stdin", "").encode())
                        native_finished = datetime.datetime.now(datetime.timezone.utc)
                        self.assertIn(
                            compiled.returncode,
                            program.get("run_exit_codes", [program["exit_code"]]),
                            program["path"],
                        )
                        self.assertNotIn(b"FAIL", compiled.stdout, program["path"])
                        if program.get("observation") == "utc-clock":
                            for output, before, after in (
                                (interpreted.stdout, started, finished),
                                (compiled.stdout, native_started, native_finished),
                            ):
                                value = datetime.datetime.strptime(output.decode(), "Data:  %Y-%m-%d\nHora:  %H:%M:%S.%f\n").replace(tzinfo=datetime.timezone.utc)
                                self.assertLessEqual(before - datetime.timedelta(milliseconds=1), value)
                                self.assertLessEqual(value, after)
                        elif program.get("observation") == "language-tour":
                            outputs = (interpreted.stdout, compiled.stdout)
                            normalized = []
                            for output in outputs:
                                lines = output.splitlines()
                                self.assertEqual(len(lines), 15, output)
                                clock_fields = lines[5].split()
                                self.assertEqual(len(clock_fields), 7, lines[5])
                                self.assertGreater(int(clock_fields[5]), 0)
                                self.assertGreaterEqual(int(clock_fields[6]), 0)
                                clock_fields[5:] = [b"<timestamp>", b"<monotonic>"]
                                lines[5] = b" ".join(clock_fields)

                                argument_fields = lines[6].split(maxsplit=2)
                                self.assertEqual(argument_fields[0], b"1")
                                self.assertTrue(argument_fields[1])
                                argument_fields[1] = b"<program>"
                                lines[6] = b" ".join(argument_fields)

                                temporal_fields = lines[7].split()
                                self.assertEqual(len(temporal_fields), 2)
                                self.assertIn(int(temporal_fields[0]), range(24))
                                self.assertIn(int(temporal_fields[1]), range(1, 8))  # ISO weekday Mon=1..Sun=7
                                lines[7] = b"<derived-temporal>"
                                normalized.append(lines)
                            self.assertEqual(normalized[0], normalized[1], program["path"])
                        elif program.get("observation") == "unordered-prefix-lines":
                            interpreted_lines = interpreted.stdout.splitlines()
                            compiled_lines = compiled.stdout.splitlines()
                            self.assertEqual(len(interpreted_lines), len(compiled_lines), program["path"])
                            self.assertEqual(interpreted_lines[-1], compiled_lines[-1], program["path"])
                            self.assertEqual(
                                sorted(interpreted_lines[:-1]),
                                sorted(compiled_lines[:-1]),
                                program["path"],
                            )
                        elif program.get("observation") == "environment-dependent-network":
                            self.assertTrue(interpreted.stdout, program["path"])
                            self.assertTrue(compiled.stdout, program["path"])
                        else:
                            self.assertEqual(compiled.stdout, interpreted.stdout, program["path"])
                        if "stdout" in program:
                            self.assertEqual(interpreted.stdout, program["stdout"].encode(), program["path"])
                        if expected_fragment:
                            self.assertIn(expected_fragment.encode(), compiled.stdout)
                    else:
                        self.assertNotEqual(built.returncode, 0, program["path"])
                        diagnostic = built.stderr.decode()
                        self.assertIn(program["build_diagnostic_contains"], diagnostic)
                        self.assertEqual(set(re.findall(r"error\[([^]]+)\]", diagnostic)), {program["reject_diag"]})

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
