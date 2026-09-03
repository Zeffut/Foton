package org.bukkit;

/** Vanilla world generator type identifiers. */
public enum WorldType {
    NORMAL, FLAT, LARGE_BIOMES, AMPLIFIED, CUSTOMIZED, BUFFET, DEFAULT_1_1;

    public String getName() { return name(); }
    public static WorldType getByName(String value) {
        if (value == null) return null;
        for (WorldType type : values()) if (type.name().equalsIgnoreCase(value)) return type;
        return null;
    }
}
