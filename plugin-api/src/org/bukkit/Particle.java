package org.bukkit;

/** Particle types used by the plugin API. */
public enum Particle {
    DUST;
    public static final class DustOptions {
        private final Color color; private final float size;
        public DustOptions(Color color, float size) { this.color = color; this.size = size; }
        public Color getColor() { return color; } public float getSize() { return size; }
    }
}
