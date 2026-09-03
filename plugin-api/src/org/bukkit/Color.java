package org.bukkit;

/** Immutable RGB color. */
public final class Color {
    private final int rgb;
    public static final Color WHITE = new Color(0xFFFFFF);
    public static final Color SILVER = new Color(0xC0C0C0);
    public static final Color GRAY = new Color(0x808080);
    public static final Color BLACK = new Color(0x000000);
    public static final Color RED = new Color(0xFF0000);
    public static final Color MAROON = new Color(0x800000);
    public static final Color YELLOW = new Color(0xFFFF00);
    public static final Color OLIVE = new Color(0x808000);
    public static final Color LIME = new Color(0x00FF00);
    public static final Color GREEN = new Color(0x008000);
    public static final Color AQUA = new Color(0x00FFFF);
    public static final Color TEAL = new Color(0x008080);
    public static final Color BLUE = new Color(0x0000FF);
    public static final Color NAVY = new Color(0x000080);
    public static final Color FUCHSIA = new Color(0xFF00FF);
    public static final Color PURPLE = new Color(0x800080);
    private Color(int rgb) { this.rgb = rgb & 0xFFFFFF; }
    public static Color fromRGB(int rgb) { return new Color(rgb); }
    public static Color fromRGB(int red, int green, int blue) {
        if ((red | green | blue) < 0 || red > 255 || green > 255 || blue > 255)
            throw new IllegalArgumentException("Color components must be between 0 and 255");
        return new Color((red << 16) | (green << 8) | blue);
    }
    public int getRed() { return (rgb >> 16) & 255; }
    public int getGreen() { return (rgb >> 8) & 255; }
    public int getBlue() { return rgb & 255; }
    public int asRGB() { return rgb; }
    public static Color deserialize(java.util.Map<String, Object> map) {
        if (map == null) return null;
        Object red = map.get("RED"), green = map.get("GREEN"), blue = map.get("BLUE");
        if (!(red instanceof Number) || !(green instanceof Number) || !(blue instanceof Number)) return null;
        return fromRGB(((Number) red).intValue(), ((Number) green).intValue(), ((Number) blue).intValue());
    }
    @Override public boolean equals(Object other) { return other instanceof Color c && rgb == c.rgb; }
    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> map = new java.util.LinkedHashMap<>();
        map.put("ALPHA", 255);
        map.put("RED", getRed());
        map.put("GREEN", getGreen());
        map.put("BLUE", getBlue());
        return map;
    }
    @Override public int hashCode() { return rgb; }
}
