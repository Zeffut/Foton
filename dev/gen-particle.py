#!/usr/bin/env python3
"""Generate Bukkit's Particle enum from Foton's extracted particle registry.

Paper names each constant after its vanilla key, upper-cased, and pairs it with
the Java type a plugin hands over as its data. That type is decided by the
payload vanilla registers the particle with (`options_type` in the extracted
registry), so both the names and the data types come from the registry the
server encodes particles with -- a constant cannot promise data the packet
would not carry.
"""
import json
import sys
from pathlib import Path

repo = Path(__file__).resolve().parents[1]
source = repo / "foton-registry/build_assets/particle_types.json"
out = Path(sys.argv[1]) / "org/bukkit/Particle.java"

# Vanilla payload -> the Java type Paper accepts for it. `geyser` carries one
# int (its water column height); `geyser_base` carries an int and a float, which
# no Bukkit type expresses, so it takes no data and cannot be spawned by a plugin.
DATA_TYPES = {
    "simple": "Void.class",
    "block": "org.bukkit.block.data.BlockData.class",
    "dust": "DustOptions.class",
    "dust_color_transition": "DustTransition.class",
    "spell": "Spell.class",
    "color": "Color.class",
    "item": "org.bukkit.inventory.ItemStack.class",
    "vibration": "Vibration.class",
    "power": "Float.class",
    "sculk_charge": "Float.class",
    "shriek": "Integer.class",
    "trail": "Trail.class",
    "geyser": "Integer.class",
    "geyser_base": "Void.class",
}

particles = sorted(json.loads(source.read_text()), key=lambda entry: entry["id"])
if not particles:
    raise SystemExit(f"no particles found in {source}")
unknown = sorted({p["options_type"] for p in particles} - DATA_TYPES.keys())
if unknown:
    raise SystemExit(f"particle payloads with no Bukkit data type: {unknown}")

constants = []
for particle in particles:
    path = particle["key"].split(":", 1)[1]
    constants.append(
        f'    {path.upper()}("{path}", {DATA_TYPES[particle["options_type"]]}, '
        f'{"true" if particle["options_type"] == "geyser_base" else "false"})'
    )

java = """package org.bukkit;

/** A vanilla particle, generated from Foton's particle registry. */
public enum Particle implements Keyed {
""" + ",\n".join(constants) + """;

    private final NamespacedKey key;
    private final Class<?> dataType;
    private final boolean unspawnable;

    Particle(String key, Class<?> dataType, boolean unspawnable) {
        this.key = NamespacedKey.minecraft(key);
        this.dataType = dataType;
        this.unspawnable = unspawnable;
    }

    /** The type {@code spawnParticle} expects as data, {@code Void} for none. */
    public Class<?> getDataType() { return dataType; }

    @Override public NamespacedKey getKey() { return key; }

    /** Whether vanilla's payload for this particle has no Bukkit counterpart. */
    public boolean isUnspawnable() { return unspawnable; }

    public static class DustOptions {
        private final Color color;
        private final float size;

        public DustOptions(Color color, float size) {
            if (color == null) throw new IllegalArgumentException("color");
            this.color = color;
            this.size = size;
        }

        public Color getColor() { return color; }
        public float getSize() { return size; }
    }

    public static class DustTransition extends DustOptions {
        private final Color toColor;

        public DustTransition(Color fromColor, Color toColor, float size) {
            super(fromColor, size);
            if (toColor == null) throw new IllegalArgumentException("toColor");
            this.toColor = toColor;
        }

        public Color getToColor() { return toColor; }
    }

    public static class Trail {
        private final Location target;
        private final Color color;
        private final int duration;

        public Trail(Location target, Color color, int duration) {
            this.target = target;
            this.color = color;
            this.duration = duration;
        }

        public Location getTarget() { return target; }
        public Color getColor() { return color; }
        public int getDuration() { return duration; }
    }

    public static class Spell {
        private final Color color;
        private final float power;

        public Spell(Color color, float power) {
            this.color = color;
            this.power = power;
        }

        public Color getColor() { return color; }
        public float getPower() { return power; }
    }
}
"""
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(java, encoding="utf-8")
