import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).parents[1]
BN = ROOT / "target" / "debug" / "bn"


class CompilerParityTests(unittest.TestCase):
    def test_csv_audit_cases_match_native_and_interpreter(self):
        path = ROOT / "tests/grammar/valid/build-csv-audit.bn"
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            csv = pathlib.Path(directory) / "data.csv"
            for content, mode, expected in (
                ('"é,quoted",\n', "no-header", b"1 2\n\xc3\xa9,quoted\n"),
                ("", "no-header", b"0 0\n"),
                ("Column1,b\n", "header", b"0 2\n"),
                ("a,b\n1\n", "header", b"csv-error\n"),
                ("a,b\n1,2,3\n", "header", b"csv-error\n"),
                ("a,a\n1,2\n", "header", b"csv-error\n"),
                ('a\n"unfinished', "header", b"csv-error\n"),
            ):
                csv.write_text(content, encoding="utf-8")
                for command in ([BN, "run", path, "--", csv, mode], [artifact, csv, mode]):
                    result = subprocess.run(command, capture_output=True, timeout=10)
                    self.assertEqual(result.returncode, 0, result.stderr.decode())
                    self.assertEqual(result.stdout, expected, (content, command))

    def test_slice_rejects_huge_counts_without_materializing_indices(self):
        path = ROOT / "tests/grammar/valid/build-slice-audit.bn"
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            for command in ([BN, "run", path], [artifact]):
                result = subprocess.run(command, capture_output=True, timeout=5)
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assertEqual(result.stdout, b"TRUE\nTRUE\n20\n")

    def test_supported_constant_programs_match_interpreter(self):
        fixtures = (
            "examples/hello.bn",
            "print-integer.bn",
            "print-const.bn",
            "print-expression.bn",
            "print-float.bn",
            "print-string.bn",
            "print-comparison.bn",
            "print-variable.bn",
            "print-args-length.bn",
            "print-if-constant.bn",
            "print-while-false.bn",
            "build-print-same-value.bn",
            "build-euclidean-div.bn",
            "build-euclidean-rem.bn",
            "build-euclidean-runtime.bn",
            "build-power-shift.bn",
            "build-power-shift-runtime.bn",
            "build-widths.bn",
            "build-clock.bn",
            "cls-and-beep.bn",
            "print-call.bn",
            "print-call-nested.bn",
            "print-predicate-call.bn",
            "print-string-call.bn",
            "build-integer-error-compare.bn",
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

    def test_phi_predecessors_are_function_local(self):
        path = ROOT / "tests/grammar/valid/build-phi-function-isolation.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"TRUE\nFALSE\nTRUE\nFALSE\nTRUE\nFALSE\n")
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            compiled = subprocess.run([artifact], capture_output=True, timeout=30)
        self.assertEqual(compiled.returncode, 0, compiled.stderr.decode())
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_distance_examples_cover_input_and_eof(self):
        for fixture in ("edit_distance.bn", "levenshtein.bn"):
            path = ROOT / "examples" / fixture
            with self.subTest(fixture=fixture), tempfile.TemporaryDirectory() as directory:
                artifact = pathlib.Path(directory) / "program"
                built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
                self.assertEqual(built.returncode, 0, built.stderr.decode())
                for data, exit_code, suffix in (
                    (b"kitten\nsitting\n", 0, b"3\n"),
                    ("café\ncafe\n".encode(), 0, b"1\n"),
                    (b"EOF\nEOF\n", 0, b"0\n"),
                    (b"\n\n", 0, b"0\n"),
                    (b"", 1, b"Expected word 1.\n"),
                    (b"kitten\n", 1, b"Expected word 2.\n"),
                ):
                    with self.subTest(input=data):
                        interpreted = subprocess.run([BN, "run", path], input=data, capture_output=True, timeout=30)
                        compiled = subprocess.run([artifact], input=data, capture_output=True, timeout=30)
                        self.assertEqual(interpreted.returncode, exit_code, interpreted.stderr.decode())
                        self.assertTrue(interpreted.stdout.endswith(suffix), interpreted.stdout)
                        self.assertEqual(compiled.returncode, exit_code, compiled.stderr.decode())
                        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_input_eof_sentinel_is_distinct_from_string_contents(self):
        path = ROOT / "tests/grammar/valid/build-input-type-tests.bn"
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            for data, expected_exit, expected_stdout in (
                (b"", 1, b"EOF\n"),
                (b"EOF\n", 0, b"STRING\n"),
                ("café\n".encode(), 0, b"STRING\n"),
            ):
                with self.subTest(input=data):
                    interpreted = subprocess.run([BN, "run", path], input=data, capture_output=True, timeout=30)
                    compiled = subprocess.run([artifact], input=data, capture_output=True, timeout=30)
                    self.assertEqual(interpreted.returncode, expected_exit, interpreted.stderr.decode())
                    self.assertEqual(interpreted.stdout, expected_stdout)
                    self.assertEqual(compiled.returncode, interpreted.returncode, compiled.stderr.decode())
                    self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_nullable_integer_type_tests_inspect_the_runtime_tag(self):
        path = ROOT / "tests/grammar/valid/build-nullable-integer-type-tests.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"TRUE\nFALSE\nFALSE\nTRUE\n")
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            compiled = subprocess.run([artifact], capture_output=True, timeout=30)
        self.assertEqual(compiled.returncode, 0, compiled.stderr.decode())
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_indexed_struct_and_static_fields_preserve_language_semantics(self):
        path = ROOT / "tests/grammar/valid/indexed-member-assignment.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"7\n9\n")
        built = subprocess.run([BN, "build", path], capture_output=True, timeout=30)
        self.assertNotEqual(built.returncode, 0)
        diagnostics = built.stderr.decode()
        self.assertIn("error[TARGET_UNSUPPORTED_OP]", diagnostics)
        self.assertNotIn("INVALID_IR", diagnostics)

    def test_inherited_fields_have_one_complete_native_object_layout(self):
        path = ROOT / "tests/grammar/valid/build-inherited-field-layout.bn"
        emitted = subprocess.run([BN, "build", path], capture_output=True, timeout=30)
        self.assertEqual(emitted.returncode, 0, emitted.stderr.decode())
        self.assertIn(b"call ptr @calloc(i64 1, i64 24)", emitted.stdout)
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"17\n23\n31\n")
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            compiled = subprocess.run([artifact], capture_output=True, timeout=30)
        self.assertEqual(compiled.returncode, 0, compiled.stderr.decode())
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_struct_fields_have_distinct_layout_and_bounded_native_lifetime(self):
        path = ROOT / "tests/grammar/valid/build-struct-layout-lifetime.bn"
        emitted = subprocess.run([BN, "build", path], capture_output=True, timeout=30)
        self.assertEqual(emitted.returncode, 0, emitted.stderr.decode())
        self.assertIn(b"call ptr @calloc(i64 1, i64 16)", emitted.stdout)
        self.assertIn(b"getelementptr i8, ptr %fieldobj", emitted.stdout)
        self.assertIn(b"i32 8", emitted.stdout)
        self.assertIn(b"i32 12", emitted.stdout)
        self.assertIn(b"call void @free(ptr %structfree", emitted.stdout)
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"17 23\n")
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            compiled = subprocess.run([artifact], capture_output=True, timeout=30)
        self.assertEqual(compiled.returncode, 0, compiled.stderr.decode())
        self.assertEqual(compiled.stdout, interpreted.stdout)

    def test_struct_return_without_ownership_transfer_fails_closed(self):
        path = ROOT / "tests/grammar/valid/struct-return-lifetime-deferred.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"ok\n")
        built = subprocess.run([BN, "build", path], capture_output=True, timeout=30)
        self.assertNotEqual(built.returncode, 0)
        diagnostics = built.stderr.decode()
        self.assertIn("error[TARGET_UNSUPPORTED_OP]", diagnostics)
        self.assertIn("STRUCT default allocation requires an acyclic Start lifetime", diagnostics)
        self.assertNotIn("INVALID_IR", diagnostics)

    def test_multidimensional_vector_matches_interpreter(self):
        path = ROOT / "tests/grammar/valid/multidimensional-vectors.bn"
        interpreted = subprocess.run([BN, "run", path], capture_output=True, timeout=30)
        self.assertEqual(interpreted.returncode, 0, interpreted.stderr.decode())
        self.assertEqual(interpreted.stdout, b"9\n")
        with tempfile.TemporaryDirectory() as directory:
            artifact = pathlib.Path(directory) / "program"
            built = subprocess.run([BN, "build", path, "-o", artifact], capture_output=True, timeout=30)
            self.assertEqual(built.returncode, 0, built.stderr.decode())
            compiled = subprocess.run([artifact], capture_output=True, timeout=30)
        self.assertEqual(compiled.returncode, interpreted.returncode, compiled.stderr.decode())
        self.assertEqual(compiled.stdout, interpreted.stdout)

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
