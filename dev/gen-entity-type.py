#!/usr/bin/env python3
"""Generate Bukkit EntityType constants from Steel's extracted registry."""
import re
import sys
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
source = repo / "foton-registry/src/generated/vanilla_entities.rs"
out = Path(sys.argv[1]) / "org/bukkit/entity/EntityType.java"
names = re.findall(r"pub static ([A-Z][A-Z0-9_]*)\s*:", source.read_text())
if not names:
    raise SystemExit(f"no entity types found in {source}")
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(
    "package org.bukkit.entity;\n\n"
    "/** Vanilla entity types generated from Steel's registry source. */\n"
    "public enum EntityType {\n    " + ",\n    ".join(names) + "\n}\n",
    encoding="utf-8",
)
