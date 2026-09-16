import json
import pathlib
import re
import subprocess
import tomllib
import unittest


ROOT = pathlib.Path(__file__).parents[1]
REGISTRY = ROOT / "crates" / "bn_diag" / "src" / "lib.rs"
INVENTORY = ROOT / "diagnostics" / "inventory.toml"
SITES = ROOT / "diagnostics" / "sites.json"


class DiagnosticInventoryTests(unittest.TestCase):
    def test_named_producer_routes_are_explicit_and_live(self):
        inventory = tomllib.loads(INVENTORY.read_text(encoding="utf-8"))
        routes = inventory.get("route", [])
        self.assertGreater(len(routes), 0)

        names = set()
        for route in routes:
            with self.subTest(route=route.get("name")):
                self.assertEqual(
                    set(route),
                    {"name", "producer", "marker", "identity", "catalog", "consumer", "state"},
                )
                self.assertNotIn(route["name"], names)
                names.add(route["name"])
                producer = ROOT / route["producer"]
                self.assertTrue(producer.is_file(), producer)
                self.assertIn(route["marker"], producer.read_text(encoding="utf-8"))
                self.assertIn(route["state"], {"structured", "legacy"})
                self.assertTrue((ROOT / route["catalog"]).is_file())
                self.assertTrue((ROOT / route["consumer"]).is_file())
                if route["identity"] not in {"call-site", "DiagId"}:
                    self.assertIn(route["identity"], registered_codes())
                    self.assertIn(
                        f'.code = {route["identity"]}',
                        (ROOT / route["catalog"]).read_text(encoding="utf-8"),
                    )
    def test_exact_emit_site_inventory_is_current_and_registered(self):
        subprocess.run(
            ["python3", "tests/update_diagnostic_inventory.py", "--check"],
            cwd=ROOT,
            check=True,
        )
        sites = json.loads(SITES.read_text(encoding="utf-8"))["sites"]
        self.assertGreater(len(sites), 500)
        identities = {site["identity"] for site in sites}
        self.assertTrue(
            {"DOUBLE_RELEASE", "FUNCTION_NOT_FOUND", "USE_AFTER_RELEASE"}
            <= identities
        )
        registered = registered_codes()
        for site in sites:
            with self.subTest(path=site["path"], line=site["line"]):
                self.assertTrue((ROOT / site["path"]).is_file())
                if site["kind"] in {"duplicate-facts", "name-facts"}:
                    source_lines = (ROOT / site["path"]).read_text(encoding="utf-8").splitlines()
                    self.assertNotRegex(source_lines[site["line"] - 1], r"\bfn\s+(duplicate_name|undefined_name)\s*\(")
                if site["catalog"] is not None:
                    self.assertTrue((ROOT / site["catalog"]).is_file())
                if site["identity"].startswith("DiagId::"):
                    variant = site["identity"].split("::", 1)[1]
                    self.assertRegex(REGISTRY.read_text(encoding="utf-8"), rf"\b{re.escape(variant)}\b")
                for consumer in site["consumers"]:
                    self.assertTrue((ROOT / consumer).is_file())
                if not site["identity"].startswith("legacy:"):
                    identity = site["identity"]
                    identity_map = {
                            "Lexical": "E0001",
                            "Parse": "E0100",
                            "UnusedBinding": "UNUSED_BINDING",
                            "UnusedImport": "UNUSED_IMPORT",
                            "UnreachableCode": "UNREACHABLE_CODE",
                            "NumericOverflow": "NUMERIC_OVERFLOW",
                            "TypeMismatch": "TYPE_MISMATCH",
                            "IrLowering": "IR_LOWERING",
                            "InvalidIr": "INVALID_IR",
                            "ModuleNotFound": "MODULE_NOT_FOUND",
                        }
                    if identity.startswith("DiagId::"):
                        identity = identity_map.get(identity.split("::", 1)[1], identity)
                    else:
                        identity = identity_map.get(identity, identity)
                    self.assertIn(identity, registered)
                    self.assertIsNotNone(site["catalog"])
                    self.assertIn(
                        f'.code = {identity}',
                        (ROOT / site["catalog"]).read_text(encoding="utf-8"),
                    )

    def test_every_dynamic_constructor_family_has_a_named_route(self):
        routes = tomllib.loads(INVENTORY.read_text(encoding="utf-8"))["route"]
        routed = {route["producer"] for route in routes if route["identity"] == "call-site"}
        dynamic = set()
        patterns = (
            re.compile(r"fn\s+\w*error\([^)]*code:\s*&'static str"),
            re.compile(r"runtime_error\(error\.code\(\)"),
            re.compile(r'eprintln!\("error\[\{code\}\]'),
        )
        for root in (ROOT / "src", ROOT / "crates"):
            for path in root.rglob("*.rs"):
                text = path.read_text(encoding="utf-8")
                if any(pattern.search(text) for pattern in patterns):
                    dynamic.add(path.relative_to(ROOT).as_posix())
        self.assertEqual(dynamic - routed, set())

    def test_emitted_stable_codes_are_registered(self):
        registered = registered_codes()

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


def registered_codes():
    registry = REGISTRY.read_text(encoding="utf-8")
    runtime_start = registry.index("const RUNTIME_CODES")
    runtime_end = registry.index("];", runtime_start)
    registered = set(
        re.findall(r'"([A-Z][A-Z0-9_]+)"', registry[runtime_start:runtime_end])
    )
    registered.update(re.findall(r'Self::\w+ => "([A-Z][A-Z0-9_]+)"', registry))
    return registered


if __name__ == "__main__":
    unittest.main()
