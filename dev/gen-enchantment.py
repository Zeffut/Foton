#!/usr/bin/env python3
"""Generate Bukkit enchantment handles from Steel's generated registry."""
import re
import sys
from pathlib import Path

source = Path(__file__).resolve().parents[1] / "foton-registry/src/generated/vanilla_enchantments.rs"
out = Path(sys.argv[1]) / "org/bukkit/enchantments/Enchantment.java"
text = source.read_text()
entries = re.findall(r"pub static ([A-Z][A-Z0-9_]*)\s*:\s*Enchantment\s*=\s*Enchantment\s*\{\s*key\s*:\s*Identifier :: vanilla_static \(\"([^\"]+)\"", text)
if not entries:
    raise SystemExit("no enchantments found in generated registry")
out.parent.mkdir(parents=True, exist_ok=True)
lines = [
    "package org.bukkit.enchantments;", "", "import org.bukkit.Keyed;", "import org.bukkit.NamespacedKey;", "",
    "/** Generated handles backed by Steel's vanilla enchantment registry. */",
    "public final class Enchantment implements Keyed {",
    "    private final NamespacedKey key;",
    "    private Enchantment(String name) { this.key = NamespacedKey.minecraft(name); }",
    "    @Override public NamespacedKey getKey() { return key; }",
]
for name, key in entries:
    lines.append(f'    public static final Enchantment {name} = new Enchantment("{key}");')
lines += [
    "    private static final Enchantment[] VALUES = {" + ", ".join(name for name, _ in entries) + "};",
    "    public static Enchantment getByKey(NamespacedKey key) {",
    "        if (key == null) return null;",
    "        for (Enchantment enchantment : VALUES) if (enchantment.key.equals(key)) return enchantment;",
    "        return null;",
    "    }",
    "    public static Enchantment[] values() { return VALUES.clone(); }",
    "}",
]
out.write_text("\n".join(lines) + "\n")
print(f"generated {len(entries)} enchantments")
