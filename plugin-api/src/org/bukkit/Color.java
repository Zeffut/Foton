package org.bukkit;

/** Immutable RGB color. */
public final class Color {
    private final int rgb;
    private Color(int rgb) { this.rgb = rgb & 0xFFFFFF; }
    public static Color fromRGB(int rgb) { return new Color(rgb); }
    public int getRed() { return (rgb >> 16) & 255; }
    public int getGreen() { return (rgb >> 8) & 255; }
    public int getBlue() { return rgb & 255; }
    public int asRGB() { return rgb; }
    @Override public boolean equals(Object other) { return other instanceof Color c && rgb == c.rgb; }
    @Override public int hashCode() { return rgb; }
}
