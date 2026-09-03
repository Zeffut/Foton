#!/usr/bin/env python3
"""Generate Bukkit enchantment handles from Steel's generated registry."""
import re
import sys
from pathlib import Path

source = Path(__file__).resolve().parents[1] / "foton-registry/src/generated/vanilla_enchantments.rs"
out = Path(sys.argv[1]) / "org/bukkit/enchantments/Enchantment.java"
text = source.read_text()
entries = []
for block in re.split(r"(?=pub static [A-Z])", text):
    match = re.search(r"pub static ([A-Z][A-Z0-9_]*)\s*:.*?vanilla_static \(\"([^\"]+)\".*?max_level\s*:\s*(\d+)", block)
    if match:
        entries.append(match.groups())
if not entries:
    raise SystemExit("no enchantments found in generated registry")
out.parent.mkdir(parents=True, exist_ok=True)
lines = [
    "package org.bukkit.enchantments;", "", "import org.bukkit.Keyed;", "import org.bukkit.NamespacedKey;", "",
    "/** Generated handles backed by Steel's vanilla enchantment registry. */",
    "public final class Enchantment implements Keyed {",
    "    private final NamespacedKey key;",
    "    private final int maxLevel;",
    "    private Enchantment(String name, int maxLevel) { this.key = NamespacedKey.minecraft(name); this.maxLevel = maxLevel; }",
    "    public int getStartLevel() { return 1; }",
    "    public int getMaxLevel() { return maxLevel; }",
    "    public boolean canEnchantItem(org.bukkit.inventory.ItemStack item) {",
    "        return item != null && foton.Native.enchantmentCanEnchant(key.getKey(), item.getType().getKeyName());",
    "    }",
    "    public String getName() { return key.getKey().toUpperCase(java.util.Locale.ROOT); }",
    "    @Override public NamespacedKey getKey() { return key; }",
]
for name, key, max_level in entries:
    lines.append(f'    public static final Enchantment {name} = new Enchantment("{key}", {max_level});')
lines += [
    "    public static final Enchantment DURABILITY = UNBREAKING;",
    "    private static final Enchantment[] VALUES = {" + ", ".join(name for name, _, _ in entries) + "};",
    "    public static Enchantment getByKey(NamespacedKey key) {",
    "        if (key == null) return null;",
    "        for (Enchantment enchantment : VALUES) if (enchantment.key.equals(key)) return enchantment;",
    "        return null;",
    "    }",
    "    public static Enchantment getByName(String name) {",
    "        if (name == null) return null;",
    "        String wanted = name.toLowerCase(java.util.Locale.ROOT);",
    "        for (Enchantment enchantment : VALUES) if (enchantment.key.getKey().equals(wanted)) return enchantment;",
    "        return null;",
    "    }",
    "    public static Enchantment[] values() { return VALUES.clone(); }",
    "}",
]
out.write_text("\n".join(lines) + "\n")
print(f"generated {len(entries)} enchantments")
