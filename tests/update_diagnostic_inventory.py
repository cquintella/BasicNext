#!/usr/bin/env python3
"""Generate/check the exact 0.5.1 diagnostic emit-site inventory."""

import argparse
import json
import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).parents[1]
OUTPUT = ROOT / "diagnostics" / "sites.json"
PATTERNS = (
    ("literal-call", re.compile(r"(?:runtime_error|heap_error|temporal_error)\(\s*\"(?P<id>[A-Z][A-Z0-9_]+)\"")),
    ("literal-field", re.compile(r"code:\s*\"(?P<id>[A-Z][A-Z0-9_]+)\"")),
    ("printed-error", re.compile(r"eprintln!\(\s*\"(?P<printed>error[^\"]*)\"")),
    ("lexical", re.compile(r"Diagnostic::lexical\(")),
    ("typed-sink", re.compile(r"\.emit\(\s*DiagId::(?P<id>[A-Za-z0-9_]+)")),
    ("structured-call", re.compile(r"Diagnostic::structured\(\s*(?:[A-Za-z0-9_]+::)*DiagId::(?P<id>[A-Za-z0-9_]+)")),
    ("structured-call", re.compile(r"Diagnostic::lexical_facts\(")),
    ("parse-facts", re.compile(r"Diagnostic::parse_facts\(")),
    ("duplicate-facts", re.compile(r"duplicate_name\(")),
    ("name-facts", re.compile(r"undefined_name\(")),
    ("numeric-facts", re.compile(r"numeric_overflow\(")),
    ("type-mismatch-facts", re.compile(r"(?<!fn )\btype_mismatch\(")),
    ("semantic-call", re.compile(r"(?<!fn )\berror\(\s*\"(?P<id>[A-Z][A-Z0-9_]+)\"")),
)
PROTOCOL_LABELS = {"DAP", "LSP"}


DIAG_VARIANT_CODES = {
    "DiagId::Lexical": "E0001",
    "DiagId::Parse": "E0100",
    "DiagId::NumericOverflow": "NUMERIC_OVERFLOW",
    "DiagId::TypeMismatch": "TYPE_MISMATCH",
    "DiagId::TypeMismatch": "TYPE_MISMATCH",
    "DiagId::UnusedBinding": "UNUSED_BINDING",
    "DiagId::UnusedImport": "UNUSED_IMPORT",
    "DiagId::UnreachableCode": "UNREACHABLE_CODE",
    "DiagId::ModuleNotFound": "MODULE_NOT_FOUND",
}


def catalog_index() -> dict[str, str]:
    index = {}
    directory = ROOT / "share" / "bn" / "diagnostics" / "en-US"
    for path in directory.glob("*.ftl"):
        for code in re.findall(r"^\s*\.code\s*=\s*([A-Z][A-Z0-9_]+)\s*$", path.read_text(encoding="utf-8"), re.MULTILINE):
            index[code] = path.relative_to(ROOT).as_posix()
    return index


def catalog_for(identity: str, producer: pathlib.Path) -> str:
    identity = DIAG_VARIANT_CODES.get(identity, identity)
    if identity in {
        "NumericOverflow",
        "DiagId::NumericOverflow",
        "TypeMismatch",
        "DiagId::TypeMismatch",
    }:
        identity = "TYPE_MISMATCH" if "TypeMismatch" in identity else "NUMERIC_OVERFLOW"
    del producer
    return catalog_index().get(identity, "share/bn/diagnostics/en-US/runtime.ftl")


def consumer_for(path: pathlib.Path) -> list[str]:
    relative = path.as_posix()
    if relative.startswith("crates/bn_frontend/"):
        return ["src/main.rs", "src/lsp.rs"]
    return ["src/main.rs"]


def collect() -> list[dict[str, object]]:
    sites = []
    for base in (ROOT / "src", ROOT / "crates"):
        for path in sorted(base.rglob("*.rs")):
            relative = path.relative_to(ROOT)
            if "tests" in relative.parts:
                continue
            text = strip_cfg_test_modules(path.read_text(encoding="utf-8"))
            for kind, pattern in PATTERNS:
                for match in pattern.finditer(text):
                    line_start = text.rfind("\n", 0, match.start()) + 1
                    line_text = text[line_start : text.find("\n", match.start())]
                    if kind in {"duplicate-facts", "name-facts", "numeric-facts", "type-mismatch-facts"} and re.search(
                        r"\bfn\s+(duplicate_name|undefined_name|numeric_overflow|type_mismatch)\s*\(", line_text
                    ):
                        continue
                    identity = match.groupdict().get("id") or "E0001"
                    if kind == "structured-call" and identity == "Runtime":
                        runtime_code = re.search(
                            r'DiagId::Runtime\("([A-Z][A-Z0-9_]+)"\)',
                            text[match.start() :],
                        )
                        if runtime_code:
                            identity = runtime_code.group(1)
                            kind = "structured-runtime"
                    if kind == "structured-call" and match.groupdict().get("id") is None:
                        identity = "Lexical"
                    elif kind == "parse-facts":
                        identity = "Parse"
                    elif kind == "duplicate-facts":
                        identity = "DUPLICATE_NAME"
                    elif kind == "name-facts":
                        identity = "NAME_NOT_FOUND"
                    elif kind == "numeric-facts":
                        identity = "NUMERIC_OVERFLOW"
                    elif kind == "type-mismatch-facts":
                        identity = "TYPE_MISMATCH"
                    if kind == "structured-runtime" and identity == "Runtime":
                        runtime_code = re.search(
                            r'DiagId::Runtime\("([A-Z][A-Z0-9_]+)"\)',
                            text[match.start() :],
                        )
                        if runtime_code:
                            identity = runtime_code.group(1)
                    if kind == "printed-error":
                        printed = match.group("printed")
                        literal = re.search(r"\[([A-Z][A-Z0-9_]+)\]", printed)
                        identity = literal.group(1) if literal else (
                            "legacy:dynamic" if "[" in printed else "legacy:uncoded"
                        )
                    if identity in PROTOCOL_LABELS:
                        continue
                    if kind in {"typed-sink", "structured-call", "parse-facts"}:
                        identity = f"DiagId::{identity}"
                    sites.append(
                        {
                            "path": relative.as_posix(),
                            "line": text.count("\n", 0, match.start()) + 1,
                            "kind": kind,
                            "identity": identity,
                            "catalog": None if identity.startswith("legacy:") else catalog_for(identity, relative),
                            "consumers": [relative.as_posix()] if kind == "printed-error" else consumer_for(relative),
                            "state": "structured" if kind in {"typed-sink", "structured-call", "parse-facts", "structured-runtime", "duplicate-facts", "name-facts", "numeric-facts", "type-mismatch-facts"} or (kind == "semantic-call" and identity == "TYPE_MISMATCH") else "legacy",
                        }
                    )
    return sorted(sites, key=lambda site: (site["path"], site["line"], site["kind"]))


def strip_cfg_test_modules(text: str) -> str:
    """Blank inline #[cfg(test)] items while retaining original line numbers."""
    cursor = 0
    output = list(text)
    marker = "#[cfg(test)]"
    while (start := text.find(marker, cursor)) >= 0:
        brace = text.find("{", start + len(marker))
        if brace < 0:
            break
        depth = 0
        end = brace
        while end < len(text):
            if text[end] == "{":
                depth += 1
            elif text[end] == "}":
                depth -= 1
                if depth == 0:
                    end += 1
                    break
            end += 1
        for index in range(start, min(end, len(output))):
            if output[index] != "\n":
                output[index] = " "
        cursor = end
    return "".join(output)


def encoded() -> str:
    return json.dumps({"version": 1, "sites": collect()}, indent=2, ensure_ascii=False) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    current = OUTPUT.read_text(encoding="utf-8") if OUTPUT.exists() else ""
    expected = encoded()
    if args.check:
        if current != expected:
            print("diagnostics/sites.json is stale; run tests/update_diagnostic_inventory.py", file=sys.stderr)
            return 1
        return 0
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_text(expected, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
