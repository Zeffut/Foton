package org.bukkit;

/** How a player is playing. */
public enum GameMode {
    CREATIVE,
    SURVIVAL,
    ADVENTURE,
    SPECTATOR;

    /** Reads the name Foton uses, which is the lower-case one. */
    public static GameMode byName(String name) {
        if (name == null) {
            return null;
        }
        try {
            return valueOf(name.toUpperCase(java.util.Locale.ROOT));
        } catch (IllegalArgumentException unknown) {
            return null;
        }
    }
}
