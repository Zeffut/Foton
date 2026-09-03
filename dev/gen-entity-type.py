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
if "UNKNOWN" not in names:
    names.insert(0, "UNKNOWN")
body = ",\n    ".join(names)
entity_classes = {
    "PLAYER": "FotonPlayer", "VILLAGER": "FotonVillager", "COW": "FotonCow",
    "PIG": "FotonPig", "CHICKEN": "FotonChicken", "NAUTILUS": "FotonNautilus", "ZOMBIE": "FotonZombie", "ZOMBIE_NAUTILUS": "FotonZombieNautilus", "ZOMBIE_VILLAGER": "FotonZombieVillager",
    "IRON_GOLEM": "FotonIronGolem", "SNOW_GOLEM": "FotonGolem", "CREEPER": "FotonCreeper",
    "SHEEP": "FotonSheep", "WOLF": "FotonWolf", "CAT": "FotonCat", "OCELOT": "FotonOcelot", "HORSE": "FotonHorse",
    "CAMEL": "FotonCamel", "LLAMA": "FotonLlama", "TRADER_LLAMA": "FotonLlama",
    "DONKEY": "FotonChestedHorse", "MULE": "FotonChestedHorse", "FOX": "FotonFox",
    "BEE": "FotonBee", "PARROT": "FotonParrot", "PANDA": "FotonPanda", "FROG": "FotonFrog",
    "GOAT": "FotonGoat", "AXOLOTL": "FotonAxolotl", "PHANTOM": "FotonPhantom",
    "ENDERMAN": "FotonEnderman", "ITEM": "FotonItem", "ITEM_FRAME": "FotonItemFrame",
    "GLOW_ITEM_FRAME": "FotonItemFrame", "PAINTING": "FotonPainting", "ARMOR_STAND": "FotonArmorStand",
    "BLOCK_DISPLAY": "FotonBlockDisplay", "FIREWORK_ROCKET": "FotonFirework",
    "END_CRYSTAL": "FotonEnderCrystal", "ARROW": "FotonArrow", "SPECTRAL_ARROW": "FotonArrow",
}
class_cases = "\n".join(
    f"            case {name} -> foton.{class_name}.class;" for name, class_name in entity_classes.items()
)
out.write_text(
    "package org.bukkit.entity;\n\n"
    "import java.util.Locale;\n"
    "import org.bukkit.NamespacedKey;\n\n"
    "/** Vanilla entity types generated from Steel registry source. */\n"
    "public enum EntityType {\n    " + body + ";\n\n"
    "    private final String key;\n"
    "    EntityType() { this.key = name().toLowerCase(Locale.ROOT); }\n"
    "    public String getName() { return key; }\n"
    "    public boolean isAlive() {\n"
    "        return switch (key) {\n"
    "            case \"item\", \"item_frame\", \"glow_item_frame\", \"painting\", \"armor_stand\", \"block_display\", \"firework_rocket\", \"end_crystal\", \"arrow\", \"spectral_arrow\", \"lightning_bolt\", \"tnt\", \"tnt_minecart\", \"boat\", \"chest_boat\", \"minecart\", \"chest_minecart\", \"furnace_minecart\", \"hopper_minecart\", \"spawner_minecart\", \"fishing_bobber\", \"experience_orb\", \"egg\", \"snowball\", \"fireball\", \"small_fireball\", \"dragon_fireball\", \"ender_pearl\", \"eye_of_ender\", \"potion\", \"lingering_potion\", \"trident\", \"falling_block\", \"area_effect_cloud\", \"evoker_fangs\", \"leash_knot\", \"unknown\" -> false;\n"
    "            default -> true;\n"
    "        };\n"
    "    }\n"
    "    public NamespacedKey getKey() { return NamespacedKey.minecraft(key); }\n"
    "    /** Returns the concrete wrapper when Foton exposes one. */\n"
    "    public Class<? extends Entity> getEntityClass() {\n"
    "        return switch (this) {\n"
    + class_cases + "\n"
    + "            default -> null;\n        };\n    }\n"
    "    /** Legacy numeric IDs are not part of Steel's modern entity registry. */\n"
    "    @Deprecated public short getTypeId() { return -1; }\n"
    "    /** No legacy numeric entity mapping is available for this registry. */\n"
    "    @Deprecated public static EntityType fromId(int id) { return null; }\n"
    "    public static EntityType fromName(String value) {\n"
    "        if (value == null) return null;\n"
    "        String normalized = value.toLowerCase(Locale.ROOT);\n"
    "        for (EntityType type : values()) if (type.key.equals(normalized)) return type;\n"
    "        return null;\n"
    "    }\n"
    "}\n",
    encoding="utf-8",
)
