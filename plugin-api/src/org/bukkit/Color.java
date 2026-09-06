package org.bukkit;

/** Immutable ARGB color.
 *
 * <p>Alpha is carried rather than assumed. The class used to store twenty-four
 * bits and have {@code serialize} write a constant {@code "ALPHA", 255}, which
 * round-trips only for colors that happen to be opaque -- and map colors and
 * text display backgrounds, the two places plugins reach for
 * {@link #fromARGB(int)}, are exactly the ones that are not. */
public final class Color {
    private final int argb;
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

    /** Opaque, from twenty-four bits of color. */
    private Color(int rgb) { this.argb = 0xFF000000 | (rgb & 0xFFFFFF); }

    private Color(int alpha, int rgb) {
        this.argb = ((alpha & 255) << 24) | (rgb & 0xFFFFFF);
    }

    public static Color fromRGB(int rgb) { return new Color(rgb); }

    public static Color fromRGB(int red, int green, int blue) {
        if ((red | green | blue) < 0 || red > 255 || green > 255 || blue > 255)
            throw new IllegalArgumentException("Color components must be between 0 and 255");
        return new Color((red << 16) | (green << 8) | blue);
    }

    /** From thirty-two bits, alpha in the high byte. */
    public static Color fromARGB(int argb) {
        return new Color(argb >>> 24, argb);
    }

    public static Color fromARGB(int alpha, int red, int green, int blue) {
        if ((alpha | red | green | blue) < 0 || alpha > 255 || red > 255 || green > 255 || blue > 255)
            throw new IllegalArgumentException("Color components must be between 0 and 255");
        return new Color(alpha, (red << 16) | (green << 8) | blue);
    }

    /** From thirty-two bits with the alpha byte last. */
    public static Color fromBGR(int bgr) {
        return fromRGB((bgr & 255) << 16 | (bgr >> 8 & 255) << 8 | (bgr >> 16 & 255));
    }

    public int getAlpha() { return (argb >> 24) & 255; }
    public int getRed() { return (argb >> 16) & 255; }
    public int getGreen() { return (argb >> 8) & 255; }
    public int getBlue() { return argb & 255; }

    /** Twenty-four bits, alpha dropped -- what {@code asRGB} has always meant. */
    public int asRGB() { return argb & 0xFFFFFF; }

    public int asARGB() { return argb; }

    public int asBGR() {
        return (getBlue() << 16) | (getGreen() << 8) | getRed();
    }

    /** A copy with a different alpha. Immutable, so this returns rather than
     * mutates, which is also what Bukkit's signature says. */
    public Color setAlpha(int alpha) {
        if (alpha < 0 || alpha > 255)
            throw new IllegalArgumentException("Alpha must be between 0 and 255");
        return new Color(alpha, argb);
    }

    public Color setRed(int red) { return fromARGB(getAlpha(), red, getGreen(), getBlue()); }
    public Color setGreen(int green) { return fromARGB(getAlpha(), getRed(), green, getBlue()); }
    public Color setBlue(int blue) { return fromARGB(getAlpha(), getRed(), getGreen(), blue); }

    public static Color deserialize(java.util.Map<String, Object> map) {
        if (map == null) return null;
        Object red = map.get("RED"), green = map.get("GREEN"), blue = map.get("BLUE");
        if (!(red instanceof Number) || !(green instanceof Number) || !(blue instanceof Number)) return null;
        // A map written before alpha existed has no ALPHA key, and the color
        // it described was opaque.
        Object alpha = map.get("ALPHA");
        int alphaValue = alpha instanceof Number number ? number.intValue() : 255;
        return fromARGB(alphaValue, ((Number) red).intValue(), ((Number) green).intValue(),
                ((Number) blue).intValue());
    }

    @Override public boolean equals(Object other) { return other instanceof Color c && argb == c.argb; }

    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> map = new java.util.LinkedHashMap<>();
        map.put("ALPHA", getAlpha());
        map.put("RED", getRed());
        map.put("GREEN", getGreen());
        map.put("BLUE", getBlue());
        return map;
    }

    @Override public int hashCode() { return argb; }

    @Override public String toString() {
        return "Color:[argb=0x" + Integer.toHexString(argb).toUpperCase(java.util.Locale.ROOT) + "]";
    }
}
