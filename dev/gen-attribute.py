#!/usr/bin/env python3
"""Generate Bukkit's Attribute interface from Foton's extracted attribute registry.

Paper made `Attribute` a registry-backed interface in 1.21.3, and a plugin
compiled against it calls `Attribute.getKey()` with `invokeinterface`: an enum
here links and then throws `IncompatibleClassChangeError`. Each constant is the
vanilla key upper-cased, which is exactly how Paper names them, so the list
cannot drift from the registry the server itself is built from.
"""
import json
import sys
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
source = repo / "foton-registry/build_assets/attributes.json"
out = Path(sys.argv[1]) / "org/bukkit/attribute/Attribute.java"

entries = sorted(json.loads(source.read_text()), key=lambda entry: entry["id"])
keys = [entry["name"] for entry in entries]
if not keys:
    raise SystemExit(f"no attributes found in {source}")

# Bukkit's pre-1.21.3 spellings, exactly the set Foton's API exposed before
# Attribute became an interface. Paper still loads plugins built against them by
# renaming the field at class-load time; Foton has no such rewriter, so they
# stay as aliases of the same object. Each maps onto a current key by dropping
# its prefix, and one that no longer does is left out rather than guessed.
legacy = {
    "GENERIC_MAX_HEALTH", "GENERIC_FOLLOW_RANGE", "GENERIC_KNOCKBACK_RESISTANCE",
    "GENERIC_MOVEMENT_SPEED", "GENERIC_ATTACK_DAMAGE", "GENERIC_ATTACK_KNOCKBACK",
    "GENERIC_ATTACK_SPEED", "GENERIC_ARMOR", "GENERIC_ARMOR_TOUGHNESS",
    "GENERIC_LUCK", "GENERIC_JUMP_STRENGTH", "GENERIC_SCALE",
    "PLAYER_BLOCK_INTERACTION_RANGE", "PLAYER_ENTITY_INTERACTION_RANGE",
    "PLAYER_BLOCK_BREAK_SPEED", "PLAYER_MINING_EFFICIENCY", "PLAYER_SNEAKING_SPEED",
    "ZOMBIE_SPAWN_REINFORCEMENTS",
}
current = {key.upper() for key in keys}
aliases = []
for name in sorted(legacy):
    target = name.split("_", 1)[1]
    if target in current:
        aliases.append((name, target))

lines = [
    "package org.bukkit.attribute;",
    "",
    "/** A vanilla attribute, generated from Foton's attribute registry.",
    " *",
    " * <p>An interface, as in Paper: the constants are registry entries, not an",
    " * enum, and a plugin compiled against Paper links against this shape. */",
    "public interface Attribute extends org.bukkit.util.OldEnum<Attribute>, org.bukkit.Keyed,",
    "        net.kyori.adventure.translation.Translatable {",
]
for index, key in enumerate(keys):
    lines.append(f'    Attribute {key.upper()} = new foton.FotonAttribute("{key}", {index});')
lines.append("")
for name, target in aliases:
    lines.append(f"    /** Bukkit's name before 1.21.3; the same attribute as {{@link #{target}}}. */")
    lines.append(f"    @Deprecated Attribute {name} = {target};")
lines += [
    "",
    "    /** Every attribute, in registry order. */",
    "    @Deprecated static Attribute[] values() {",
    "        return new Attribute[] {" + ", ".join(key.upper() for key in keys) + "};",
    "    }",
    "",
    "    /** Resolves a constant name, or a key, the way Paper's deprecated bridge does. */",
    "    @Deprecated static Attribute valueOf(String name) {",
    "        if (name == null) throw new NullPointerException(\"name\");",
    "        String upper = name.toUpperCase(java.util.Locale.ROOT);",
    "        String path = upper.startsWith(\"MINECRAFT:\") ? upper.substring(10) : upper;",
    "        Attribute value = switch (path) {",
]
for key in keys:
    lines.append(f'            case "{key.upper()}" -> {key.upper()};')
for name, target in aliases:
    lines.append(f'            case "{name}" -> {target};')
lines += [
    "            default -> null;",
    "        };",
    "        if (value == null) throw new IllegalArgumentException(\"No attribute found with the name \" + name);",
    "        return value;",
    "    }",
    "",
    "    /** The vanilla translation key, {@code attribute.name.<path>}. */",
    "    default String getTranslationKey() { return translationKey(); }",
    "}",
    "",
]
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text("\n".join(lines), encoding="utf-8")
