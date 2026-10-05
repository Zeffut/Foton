#!/usr/bin/env python3
"""Generate Bukkit's Attribute interface and Foton implementation from extracted data."""

from __future__ import annotations

import json
import math
from pathlib import Path
import re
import sys


ALIASES = {
    "GENERIC_MAX_HEALTH": "MAX_HEALTH",
    "GENERIC_FOLLOW_RANGE": "FOLLOW_RANGE",
    "GENERIC_KNOCKBACK_RESISTANCE": "KNOCKBACK_RESISTANCE",
    "GENERIC_MOVEMENT_SPEED": "MOVEMENT_SPEED",
    "GENERIC_ATTACK_DAMAGE": "ATTACK_DAMAGE",
    "GENERIC_ATTACK_KNOCKBACK": "ATTACK_KNOCKBACK",
    "GENERIC_ATTACK_SPEED": "ATTACK_SPEED",
    "GENERIC_ARMOR": "ARMOR",
    "GENERIC_ARMOR_TOUGHNESS": "ARMOR_TOUGHNESS",
    "GENERIC_LUCK": "LUCK",
    "GENERIC_JUMP_STRENGTH": "JUMP_STRENGTH",
    "GENERIC_SCALE": "SCALE",
    "GENERIC_STEP_HEIGHT": "STEP_HEIGHT",
    "PLAYER_BLOCK_INTERACTION_RANGE": "BLOCK_INTERACTION_RANGE",
    "PLAYER_ENTITY_INTERACTION_RANGE": "ENTITY_INTERACTION_RANGE",
    "PLAYER_BLOCK_BREAK_SPEED": "BLOCK_BREAK_SPEED",
    "PLAYER_MINING_EFFICIENCY": "MINING_EFFICIENCY",
    "PLAYER_SNEAKING_SPEED": "SNEAKING_SPEED",
    "ZOMBIE_SPAWN_REINFORCEMENTS": "SPAWN_REINFORCEMENTS",
}
# Paper exposes sentiment through its Attribute API, while FotonExtractor's
# registry asset intentionally does not. Keep that API-only compatibility
# metadata separate here; PaperAttributeConsumer pins representative values.
PAPER_SENTIMENTS = {
    "air_drag_modifier": "POSITIVE",
    "armor": "POSITIVE",
    "armor_toughness": "POSITIVE",
    "attack_damage": "POSITIVE",
    "attack_knockback": "POSITIVE",
    "attack_speed": "POSITIVE",
    "below_name_distance": "POSITIVE",
    "block_break_speed": "POSITIVE",
    "block_interaction_range": "POSITIVE",
    "bounciness": "POSITIVE",
    "burning_time": "NEGATIVE",
    "camera_distance": "POSITIVE",
    "explosion_knockback_resistance": "POSITIVE",
    "entity_interaction_range": "POSITIVE",
    "fall_damage_multiplier": "NEGATIVE",
    "flying_speed": "POSITIVE",
    "follow_range": "POSITIVE",
    "friction_modifier": "POSITIVE",
    "gravity": "NEUTRAL",
    "jump_strength": "POSITIVE",
    "knockback_resistance": "POSITIVE",
    "luck": "POSITIVE",
    "max_absorption": "POSITIVE",
    "max_health": "POSITIVE",
    "mining_efficiency": "POSITIVE",
    "movement_efficiency": "POSITIVE",
    "movement_speed": "POSITIVE",
    "name_tag_distance": "POSITIVE",
    "oxygen_bonus": "POSITIVE",
    "safe_fall_distance": "POSITIVE",
    "scale": "NEUTRAL",
    "sneaking_speed": "POSITIVE",
    "spawn_reinforcements": "POSITIVE",
    "step_height": "POSITIVE",
    "submerged_mining_speed": "POSITIVE",
    "sweeping_damage_ratio": "POSITIVE",
    "tempt_range": "POSITIVE",
    "water_movement_efficiency": "POSITIVE",
    "waypoint_transmit_range": "NEUTRAL",
    "waypoint_receive_range": "NEUTRAL",
}
NAME = re.compile(r"[a-z][a-z0-9_]*\Z")


def java_string(value: str) -> str:
    return value.replace("\\", "\\\\").replace('"', '\\"')


def java_double(value: object) -> str:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"default_value must be numeric, got {value!r}")
    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"default_value must be finite, got {value!r}")
    literal = repr(value)
    return literal if "." in literal or "e" in literal.lower() else literal + ".0"


def load_attributes(path: Path) -> list[dict[str, object]]:
    attributes = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(attributes, list) or not attributes:
        raise ValueError("attributes.json must contain a non-empty array")
    attributes.sort(key=lambda attribute: attribute["id"])
    for ordinal, attribute in enumerate(attributes):
        missing = {
            "id", "name", "translation_key", "default_value"
        } - attribute.keys()
        if missing:
            raise ValueError(f"attribute entry is missing {sorted(missing)}")
        if attribute["id"] != ordinal:
            raise ValueError(
                f"attribute ids must be contiguous: expected {ordinal}, got {attribute['id']}"
            )
        name = attribute["name"]
        if not isinstance(name, str) or NAME.fullmatch(name) is None:
            raise ValueError(f"invalid attribute name {name!r}")
        translation_key = attribute["translation_key"]
        if not isinstance(translation_key, str) or not translation_key:
            raise ValueError(f"invalid translation key for {name}")
        java_double(attribute["default_value"])

    constants = {str(attribute["name"]).upper() for attribute in attributes}
    missing_alias_targets = set(ALIASES.values()) - constants
    if missing_alias_targets:
        raise ValueError(f"attribute aliases have missing targets: {sorted(missing_alias_targets)}")
    names = {str(attribute["name"]) for attribute in attributes}
    if PAPER_SENTIMENTS.keys() != names:
        missing = names - PAPER_SENTIMENTS.keys()
        extra = PAPER_SENTIMENTS.keys() - names
        raise ValueError(
            f"Paper sentiment map differs from the asset; missing={sorted(missing)}, "
            f"extra={sorted(extra)}"
        )
    return attributes


def render(attributes: list[dict[str, object]]) -> str:
    lines = [
        "package org.bukkit.attribute;",
        "",
        "/** Registry-backed Paper 26.2 attribute surface.",
        " *",
        " * Generated by dev/gen-attribute.py from FotonExtractor's attributes.json.",
        " * Do not edit this generated source.",
        " */",
        "public interface Attribute extends org.bukkit.util.OldEnum<Attribute>, org.bukkit.Keyed,",
        "        org.bukkit.Translatable, net.kyori.adventure.translation.Translatable {",
    ]
    for attribute in attributes:
        constant = str(attribute["name"]).upper()
        lines.append(f"    Attribute {constant} = FotonAttributes.{constant};")

    lines += ["", "    /** Legacy Bukkit names retained as aliases of modern values. */"]
    for alias, target in ALIASES.items():
        lines.append(f"    Attribute {alias} = {target};")

    lines += [
        "",
        "    Sentiment getSentiment();",
        "",
        "    double getDefaultValue();",
        "",
        "    static Attribute valueOf(String name) {",
        "        return FotonAttributes.valueOf(name);",
        "    }",
        "",
        "    static Attribute[] values() {",
        "        return FotonAttributes.values();",
        "    }",
        "",
        "    enum Sentiment {",
        "        POSITIVE,",
        "        NEUTRAL,",
        "        NEGATIVE",
        "    }",
        "}",
        "",
        "final class FotonAttribute implements Attribute {",
        "    private final org.bukkit.NamespacedKey key;",
        "    private final String name;",
        "    private final String translationKey;",
        "    private final int ordinal;",
        "    private final double defaultValue;",
        "    private final Attribute.Sentiment sentiment;",
        "",
        "    FotonAttribute(String key, String translationKey, int ordinal, double defaultValue,",
        "            Attribute.Sentiment sentiment) {",
        "        this.key = org.bukkit.NamespacedKey.minecraft(key);",
        "        this.name = key.toUpperCase(java.util.Locale.ROOT);",
        "        this.translationKey = translationKey;",
        "        this.ordinal = ordinal;",
        "        this.defaultValue = defaultValue;",
        "        this.sentiment = sentiment;",
        "    }",
        "",
        "    @Override public org.bukkit.NamespacedKey getKey() { return key; }",
        "    @Override public String getTranslationKey() { return translationKey; }",
        "    @Override public String translationKey() { return translationKey; }",
        "    @Override public Attribute.Sentiment getSentiment() { return sentiment; }",
        "    @Override public double getDefaultValue() { return defaultValue; }",
        "    @Override public String name() { return name; }",
        "    @Override public int ordinal() { return ordinal; }",
        "    @Override public int compareTo(Attribute other) {",
        "        return Integer.compare(ordinal, other.ordinal());",
        "    }",
        "    @Override public String toString() { return name; }",
        "}",
        "",
        "final class FotonAttributes {",
    ]

    for attribute in attributes:
        name = str(attribute["name"])
        constant = name.upper()
        translation_key = java_string(str(attribute["translation_key"]))
        default_value = java_double(attribute["default_value"])
        sentiment = PAPER_SENTIMENTS[name]
        lines += [
            f"    static final Attribute {constant} = value(",
            f'        "{java_string(name)}", "{translation_key}", {attribute["id"]}, '
            f"{default_value}, Attribute.Sentiment.{sentiment});",
        ]

    constants = [str(attribute["name"]).upper() for attribute in attributes]
    lines += [
        "",
        "    private static final Attribute[] VALUES = {",
    ]
    for start in range(0, len(constants), 6):
        suffix = "," if start + 6 < len(constants) else ""
        lines.append("        " + ", ".join(constants[start : start + 6]) + suffix)
    lines += [
        "    };",
        "    private static final java.util.Map<String, Attribute> BY_NAME = names();",
        "",
        "    private FotonAttributes() {}",
        "",
        "    static Attribute valueOf(String name) {",
        "        String normalized = java.util.Objects.requireNonNull(name, \"name\")",
        "            .toUpperCase(java.util.Locale.ROOT);",
        "        if (normalized.startsWith(\"MINECRAFT:\")) {",
        "            normalized = normalized.substring(\"MINECRAFT:\".length());",
        "        }",
        "        Attribute value = BY_NAME.get(normalized);",
        "        if (value == null) {",
        "            throw new IllegalArgumentException(\"No attribute found with the name \" + name);",
        "        }",
        "        return value;",
        "    }",
        "",
        "    static Attribute[] values() {",
        "        return java.util.Arrays.copyOf(VALUES, VALUES.length);",
        "    }",
        "",
        "    private static Attribute value(String key, String translationKey, int ordinal,",
        "            double defaultValue, Attribute.Sentiment sentiment) {",
        "        return new FotonAttribute(key, translationKey, ordinal, defaultValue, sentiment);",
        "    }",
        "",
        "    private static java.util.Map<String, Attribute> names() {",
        "        java.util.Map<String, Attribute> values = new java.util.HashMap<>();",
        "        for (Attribute value : VALUES) values.put(value.name(), value);",
    ]
    for alias, target in ALIASES.items():
        lines.append(f'        values.put("{alias}", {target});')
    lines += [
        "        return java.util.Collections.unmodifiableMap(values);",
        "    }",
        "}",
        "",
    ]
    return "\n".join(lines)


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: gen-attribute.py <attributes.json> <output-root>")
    attributes = load_attributes(Path(sys.argv[1]))
    output = Path(sys.argv[2]) / "org/bukkit/attribute/Attribute.java"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(render(attributes), encoding="utf-8")
    print(f"generated {len(attributes)} attributes")


if __name__ == "__main__":
    main()
