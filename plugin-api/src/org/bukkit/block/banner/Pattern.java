package org.bukkit.block.banner;

import org.bukkit.DyeColor;

/** One banner pattern layer. */
public final class Pattern {
    private final DyeColor color;
    private final PatternType pattern;
    public Pattern(DyeColor color, PatternType pattern) { this.color = color; this.pattern = pattern; }
    public Pattern(java.util.Map<String, Object> serialized) {
        this(parseColor(serialized == null ? null : serialized.get("color")),
            serialized == null ? null : PatternType.getByIdentifier(String.valueOf(serialized.get("pattern"))));
    }
    private static DyeColor parseColor(Object value) {
        if (value == null) return null;
        try { return DyeColor.valueOf(String.valueOf(value).toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    public DyeColor getColor() { return color; }
    public PatternType getPattern() { return pattern; }
    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> result = new java.util.LinkedHashMap<>();
        result.put("color", color == null ? null : color.name());
        result.put("pattern", pattern == null ? null : pattern.getIdentifier());
        return result;
    }
    @Override public boolean equals(Object other) {
        return other instanceof Pattern that && color == that.color && java.util.Objects.equals(pattern, that.pattern);
    }
    @Override public int hashCode() { return java.util.Objects.hash(color, pattern); }
    @Override public String toString() { return "Pattern{color=" + color + ", pattern=" + pattern + "}"; }
}
