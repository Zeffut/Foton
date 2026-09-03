#!/usr/bin/env python3
"""Generate Bukkit PotionType constants from Steel's extracted potion registry."""
import json, re, sys
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
source = repo / "foton-registry/src/generated/vanilla_potions.rs"
data_source = repo / "foton-registry/build_assets/potions.json"
out = Path(sys.argv[1]) / "org/bukkit/potion/PotionType.java"
names = re.findall(r"pub static ([A-Z][A-Z0-9_]*)\s*:\s*Potion", source.read_text())
if not names:
    raise SystemExit("no potion types found")
potion_data = {entry["key"].upper(): entry.get("effects", []) for entry in json.loads(data_source.read_text())}
out.parent.mkdir(parents=True, exist_ok=True)
names += [alias for alias in ("JUMP", "REGEN", "SPEED", "INSTANT_HEAL", "INSTANT_DAMAGE") if alias not in names]
body = ",\n    ".join(names)
cases = []
body += ";\n\n    /** Returns the highest amplifier level represented by this potion family. */\n    public int getMaxLevel() {\n        return name().startsWith(\"STRONG_\") ? 2 : 1;\n    }\n"
upgradeable = sorted({name.removeprefix("STRONG_") for name in names if name.startswith("STRONG_")})
upgrade_cases = ", ".join(upgradeable)
body += f""";\n\n    /** Returns whether vanilla exposes a stronger variant for this potion. */\n    public boolean isUpgradeable() {{\n        return switch (this) {{\n            case {upgrade_cases} -> true;\n            default -> false;\n        }};\n    }}\n"""
body += """\n    /** Returns whether this potion deals an instantaneous effect in vanilla. */\n    public boolean isExtendable() {
        return isUpgradeable();
    }

    /** Returns whether this potion deals an instantaneous effect in vanilla. */
    public boolean isInstant() {\n        return switch (this) {\n            case HARMING, STRONG_HARMING, HEALING, STRONG_HEALING -> true;\n            default -> false;\n        };\n    }\n"""
alias_sources = {"INSTANT_HEAL": "HEALING", "INSTANT_DAMAGE": "HARMING"}
for constant_name in names:
    effects = potion_data.get(alias_sources.get(constant_name, constant_name), [])
    if not effects:
        continue
    cases.append(f"            case {constant_name} -> {{")
    for effect in effects:
        effect_name = effect.get("effect", "")
        duration = int(effect.get("duration", 0))
        amplifier = int(effect.get("amplifier", 0))
        cases.append(f'                effects.add(new PotionEffect(PotionEffectType.getByName("{effect_name}"), {duration}, {amplifier} + Math.max(0, level - 1)));')
    cases.append("            }")
# Rebuild the generated method body with data-driven cases.
method_tail = "\n".join([
    "    /** Creates the vanilla base effects for this potion family. */",
    "    public java.util.List<PotionEffect> createEffects(int level) {",
    "        java.util.ArrayList<PotionEffect> effects = new java.util.ArrayList<>();",
    "        switch (this) {",
    *cases,
    "            default -> { }",
    "        }",
    "        return java.util.Collections.unmodifiableList(effects);",
    "    }",
    "",
    "    /** Returns the vanilla base effects at the default potion level. */",
    "    public java.util.List<PotionEffect> getPotionEffects() {",
    "        return createEffects(1);",
    "    }",
    "",
])
body += method_tail
java = "package org.bukkit.potion;\n\n/** Generated from Steel's vanilla potion registry. */\npublic enum PotionType {\n    " + body + "}\n"
out.write_text(java, encoding="utf-8")
