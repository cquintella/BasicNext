#!/usr/bin/env python3
"""Generate the machine-readable support-matrix inventory and gap report."""

import argparse
import json
import re
from pathlib import Path
from typing import Any


ROOT = Path(__file__).parents[1]
INSTRUCTION_MODEL = ROOT / "crates" / "bn_ir" / "src" / "model.rs"
TYPE_MODEL = ROOT / "crates" / "bn_types" / "src" / "lib.rs"
CATALOG = ROOT / "tests" / "compiler-capabilities.json"


def enum_variants(path: Path, enum_name: str) -> list[str]:
    source = path.read_text(encoding="utf-8")
    match = re.search(rf"pub enum {enum_name}\s*\{{(?P<body>.*?)\n\}}", source, re.DOTALL)
    if match is None:
        raise RuntimeError(f"could not find enum {enum_name} in {path}")
    variants: list[str] = []
    for line in match.group("body").splitlines():
        candidate = re.match(r"\s{4}([A-Z][A-Za-z0-9_]*)\b", line)
        if candidate is not None:
            variants.append(candidate.group(1))
    if not variants:
        raise RuntimeError(f"enum {enum_name} in {path} has no variants")
    return variants


def constraint_matches(constraint: str, type_name: str) -> bool:
    """Map catalog type classes to the structural Type enum categories."""
    normalized = constraint.removesuffix(" ABI")
    if type_name == "Integer":
        return normalized in {
            "INTEGER",
            "BYTE",
            "INT8",
            "INT16",
            "INT64",
            "UINT16",
            "UINT32",
            "UINT64",
        }
    if type_name == "Float":
        return normalized in {"FLOAT", "FLOAT32", "FLOAT64"}
    if type_name == "FloatLiteral":
        return normalized == "FLOAT_LITERAL"
    if type_name == "Vector":
        return "[" in normalized
    if type_name == "Boolean":
        return normalized == "BOOLEAN"
    if type_name == "String":
        return normalized == "STRING"
    if type_name == "EndOfFile":
        return normalized == "EOF"
    if type_name == "HostNet":
        return normalized == "HOST.Net"
    if type_name == "Named":
        return normalized in {"TIMESTAMP", "DATE", "TIME"}
    return False


def build_report() -> dict[str, Any]:
    catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    rows = catalog["programs"]
    instructions = enum_variants(INSTRUCTION_MODEL, "Instruction")
    types = enum_variants(TYPE_MODEL, "Type")
    targets = ["interpret", "llvm-native", "wasm32"]

    evidence: dict[tuple[str, str, str], list[str]] = {}
    for row in rows:
        for instruction in row["ir_instructions"]:
            for type_name in types:
                if any(
                    constraint_matches(constraint, type_name)
                    for constraint in row["type_constraints"]
                ):
                    key = (instruction, type_name, row["target"])
                    evidence.setdefault(key, []).append(row["id"])

    inventory = [
        {
            "instruction": instruction,
            "type": type_name,
            "target": target,
            "evidence": evidence.get((instruction, type_name, target), []),
        }
        for instruction in instructions
        for type_name in types
        for target in targets
    ]
    gaps = [entry for entry in inventory if not entry["evidence"]]
    return {
        "schema_version": 1,
        "source": {
            "instruction_model": str(INSTRUCTION_MODEL.relative_to(ROOT)),
            "type_model": str(TYPE_MODEL.relative_to(ROOT)),
            "catalog": str(CATALOG.relative_to(ROOT)),
        },
        "targets": targets,
        "instruction_count": len(instructions),
        "type_count": len(types),
        "inventory_count": len(inventory),
        "covered_count": len(inventory) - len(gaps),
        "gap_count": len(gaps),
        "inventory": inventory,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="emit JSON instead of the summary")
    args = parser.parse_args()
    report = build_report()
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print(
            f"{report['inventory_count']} combinations; "
            f"{report['covered_count']} covered; {report['gap_count']} gaps"
        )


if __name__ == "__main__":
    main()
