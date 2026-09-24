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
    "    /** Not offered by an enchanting table: Paper reads it as absence from minecraft:in_enchanting_table. */",
    "    @Deprecated public boolean isTreasure() { return !foton.Native.isTagged(\"enchantment\", \"minecraft:in_enchanting_table\", key.toString()); }",
    "    public boolean isCursed() { return foton.Native.isTagged(\"enchantment\", \"minecraft:curse\", key.toString()); }",
    "    /** Vanilla Enchantment.areCompatible, negated; an enchantment conflicts with itself. */",
    "    public boolean conflictsWith(Enchantment other) {",
    "        return other != null && foton.Native.enchantmentsConflict(key.toString(), other.key.toString());",
    "    }",
    "    /** Vanilla Enchantment.getFullname: grey, red for a curse, and the level unless both it and the maximum are one. */",
    "    public net.kyori.adventure.text.Component displayName(int level) {",
    "        net.kyori.adventure.text.Component name = net.kyori.adventure.text.Component.translatable(translationKey())",
    "            .color(isCursed() ? net.kyori.adventure.text.format.NamedTextColor.RED : net.kyori.adventure.text.format.NamedTextColor.GRAY);",
    "        if (level == 1 && maxLevel == 1) return name;",
    "        return name.append(net.kyori.adventure.text.Component.text(\" \"))",
    "            .append(net.kyori.adventure.text.Component.translatable(\"enchantment.level.\" + level));",
    "    }",
    "    public String translationKey() { return \"enchantment.\" + key.getNamespace() + \".\" + key.getKey(); }",
    "    /** The items a table offers this on, or null when the data pack names none and supported items apply. */",
    "    public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> getPrimaryItems() {",
    "        return itemSet(foton.Native.enchantmentItems(key.toString(), true));",
    "    }",
    "    /** The items this can be applied to at all. */",
    "    public io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> getSupportedItems() {",
    "        return itemSet(foton.Native.enchantmentItems(key.toString(), false));",
    "    }",
    "    private static io.papermc.paper.registry.set.RegistryKeySet<org.bukkit.inventory.ItemType> itemSet(String[] items) {",
    "        if (items == null) return null;",
    "        java.util.List<io.papermc.paper.registry.TypedKey<org.bukkit.inventory.ItemType>> keys = new java.util.ArrayList<>();",
    "        for (String item : items) keys.add(io.papermc.paper.registry.TypedKey.create(io.papermc.paper.registry.RegistryKey.ITEM, item));",
    "        return io.papermc.paper.registry.set.RegistrySet.keySet(io.papermc.paper.registry.RegistryKey.ITEM, keys);",
    "    }",
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
