#!/usr/bin/env python3
"""Measure how much of vanilla Steel actually implements.

Cross-references `steel-core/build/classes.json`, which maps every vanilla
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
CLASSES = REPO / "steel-core" / "build" / "classes.json"
SOURCES = REPO / "steel-core" / "src"

STRUCT = re.compile(
    r"#\[(block|item|entity)_behavior([^\]]*)\]"
    r"\s*(?:(?://[^\n]*|///[^\n]*)\n\s*)*"
    r"(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)"
)
EXPLICIT_CLASS = re.compile(r'class\s*=\s*"([^"]+)"')

SECTIONS = (("blocks", "block"), ("items", "item"), ("entities", "entity"))


def implemented_classes():
    """Returns the vanilla class names Steel has a behavior for, by kind."""
    found = {"block": set(), "item": set(), "entity": set()}
    for path in SOURCES.rglob("*.rs"):
        text = path.read_text(encoding="utf-8", errors="ignore")
        for kind, args, name in STRUCT.findall(text):
            explicit = EXPLICIT_CLASS.search(args)
            found[kind].add(explicit.group(1) if explicit else name)
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--list",
        choices=[section for section, _ in SECTIONS],
        help="also print the covered and missing class names for one section",
    )
    args = parser.parse_args()

    if not CLASSES.is_file():
        print(f"missing {CLASSES}", file=sys.stderr)
        return 1

    registry = json.loads(CLASSES.read_text(encoding="utf-8"))
    found = implemented_classes()

    for section, kind in SECTIONS:
        classes = {entry["class"] for entry in registry[section]}
        covered = sorted(name for name in classes if name in found[kind])
        percent = 100 * len(covered) / max(len(classes), 1)
        print(f"{section:9} {len(covered):4} / {len(classes):4} classes  ({percent:.0f} %)")

        if args.list == section:
            missing = sorted(classes - set(covered))
            print(f"\n  covered ({len(covered)}):\n    " + "\n    ".join(covered))
            print(f"\n  missing ({len(missing)}):\n    " + "\n    ".join(missing))

    return 0


if __name__ == "__main__":
    sys.exit(main())
