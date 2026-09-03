package org.bukkit;

/** Particle types used by the plugin API. */
public enum Particle implements Keyed {
    DUST, BLOCK, SWEEP_ATTACK, WAX_OFF, WAX_ON;

    /** Common Bukkit compatibility alias. */
    public static final Particle ENCHANT = DUST;
    @Override public NamespacedKey getKey() { return NamespacedKey.minecraft(name().toLowerCase(java.util.Locale.ROOT)); }
    public static final class DustOptions {
        private final Color color; private final float size;
        public DustOptions(Color color, float size) { this.color = color; this.size = size; }
        public Color getColor() { return color; } public float getSize() { return size; }
    }
}
