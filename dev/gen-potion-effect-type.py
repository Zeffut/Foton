#!/usr/bin/env python3
"""Generate Bukkit's PotionEffectType from Foton's extracted mob effect registry.

Paper names each constant after its vanilla key, upper-cased, so the modern
names (JUMP_BOOST, HASTE, RESISTANCE, STRENGTH, ...) are the registry itself.
The pre-1.20.5 Bukkit names are kept as aliases of the same object, so that a
plugin comparing `type == PotionEffectType.JUMP` against `JUMP_BOOST` sees one
effect, not two.
"""
import json
import sys
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
source = repo / "foton-registry/build_assets/mob_effects.json"
out = Path(sys.argv[1]) / "org/bukkit/potion/PotionEffectType.java"

effects = sorted(json.loads(source.read_text()), key=lambda entry: entry["id"])
if not effects:
    raise SystemExit(f"no mob effects found in {source}")
names = [effect["name"] for effect in effects]

# Bukkit's legacy spellings that Foton's API already exposed, each naming the
# vanilla key it always meant.
legacy = {
    "SLOW": "slowness",
    "FAST_DIGGING": "haste",
    "SLOW_DIGGING": "mining_fatigue",
    "INCREASE_DAMAGE": "strength",
    "HEAL": "instant_health",
    "HARM": "instant_damage",
    "JUMP": "jump_boost",
    "CONFUSION": "nausea",
    "DAMAGE_RESISTANCE": "resistance",
}
missing = sorted(key for key in legacy.values() if key not in names)
if missing:
    raise SystemExit(f"legacy aliases name effects the registry lacks: {missing}")

lines = [
    "package org.bukkit.potion;",
    "",
    "import java.util.Locale;",
    "",
    "/** A vanilla mob effect, generated from Foton's mob effect registry.",
    " *",
    " * <p>One object per effect: {@link #getByName}, {@link #getByKey} and the",
    " * legacy aliases all answer the constant itself, so identity comparison",
    " * works the way it does on Paper. */",
    "public final class PotionEffectType implements org.bukkit.Keyed {",
]
for effect in effects:
    constant = effect["name"].upper()
    lines.append(
        f'    public static final PotionEffectType {constant} = new PotionEffectType('
        f'"{effect["name"]}", {effect["id"] + 1}, {effect["color"]}, "{effect["category"]}");'
    )
lines.append("")
for alias, key in sorted(legacy.items()):
    lines.append(f"    /** Bukkit's name before 1.20.5; the same effect as {{@link #{key.upper()}}}. */")
    lines.append(f"    @Deprecated public static final PotionEffectType {alias} = {key.upper()};")
lines += [
    "",
    "    private static final PotionEffectType[] VALUES = {",
    "        " + ", ".join(name.upper() for name in names),
    "    };",
    "",
    "    private final String name;",
    "    private final int id;",
    "    private final int color;",
    "    private final String category;",
    "",
    "    private PotionEffectType(String name, int id, int color, String category) {",
    "        this.name = name;",
    "        this.id = id;",
    "        this.color = color;",
    "        this.category = category;",
    "    }",
    "",
    "    /** Every effect, in registry order. */",
    "    public static PotionEffectType[] values() { return VALUES.clone(); }",
    "",
    "    /** Paper's network id: the registry index plus one. */",
    "    public static PotionEffectType getById(int id) {",
    "        return id >= 1 && id <= VALUES.length ? VALUES[id - 1] : null;",
    "    }",
    "",
    "    /** The effect for a key such as {@code jump_boost} or {@code minecraft:jump_boost}. */",
    "    public static PotionEffectType getByName(String name) {",
    "        if (name == null || name.isEmpty()) return null;",
    "        String normalized = name.toLowerCase(Locale.ROOT);",
    "        if (normalized.startsWith(\"minecraft:\")) normalized = normalized.substring(10);",
    "        for (PotionEffectType type : VALUES) if (type.name.equals(normalized)) return type;",
    "        return null;",
    "    }",
    "",
    "    public static PotionEffectType getByKey(org.bukkit.NamespacedKey key) {",
    "        return key == null || !key.getNamespace().equals(\"minecraft\") ? null : getByName(key.getKey());",
    "    }",
    "",
    "    @Override public org.bukkit.NamespacedKey getKey() { return org.bukkit.NamespacedKey.minecraft(name); }",
    "    /** The vanilla key path, which is also what crosses to Foton. */",
    "    public String getName() { return name; }",
    "    @Deprecated public int getId() { return id; }",
    "    /** The effect's particle colour, as vanilla registers it. */",
    "    public org.bukkit.Color getColor() { return org.bukkit.Color.fromRGB(color & 0xFFFFFF); }",
    "    /** {@code BENEFICIAL}, {@code HARMFUL} or {@code NEUTRAL}, as vanilla registers it. */",
    "    public String getCategoryName() { return category; }",
    "    public PotionEffect createEffect(int duration, int amplifier) {",
    "        return new PotionEffect(this, duration, amplifier);",
    "    }",
    "    @Override public String toString() { return name; }",
    "}",
    "",
]
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text("\n".join(lines), encoding="utf-8")
