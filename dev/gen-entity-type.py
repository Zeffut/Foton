#!/usr/bin/env python3
"""Generate Bukkit EntityType constants from Foton's extracted registry."""
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

# Paper 26.2 EntityType metadata differs from registry-name PascalCase for
# these entries. Keep this narrow list aligned with the upstream API metadata;
# regular names are derived only when Foton actually carries that Bukkit
# interface source.
PAPER_CLASS_ALIASES = {
    "ACACIA_BOAT": "org.bukkit.entity.boat.AcaciaBoat",
    "ACACIA_CHEST_BOAT": "org.bukkit.entity.boat.AcaciaChestBoat",
    "BAMBOO_CHEST_RAFT": "org.bukkit.entity.boat.BambooChestRaft",
    "BAMBOO_RAFT": "org.bukkit.entity.boat.BambooRaft",
    "BIRCH_BOAT": "org.bukkit.entity.boat.BirchBoat",
    "BIRCH_CHEST_BOAT": "org.bukkit.entity.boat.BirchChestBoat",
    "CHERRY_BOAT": "org.bukkit.entity.boat.CherryBoat",
    "CHERRY_CHEST_BOAT": "org.bukkit.entity.boat.CherryChestBoat",
    "CHEST_MINECART": "org.bukkit.entity.minecart.StorageMinecart",
    "COMMAND_BLOCK_MINECART": "org.bukkit.entity.minecart.CommandMinecart",
    "DARK_OAK_BOAT": "org.bukkit.entity.boat.DarkOakBoat",
    "DARK_OAK_CHEST_BOAT": "org.bukkit.entity.boat.DarkOakChestBoat",
    "END_CRYSTAL": "org.bukkit.entity.EnderCrystal",
    "EXPERIENCE_BOTTLE": "org.bukkit.entity.ThrownExpBottle",
    "EYE_OF_ENDER": "org.bukkit.entity.EnderSignal",
    "FIREBALL": "org.bukkit.entity.LargeFireball",
    "FIREWORK_ROCKET": "org.bukkit.entity.Firework",
    "FISHING_BOBBER": "org.bukkit.entity.FishHook",
    "FURNACE_MINECART": "org.bukkit.entity.minecart.PoweredMinecart",
    "HOPPER_MINECART": "org.bukkit.entity.minecart.HopperMinecart",
    "JUNGLE_BOAT": "org.bukkit.entity.boat.JungleBoat",
    "JUNGLE_CHEST_BOAT": "org.bukkit.entity.boat.JungleChestBoat",
    "LEASH_KNOT": "org.bukkit.entity.LeashHitch",
    "LIGHTNING_BOLT": "org.bukkit.entity.LightningStrike",
    "MANGROVE_BOAT": "org.bukkit.entity.boat.MangroveBoat",
    "MANGROVE_CHEST_BOAT": "org.bukkit.entity.boat.MangroveChestBoat",
    "MINECART": "org.bukkit.entity.minecart.RideableMinecart",
    "MOOSHROOM": "org.bukkit.entity.MushroomCow",
    "OAK_BOAT": "org.bukkit.entity.boat.OakBoat",
    "OAK_CHEST_BOAT": "org.bukkit.entity.boat.OakChestBoat",
    "PALE_OAK_BOAT": "org.bukkit.entity.boat.PaleOakBoat",
    "PALE_OAK_CHEST_BOAT": "org.bukkit.entity.boat.PaleOakChestBoat",
    "PUFFERFISH": "org.bukkit.entity.PufferFish",
    "SNOW_GOLEM": "org.bukkit.entity.Snowman",
    "SPAWNER_MINECART": "org.bukkit.entity.minecart.SpawnerMinecart",
    "SPRUCE_BOAT": "org.bukkit.entity.boat.SpruceBoat",
    "SPRUCE_CHEST_BOAT": "org.bukkit.entity.boat.SpruceChestBoat",
    "TNT": "org.bukkit.entity.TNTPrimed",
    "TNT_MINECART": "org.bukkit.entity.minecart.ExplosiveMinecart",
    "ZOMBIFIED_PIGLIN": "org.bukkit.entity.PigZombie",
}

# CraftRegionAccessor converts these API supertypes to a concrete default
# before consulting Paper's class metadata. Generic Boat and Fish deliberately
# have no default because their registry variant is ambiguous.
PAPER_SPAWN_DEFAULTS = {
    "org.bukkit.entity.AbstractArrow": "ARROW",
    "org.bukkit.entity.AbstractCow": "COW",
    "org.bukkit.entity.AbstractCubeMob": "SLIME",
    "org.bukkit.entity.AbstractHorse": "HORSE",
    "org.bukkit.entity.Fireball": "FIREBALL",
    "org.bukkit.entity.Minecart": "MINECART",
    "org.bukkit.entity.SizedFireball": "FIREBALL",
    "org.bukkit.entity.ThrownPotion": "SPLASH_POTION",
    "org.bukkit.entity.TippedArrow": "ARROW",
}

# Wave 3 Task 4 adds these interfaces. Keeping this allowlist explicit lets
# Task 1 emit their stable metadata without accepting arbitrary future guesses.
PLANNED_ENTITY_INTERFACES = {
    "org.bukkit.entity.Allay",
    "org.bukkit.entity.ItemDisplay",
    "org.bukkit.entity.TextDisplay",
}

entity_source_root = repo / "plugin-api/src"
entity_source_classes = {
    ".".join(path.relative_to(entity_source_root).with_suffix("").parts)
    for path in (entity_source_root / "org/bukkit/entity").rglob("*.java")
}
allowed_entity_classes = entity_source_classes | PLANNED_ENTITY_INTERFACES


def register_class(mapping, class_name, entity_type):
    if class_name not in allowed_entity_classes:
        return
    previous = mapping.setdefault(class_name, entity_type)
    if previous != entity_type:
        raise SystemExit(
            f"ambiguous Bukkit entity class {class_name}: {previous} and {entity_type}"
        )


class_to_type = {}
for name in names:
    if name == "UNKNOWN":
        continue
    regular_class = "".join(part.title() for part in name.lower().split("_"))
    class_name = PAPER_CLASS_ALIASES.get(name, f"org.bukkit.entity.{regular_class}")
    register_class(class_to_type, class_name, name)

for class_name, entity_type in PAPER_SPAWN_DEFAULTS.items():
    if entity_type not in names:
        raise SystemExit(f"Paper spawn default references missing entity type {entity_type}")
    register_class(class_to_type, class_name, entity_type)

class_to_type_cases = "\n".join(
    f'            case "{class_name}" -> {entity_type};'
    for class_name, entity_type in sorted(class_to_type.items())
)
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
    "/** Vanilla entity types generated from Foton registry source. */\n"
    "public enum EntityType implements org.bukkit.Keyed {\n    " + body + ";\n\n"
    "    private final String key;\n"
    "    EntityType() { this.key = name().toLowerCase(Locale.ROOT); }\n"
    "    public String getName() { return key; }\n"
    "    public boolean isAlive() {\n"
    "        return switch (key) {\n"
    "            case \"item\", \"item_frame\", \"glow_item_frame\", \"painting\", \"armor_stand\", \"block_display\", \"firework_rocket\", \"end_crystal\", \"arrow\", \"spectral_arrow\", \"lightning_bolt\", \"tnt\", \"tnt_minecart\", \"boat\", \"chest_boat\", \"minecart\", \"chest_minecart\", \"furnace_minecart\", \"hopper_minecart\", \"spawner_minecart\", \"fishing_bobber\", \"experience_orb\", \"egg\", \"snowball\", \"fireball\", \"small_fireball\", \"dragon_fireball\", \"ender_pearl\", \"eye_of_ender\", \"potion\", \"lingering_potion\", \"trident\", \"falling_block\", \"area_effect_cloud\", \"evoker_fangs\", \"leash_knot\", \"unknown\" -> false;\n"
    "            default -> true;\n"
    "        };\n"
    "    }\n"
    "    @Override public NamespacedKey getKey() { return NamespacedKey.minecraft(key); }\n"
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
    "    /** Returns the registry type for a canonical Bukkit entity class. */\n"
    "    public static EntityType fromEntityClass(Class<? extends Entity> entityClass) {\n"
    "        if (entityClass == null) return null;\n"
    "        return switch (entityClass.getName()) {\n"
    + class_to_type_cases + "\n"
    + "            default -> null;\n        };\n    }\n"
    "}\n",
    encoding="utf-8",
)
