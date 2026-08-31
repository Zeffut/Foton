#!/usr/bin/env python3
"""Measure how much of vanilla Foton actually implements.

Cross-references `foton-core/build/classes.json`, which maps every vanilla
registry entry to the Java class that backs it, against the structs carrying a
`#[block_behavior]`, `#[item_behavior]` or `#[entity_behavior]` attribute.

An entity declares the class it stands for explicitly (`class = "Cow"`); a block
or item is matched on the struct name, which is how the codegen resolves it too.

Usage: python3 dev/coverage.py [--list <blocks|items|entities>]
"""

import argparse
import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
CLASSES = REPO / "foton-core" / "build" / "classes.json"
SOURCES = REPO / "foton-core" / "src"

STRUCT = re.compile(
    # The attribute may be written bare or path-qualified
    # (`#[foton_macros::item_behavior]`); the codegen accepts both.
    r"#\[(?:[A-Za-z_][A-Za-z0-9_]*::)*(block|item|entity)_behavior([^\]]*)\]"
    r"\s*(?:(?://[^\n]*|///[^\n]*)\n\s*)*"
    r"(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)"
)
EXPLICIT_CLASS = re.compile(r'class\s*=\s*"([^"]+)"')

SECTIONS = (("blocks", "block"), ("items", "item"), ("entities", "entity"))


def implemented_classes():
    """Returns the vanilla class names Foton has a behavior for, by kind."""
    found = {"block": set(), "item": set(), "entity": set()}
    for path in SOURCES.rglob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="ignore")
        for kind, args, name in STRUCT.findall(text):
            explicit = EXPLICIT_CLASS.search(args)
            found[kind].add(explicit.group(1) if explicit else name)
    return found


def counts():
    """Coverage per section: how many classes are covered, and which are not."""
    if not CLASSES.is_file():
        raise SystemExit(f"missing {CLASSES}")
    try:
        registry = json.loads(CLASSES.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"{CLASSES} is not valid JSON: {exc}") from exc
    found = implemented_classes()
    out = {}
    for section, kind in SECTIONS:
        if section not in registry:
            raise SystemExit(f"{CLASSES} is missing the {section!r} section")
        classes = {entry["class"] for entry in registry[section]}
        covered = sorted(name for name in classes if name in found[kind])
        out[section] = {
            "covered": len(covered),
            "total": len(classes),
            "missing": sorted(classes - set(covered)),
            "covered_names": covered,
        }
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--list",
        choices=[section for section, _ in SECTIONS],
        help="also print the covered and missing class names for one section",
    )
    args = parser.parse_args()

    data = counts()
    for section, _ in SECTIONS:
        entry = data[section]
        percent = 100 * entry["covered"] / max(entry["total"], 1)
        print(f"{section:9} {entry['covered']:4} / {entry['total']:4} classes  ({percent:.0f} %)")

        if args.list == section:
            print(f"\n  covered ({len(entry['covered_names'])}):\n    "
                  + "\n    ".join(entry["covered_names"]))
            print(f"\n  missing ({len(entry['missing'])}):\n    "
                  + "\n    ".join(entry["missing"]))

    return 0


if __name__ == "__main__":
    sys.exit(main())
