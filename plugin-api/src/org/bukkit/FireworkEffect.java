package org.bukkit;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Immutable description of one firework explosion. */
public final class FireworkEffect {
    public enum Type { BALL, BALL_LARGE, STAR, CREEPER, BURST }
    private final Type type;
    private final List<Color> colors, fades;
    private final boolean flicker, trail;
    private FireworkEffect(Builder builder) {
        type = builder.type;
        colors = List.copyOf(builder.colors);
        fades = List.copyOf(builder.fades);
        flicker = builder.flicker;
        trail = builder.trail;
    }
    public static Builder builder() { return new Builder(); }
    public Type getType() { return type; }
    public List<Color> getColors() { return colors; }
    public List<Color> getFadeColors() { return fades; }
    public boolean hasFlicker() { return flicker; }
    public boolean hasTrail() { return trail; }
    public java.util.Map<String, Object> serialize() {
        java.util.Map<String, Object> values = new java.util.LinkedHashMap<>();
        values.put("type", type.name());
        values.put("colors", colors.stream().map(Color::asRGB).toList());
        values.put("fade-colors", fades.stream().map(Color::asRGB).toList());
        values.put("flicker", flicker); values.put("trail", trail);
        return values;
    }
    public static FireworkEffect deserialize(java.util.Map<String, Object> values) {
        Builder builder = builder();
        if (values == null) return builder.build();
        if (values.get("type") instanceof String value) try { builder.with(Type.valueOf(value)); } catch (IllegalArgumentException ignored) { }
        if (values.get("colors") instanceof java.util.List<?> list) for (Object value : list) if (value instanceof Number n) builder.withColor(Color.fromRGB(n.intValue()));
        if (values.get("fade-colors") instanceof java.util.List<?> list) for (Object value : list) if (value instanceof Number n) builder.withFade(Color.fromRGB(n.intValue()));
        builder.flicker(Boolean.TRUE.equals(values.get("flicker"))).trail(Boolean.TRUE.equals(values.get("trail")));
        return builder.build();
    }
    @Override public boolean equals(Object other) {
        return other instanceof FireworkEffect effect && type == effect.type && colors.equals(effect.colors)
            && fades.equals(effect.fades) && flicker == effect.flicker && trail == effect.trail;
    }
    @Override public int hashCode() { return java.util.Objects.hash(type, colors, fades, flicker, trail); }
    public static final class Builder {
        private Type type = Type.BALL;
        private final List<Color> colors = new ArrayList<>(), fades = new ArrayList<>();
        private boolean flicker, trail;
        public Builder with(Type value) { type = value == null ? Type.BALL : value; return this; }
        public Builder withColor(Color value) { if (value != null) colors.add(value); return this; }
        public Builder withColor(Color... values) { if (values != null) for (Color value : values) withColor(value); return this; }
        public Builder withColor(List<Color> values) { if (values != null) for (Color value : values) withColor(value); return this; }
        public Builder withColor(Iterable<Color> values) { if (values != null) for (Color value : values) withColor(value); return this; }
        public Builder withFade(Color value) { if (value != null) fades.add(value); return this; }
        public Builder withFade(Color... values) { if (values != null) for (Color value : values) withFade(value); return this; }
        public Builder withFade(List<Color> values) { if (values != null) for (Color value : values) withFade(value); return this; }
        public Builder withFade(Iterable<Color> values) { if (values != null) for (Color value : values) withFade(value); return this; }
        public Builder flicker(boolean value) { flicker = value; return this; }
        public Builder trail(boolean value) { trail = value; return this; }
        public FireworkEffect build() { return new FireworkEffect(this); }
    }
}
