#!/usr/bin/env python3
"""Writes org.bukkit.Material and org.bukkit.Sound from Foton's registries.

Bukkit's Material is one enum over every block and every item, and a plugin
that touches an inventory or a block reads it constantly -- `ItemStack#getType`
alone is called by twenty-six of the fifty-nine plugins surveyed.

It is generated rather than written because there are sixteen hundred of them
and because a hand-written copy would be a second source of truth that drifts
from the registry the server actually uses. The registry files this reads are
the ones foton-registry builds from, so the enum cannot name a block Foton does
not have.

Names are the registry path in upper case, which is what Bukkit's are:
`minecraft:diamond_sword` is DIAMOND_SWORD. That is not a coincidence to be
grateful for -- it is the reason a plugin's `Material.valueOf(name)` and its
`Material.DIAMOND_SWORD` both resolve against a server that never ran Mojang's
code.

    python3 dev/gen-material.py <output-directory>
"""
import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
ITEMS = REPO / "foton-registry" / "build_assets" / "items.json"
BLOCKS = REPO / "foton-registry" / "build_assets" / "blocks.json"
SOUNDS = REPO / "foton-registry" / "build_assets" / "sound_events.json"
CLASSES = REPO / "foton-core" / "build" / "classes.json"
# Which blocks catch fire, extracted once from FireBlock.setFlammable and
# committed. The repository's CI builds this jar and does not have
# minecraft-src -- it is a gigabyte of decompiled Mojang source and does not
# belong in the tree -- so a build step must not read it.
FLAMMABLE = REPO / "dev" / "flammable-blocks.json"
FIRE_BLOCK = (REPO / "minecraft-src" / "minecraft" / "src" / "net" / "minecraft" / "world"
              / "level" / "block" / "FireBlock.java")

# Java keywords cannot be enum constants. None of the registry names is one
# today, and a name that became one would otherwise fail to compile with a
# message that says nothing about where it came from.
KEYWORDS = frozenset("""
abstract assert boolean break byte case catch char class const continue default
do double else enum extends final finally float for goto if implements import
instanceof int interface long native new package private protected public return
short static strictfp super switch synchronized this throw throws transient try
void volatile while
""".split())


def read():
    """Every material, as {name: (is_block, is_item, stack_size, max_damage, is_solid, is_occluding)}."""
    items = json.loads(ITEMS.read_text(encoding="utf-8"))["items"]
    block_data = json.loads(BLOCKS.read_text(encoding="utf-8"))
    classes = json.loads(CLASSES.read_text(encoding="utf-8"))
    gravity_classes = {"AnvilBlock", "ColoredFallingBlock", "ConcretePowderBlock", "DragonEggBlock", "SandBlock"}
    gravity_blocks = {entry["name"] for entry in classes["blocks"] if entry.get("class") in gravity_classes}
    burnable_blocks = set(json.loads(FLAMMABLE.read_text(encoding="utf-8"))["blocks"])
    blocks = block_data["blocks"]
    shapes = block_data["shapes"]

    stacks = {}
    damages = {}
    for item in items:
        components = item.get("components", {})
        size = components.get("minecraft:max_stack_size", 64)
        stacks[item["name"]] = size if isinstance(size, int) else 64
        damage = components.get("minecraft:max_damage", 0)
        damages[item["name"]] = damage if isinstance(damage, int) else 0

    block_names = {block["name"] for block in blocks}
    materials = {}
    for block in blocks:
        properties = block["behavior_properties"]
        block["_is_occluding"] = bool(properties["canOcclude"])
        if properties["forceSolidOn"]:
            solid = True
        elif properties["forceSolidOff"] or properties["dynamicShape"]:
            solid = False
        else:
            boxes = [shapes[index] for index in block["collision_shapes"]["default"]]
            if not boxes:
                solid = False
            else:
                minimum = [min(box["min"][axis] for box in boxes) for axis in range(3)]
                maximum = [max(box["max"][axis] for box in boxes) for axis in range(3)]
                size = sum(maximum[axis] - minimum[axis] for axis in range(3)) / 3.0
                solid = size >= 0.7291666666666666 or maximum[1] - minimum[1] >= 1.0
        block["_is_solid"] = solid

    solids = {block["name"]: block["_is_solid"] for block in blocks}
    for name in sorted(block_names | set(stacks)):
        materials[name] = (
            name in block_names,
            name in stacks,
            stacks.get(name, 0),
            damages.get(name, 0),
            solids.get(name, False),
            next((block["_is_occluding"] for block in blocks if block["name"] == name), False),
            name in gravity_blocks,
            name in burnable_blocks,
        )
    return materials


def constant(name):
    """The enum constant a registry path becomes."""
    upper = name.upper()
    if upper.lower() in KEYWORDS:
        raise SystemExit(f"registry name {name!r} is a Java keyword")
    if not upper[0].isalpha() and upper[0] != "_":
        raise SystemExit(f"registry name {name!r} cannot start a Java identifier")
    return upper


def render(materials):
    lines = [
        "package org.bukkit;",
        "",
        "import java.util.HashMap;",
        "import java.util.Locale;",
        "import java.util.Map;",
        "",
        "/** Every block and every item the server knows, as Bukkit names them.",
        " *",
        " * Generated by dev/gen-material.py from Foton's own registries. Do not",
        " * edit: the registry is the source of truth, and a hand-edit here would be",
        " * a second one that drifts.",
        " */",
        "public enum Material implements org.bukkit.Keyed {",
    ]

    for name, (is_block, is_item, stack, damage, is_solid, is_occluding, has_gravity, is_burnable) in materials.items():
        flags = (1 if is_block else 0) | (2 if is_item else 0)
        if is_solid:
            flags |= 4
        if is_occluding:
            flags |= 8
        if has_gravity:
            flags |= 16
        if is_burnable:
            flags |= 32
        lines.append(f'    {constant(name)}("{name}", {flags}, {stack}, {damage}),')
    lines[-1] = lines[-1][:-1] + ";"
    lines += ["", "    /** Legacy alias for the generic infested-stone block. */", "    public static final Material MONSTER_EGG = INFESTED_STONE;"]

    lines += [
        "",
        "    /** Registry paths back to their material, for matchMaterial. */",
        "    private static final Map<String, Material> BY_KEY = new HashMap<>();",
        "",
        "    static {",
        "        for (Material material : values()) {",
        "            BY_KEY.put(material.key, material);",
        "        }",
        "    }",
        "",
        "    private final String key;",
        "    private final int flags;",
        "    private final int stackSize;",
        "    private final int maxDamage;",
        "",
        "    Material(String key, int flags, int stackSize, int maxDamage) {",
        "        this.key = key;",
        "        this.flags = flags;",
        "        this.stackSize = stackSize;",
        "        this.maxDamage = maxDamage;",
        "    }",
        "",
        "    /** Whether this can be placed in the world. */",
        "    public boolean isBlock() {",
        "        return (flags & 1) != 0;",
        "    }",
        "",
        "    /** Whether this can be held in an inventory.",
        "     *",
        "     * Not the same question as isBlock: wall signs and crop stems are",
        "     * blocks with no item, and a plugin that puts one in an inventory has",
        "     * made a stack of nothing.",
        "     */",
        "    public boolean isItem() {",
        "        return (flags & 2) != 0;",
        "    }",
        "",
        "    /** Whether the default block state has vanilla's legacy solid shape. */",
        "    public boolean isSolid() {",
        "        return (flags & 4) != 0;",
        "    }",
        "",
        "    /** Whether the default block state occludes faces in vanilla. */",
        "    public boolean isBurnable() {",
        "        return (flags & 32) != 0;",
        "    }",
        "",
        "    public boolean hasGravity() {",
        "        return (flags & 16) != 0;",
        "    }",
        "",
        "    public boolean isOccluding() {",
        "        return (flags & 8) != 0;",
        "    }",
        "",
        "    /** Whether the default block state is transparent to light and faces. */",
        "    public boolean isTransparent() {",
        "        return !isOccluding();",
        "    }",
        "",
        "    public boolean isAir() {",
        "        return this == AIR || this == CAVE_AIR || this == VOID_AIR;",
        "    }",
        "",
        "    /** How many fit in one stack. Zero for a block that is not an item. */",
        "    public int getMaxStackSize() {",
        "        return stackSize;",
        "    }",
        "",
        "    /** Maximum durability for damageable items, zero otherwise. */",
        "    public short getMaxDurability() {",
        "        return (short) maxDamage;",
        "    }",
        "",
        "    /** Legacy data class; modern registry entries use the generic representation. */",
        "    public Class<? extends org.bukkit.material.MaterialData> getData() {",
        "        return org.bukkit.material.MaterialData.class;",
        "    }",
        "",
        "    /** Legacy magic IDs are not defined for modern materials. */",
        "    @Deprecated",
        "    public int getId() {",
        '        throw new IllegalArgumentException("Cannot get ID of Modern Material");',
        "    }",
        "",
        "    public org.bukkit.block.data.BlockData createBlockData() {",
        "        return new org.bukkit.block.data.SimpleBlockData(\"minecraft:\" + key);",
        "    }",
        "",
        "    public org.bukkit.block.data.BlockData createBlockData(String data) {",
        "        if (data == null || data.isEmpty()) return createBlockData();",
        "        return new org.bukkit.block.data.SimpleBlockData(data);",
        "    }",
        "",
        "    public org.bukkit.inventory.EquipmentSlot getEquipmentSlot() {",
        "        String upper = name().toUpperCase(Locale.ROOT);",
        "        if (upper.endsWith(\"_HELMET\") || upper.endsWith(\"_SKULL\") || upper.endsWith(\"_HEAD\")) return org.bukkit.inventory.EquipmentSlot.HEAD;",
        "        if (upper.endsWith(\"_CHESTPLATE\")) return org.bukkit.inventory.EquipmentSlot.CHEST;",
        "        if (upper.endsWith(\"_LEGGINGS\")) return org.bukkit.inventory.EquipmentSlot.LEGS;",
        "        if (upper.endsWith(\"_BOOTS\")) return org.bukkit.inventory.EquipmentSlot.FEET;",
        "        if (upper.equals(\"SADDLE\")) return org.bukkit.inventory.EquipmentSlot.SADDLE;",
        "        return org.bukkit.inventory.EquipmentSlot.HAND;",
        "    }",
        "",
        "    public NamespacedKey getKey() {",
        '        return NamespacedKey.minecraft(key);',
        "    }",
        "",
        "    /** The registry path, which is the lower-case half of the name. */",
        "    public String getKeyName() {",
        "        return key;",
        "    }",
        "",
        "    /** Finds a material by name or by key, the way a config file writes it.",
        "     *",
        "     * Bukkit accepts `DIAMOND_SWORD`, `diamond_sword` and",
        "     * `minecraft:diamond_sword`, and answers null rather than throwing,",
        "     * because plugins call this on whatever an operator typed.",
        "     */",
        "    public static Material matchMaterial(String name) {",
        "        if (name == null) {",
        "            return null;",
        "        }",
        "        String trimmed = name.trim();",
        "        if (trimmed.startsWith(\"minecraft:\")) {",
        "            trimmed = trimmed.substring(10);",
        "        }",
        "        Material byKey = BY_KEY.get(trimmed.toLowerCase(Locale.ROOT));",
        "        if (byKey != null) {",
        "            return byKey;",
        "        }",
        "        try {",
        "            return valueOf(trimmed.toUpperCase(Locale.ROOT));",
        "        } catch (IllegalArgumentException unknown) {",
        "            return null;",
        "        }",
        "    }",
        "",
        "    /** Overload retained for Bukkit callers that request legacy-name matching. */",
        "    public static Material matchMaterial(String name, boolean legacyName) {",
        "        return matchMaterial(name);",
        "    }",
        "",
        "    /** Legacy Bukkit alias retained for plugins that use getMaterial. */",
        "    public static Material getMaterial(String name) {",
        "        return matchMaterial(name);",
        "    }",
        "    public static Material getMaterial(String name, boolean legacyName) {",
        "        return matchMaterial(name, legacyName);",
        "    }",
        "}",
        "",
    ]
    return "\n".join(lines)


def sounds():
    """Every sound event, keyed by the name Bukkit gives it.

    `entity.allay.ambient_with_item` becomes ENTITY_ALLAY_AMBIENT_WITH_ITEM,
    which is what Bukkit calls it and therefore what a plugin writes.
    """
    entries = json.loads(SOUNDS.read_text(encoding="utf-8"))
    found = {}
    for entry in entries:
        key = entry["key"]
        namespace, _, path = key.partition(":")
        if namespace != "minecraft":
            continue
        found[path] = key
    return dict(sorted(found.items()))


def render_sounds(found):
    lines = [
        "package org.bukkit;",
        "",
        "import java.util.HashMap;",
        "import java.util.Locale;",
        "import java.util.Map;",
        "",
        "/** Every sound the server can name.",
        " *",
        " * Generated by dev/gen-material.py from Foton's own registry. Do not",
        " * edit: the registry is the source of truth.",
        " */",
        "public enum Sound {",
    ]
    for path, key in found.items():
        lines.append(f'    {path.upper().replace(".", "_").replace("/", "_")}("{key}"),')
    lines[-1] = lines[-1][:-1] + ";"
    lines += [
        "",
        "    private static final Map<String, Sound> BY_KEY = new HashMap<>();",
        "",
        "    static {",
        "        for (Sound sound : values()) {",
        "            BY_KEY.put(sound.key, sound);",
        "        }",
        "    }",
        "",
        "    private final String key;",
        "",
        "    Sound(String key) {",
        "        this.key = key;",
        "    }",
        "",
        "    public String getKey() {",
        "        return key;",
        "    }",
        "",
        "    public static Sound match(String name) {",
        "        if (name == null) {",
        "            return null;",
        "        }",
        "        Sound byKey = BY_KEY.get(name.trim().toLowerCase(Locale.ROOT));",
        "        if (byKey != null) {",
        "            return byKey;",
        "        }",
        "        try {",
        "            return valueOf(name.trim().toUpperCase(Locale.ROOT));",
        "        } catch (IllegalArgumentException unknown) {",
        "            return null;",
        "        }",
        "    }",
        "}",
        "",
    ]
    return "\n".join(lines)


def refresh_flammable():
    """Re-extracts the flammable list from minecraft-src, when it is there.

    Run when the Minecraft version changes. The build never does this: it
    reads the committed result, so a machine without the decompiled source
    can still produce the jar.
    """
    if not FIRE_BLOCK.exists():
        raise SystemExit(f"no {FIRE_BLOCK}; minecraft-src is needed to refresh")
    source = FIRE_BLOCK.read_text(encoding="utf-8")
    names = sorted({match.group(1).lower()
                    for match in re.finditer(r"setFlammable\(Blocks\.([A-Z0-9_]+),", source)})
    current = json.loads(FLAMMABLE.read_text(encoding="utf-8"))
    current["blocks"] = names
    FLAMMABLE.write_text(json.dumps(current, indent=2) + "\n", encoding="utf-8")
    print(f"refreshed {len(names)} flammable blocks")


def main():
    if len(sys.argv) == 2 and sys.argv[1] == "--refresh-flammable":
        refresh_flammable()
        return
    if len(sys.argv) != 2:
        raise SystemExit("usage: gen-material.py <output-directory> | --refresh-flammable")
    out = pathlib.Path(sys.argv[1]) / "org" / "bukkit"
    out.mkdir(parents=True, exist_ok=True)

    materials = read()
    (out / "Material.java").write_text(render(materials), encoding="utf-8")

    found = sounds()
    (out / "Sound.java").write_text(render_sounds(found), encoding="utf-8")

    print(f"generated {len(materials)} materials and {len(found)} sounds")


if __name__ == "__main__":
    main()
