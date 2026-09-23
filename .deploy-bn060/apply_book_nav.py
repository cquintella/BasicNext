#!/usr/bin/env python3
"""Apply prev/next chapter navigation under docs/book/en/ (toc order). Idempotent."""
from pathlib import Path
import re
import sys

ORDER = [
    "toc.md",
    "00_preface.md",
    "01_introduction.md",
    "02_common_programming_concepts.md",
    "03_control_flow.md",
    "04_compound_data.md",
    "05_functions_and_program_structure.md",
    "06_object_oriented_features.md",
    "07_memory_management.md",
    "08_standard_library_and_host.md",
    "09_io_and_concurrency.md",
    "17_architecture.md",
    "10_appendices.md",
    "11_bnjson.md",
    "12_bnlog.md",
    "13_bnweb.md",
    "14_bndata.md",
    "15_external_modules.md",
    "16_bndispatch.md",
]

SHORT = {"00_preface.md": "Introducing Basic Next"}

BOTTOM_RE = re.compile(
    r"\n---\n\n\[(?:Next:[^\]]*|Contents →)\]\([^)]+\)\s*\Z"
)

def title_of(path: Path) -> str:
    if path.name in SHORT:
        return SHORT[path.name]
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.startswith("# "):
            t = line[2:].strip()
            return (t[:57].rstrip() + "…") if len(t) > 60 else t
    return path.stem

def strip_nav(text: str) -> str:
    lines = text.splitlines(keepends=True)
    hi = next((i for i, l in enumerate(lines) if l.startswith("# ")), None)
    if hi is not None:
        j = hi + 1
        if j < len(lines) and lines[j].strip() == "":
            k = j + 1
            if k < len(lines) and (
                "← Previous:" in lines[k]
                or lines[k].startswith("[Contents](toc.md)")
            ):
                del lines[j : k + 1]
        elif j < len(lines) and (
            "← Previous:" in lines[j] or lines[j].startswith("[Contents](toc.md)")
        ):
            del lines[j]
    body = "".join(lines)
    body = BOTTOM_RE.sub("", body)
    return body.rstrip("\n") + "\n"

def main(book: Path) -> None:
    titles = {f: title_of(book / f) for f in ORDER}
    titles["toc.md"] = "Contents"
    for idx, fname in enumerate(ORDER):
        path = book / fname
        body = strip_nav(path.read_text(encoding="utf-8"))
        lines = body.splitlines(keepends=True)
        hi = next(i for i, l in enumerate(lines) if l.startswith("# "))
        if fname == "toc.md":
            top = None
        elif fname == "00_preface.md" or ORDER[idx - 1] == "toc.md":
            top = "\n[Contents](toc.md)\n"
        else:
            prev = ORDER[idx - 1]
            top = f"\n[← Previous: {titles[prev]}]({prev}) · [Contents](toc.md)\n"
        if fname == "toc.md":
            nxt = ORDER[idx + 1]
            bottom = f"\n---\n\n[Next: {titles[nxt]} →]({nxt})\n"
        elif fname == "16_bndispatch.md":
            bottom = "\n---\n\n[Contents →](toc.md)\n"
        else:
            nxt = ORDER[idx + 1]
            bottom = f"\n---\n\n[Next: {titles[nxt]} →]({nxt})\n"
        if top is not None:
            lines.insert(hi + 1, top)
        path.write_text("".join(lines).rstrip("\n") + "\n" + bottom, encoding="utf-8")
        print("updated", path)

if __name__ == "__main__":
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "/Users/caq/src/BasicNext/docs/book/en")
    main(root)
