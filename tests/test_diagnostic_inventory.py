import pathlib
import re
import unittest


ROOT = pathlib.Path(__file__).parents[1]
REGISTRY = ROOT / "crates" / "bn_diag" / "src" / "lib.rs"


class DiagnosticInventoryTests(unittest.TestCase):
    def test_emitted_stable_codes_are_registered(self):
        registry = REGISTRY.read_text(encoding="utf-8")
        runtime_start = registry.index("const RUNTIME_CODES")
        runtime_end = registry.index("];", runtime_start)
        registered = set(
            re.findall(r'"([A-Z][A-Z0-9_]+)"', registry[runtime_start:runtime_end])
        )
        registered.update(re.findall(r'Self::\w+ => "([A-Z][A-Z0-9_]+)"', registry))

        emitted = set()
        for root in (ROOT / "src", ROOT / "crates"):
            for path in root.rglob("*.rs"):
                text = path.read_text(encoding="utf-8")
                emitted.update(
                    re.findall(r'runtime_error\(\s*"([A-Z][A-Z0-9_]+)"', text)
                )
                emitted.update(
                    re.findall(r'code:\s*"([A-Z][A-Z0-9_]+)"', text)
                )
                emitted.update(
                    re.findall(r'error\[([A-Z][A-Z0-9_]+)\]', text)
                )

        # These are protocol labels, not diagnostic identities.
        emitted.difference_update({"DAP", "LSP"})
        self.assertEqual(sorted(emitted - registered), [])


if __name__ == "__main__":
    unittest.main()
